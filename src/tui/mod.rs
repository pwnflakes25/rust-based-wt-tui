pub mod input;
pub mod ui;

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::prelude::CrosstermBackend;

use crate::config::Config;
use crate::git::GitContext;

/// Autocomplete state for branch name suggestions.
pub struct Autocomplete {
    pub candidates: Vec<String>,
    pub filtered: Vec<String>,
    pub selected: Option<usize>,
}

impl Autocomplete {
    pub fn new(candidates: Vec<String>) -> Self {
        Self {
            filtered: candidates.clone(),
            candidates,
            selected: None,
        }
    }

    pub fn update_filter(&mut self, input: &str) {
        if input.is_empty() {
            self.filtered = self.candidates.clone();
        } else {
            let lower = input.to_lowercase();
            self.filtered = self
                .candidates
                .iter()
                .filter(|c| c.to_lowercase().contains(&lower))
                .cloned()
                .collect();
        }
        // Always select first match so Tab works without needing Up/Down first
        if self.filtered.is_empty() {
            self.selected = None;
        } else {
            match self.selected {
                Some(idx) if idx >= self.filtered.len() => self.selected = Some(0),
                None => self.selected = Some(0),
                _ => {}
            }
        }
    }

    pub fn next(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            Some(i) => (i + 1) % self.filtered.len(),
            None => 0,
        });
    }

    pub fn prev(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            Some(0) | None => self.filtered.len().saturating_sub(1),
            Some(i) => i - 1,
        });
    }

    pub fn selected_value(&self) -> Option<&str> {
        self.selected.and_then(|i| self.filtered.get(i).map(String::as_str))
    }
}

/// App mode determines what keyboard input does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    ConfirmDelete,
    NewInput(String),
    /// Second step: user has entered a new branch name, now picking the base branch.
    NewBaseInput { branch: String, base: String },
    PrInput(String),
}

/// Main application state.
pub struct App {
    pub ctx: GitContext,
    pub config: Config,
    pub worktrees: Vec<crate::git::Worktree>,
    pub selected: usize,
    pub mode: AppMode,
    pub current_path: Option<std::path::PathBuf>,
    pub message: Option<String>,
    pub should_quit: bool,
    /// If set, the TUI should print this path to stdout after exiting.
    pub switch_path: Option<String>,
    /// Active autocomplete state for branch name input.
    pub autocomplete: Option<Autocomplete>,
    /// Cached dirty status per worktree path (computed on refresh, not every render).
    pub dirty_cache: HashMap<PathBuf, bool>,
    /// Cached ahead/behind counts per worktree path.
    pub ahead_behind_cache: HashMap<PathBuf, (u32, u32)>,
    /// Cached env files per worktree path.
    pub env_files_cache: HashMap<PathBuf, Vec<String>>,
    /// Paths of worktrees currently being deleted in the background.
    pub deleting_paths: HashSet<PathBuf>,
    /// Sender half of the deletion result channel (cloned into each worker thread).
    pub delete_tx: mpsc::Sender<(PathBuf, String, anyhow::Result<()>)>,
    /// Receiver half of the deletion result channel (drained each event-loop tick).
    pub delete_rx: mpsc::Receiver<(PathBuf, String, anyhow::Result<()>)>,
    /// Frame counter for the spinner animation (incremented each event-loop tick).
    pub spinner_frame: usize,
}

impl App {
    pub fn new(ctx: GitContext, config: Config) -> Result<Self> {
        // Prune on startup
        let worktrees = ctx.list_worktrees_ex(true)?;
        let current_path = GitContext::current_worktree_path().ok();
        let (delete_tx, delete_rx) = mpsc::channel();
        let mut app = Self {
            ctx,
            config,
            worktrees,
            selected: 0,
            mode: AppMode::Normal,
            current_path,
            message: None,
            should_quit: false,
            switch_path: None,
            autocomplete: None,
            dirty_cache: HashMap::new(),
            ahead_behind_cache: HashMap::new(),
            env_files_cache: HashMap::new(),
            deleting_paths: HashSet::new(),
            delete_tx,
            delete_rx,
            spinner_frame: 0,
        };
        app.update_caches();
        Ok(app)
    }

    /// Refresh worktree list and caches. Set `prune` to true for explicit refreshes
    /// (startup, user pressing 'r'), false after create/delete operations.
    pub fn refresh_ex(&mut self, prune: bool) -> Result<()> {
        self.worktrees = self.ctx.list_worktrees_ex(prune)?;
        if self.selected >= self.worktrees.len() && !self.worktrees.is_empty() {
            self.selected = self.worktrees.len() - 1;
        }
        self.update_caches();
        self.message = None;
        Ok(())
    }

    /// Convenience: refresh with prune (for backward compat and explicit refresh).
    pub fn refresh(&mut self) -> Result<()> {
        self.refresh_ex(true)
    }

    fn update_caches(&mut self) {
        self.dirty_cache.clear();
        self.ahead_behind_cache.clear();
        self.env_files_cache.clear();
        for wt in &self.worktrees {
            let dirty = self.ctx.is_worktree_dirty(&wt.path).unwrap_or(false);
            let ab = self.ctx.ahead_behind(&wt.path, wt.branch.as_deref());
            self.dirty_cache.insert(wt.path.clone(), dirty);
            self.ahead_behind_cache.insert(wt.path.clone(), ab);

            let env_files = crate::env::find_env_files(&wt.path, &self.config.env_patterns)
                .unwrap_or_default();
            self.env_files_cache.insert(wt.path.clone(), env_files);
        }
    }

    pub fn selected_worktree(&self) -> Option<&crate::git::Worktree> {
        self.worktrees.get(self.selected)
    }
}

/// Install a panic hook that restores the terminal.
fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), LeaveAlternateScreen);
        original_hook(info);
    }));
}

/// Run the TUI dashboard. Renders to stderr so stdout stays clean for switch paths.
pub fn run_dashboard(ctx: GitContext, config: Config) -> Result<Option<String>> {
    install_panic_hook();

    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(ctx, config)?;

    while !app.should_quit {
        terminal.draw(|f| ui::render(f, &app))?;

        // Poll with timeout so the UI can redraw even without input
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    input::handle_key(&mut app, key);
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(io::stderr(), LeaveAlternateScreen)?;

    Ok(app.switch_path)
}
