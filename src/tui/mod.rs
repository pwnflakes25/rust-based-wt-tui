pub mod input;
pub mod ui;

use std::io;

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

/// App mode determines what keyboard input does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    ConfirmDelete,
    ConfirmForceDelete,
    NewInput(String),
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
}

impl App {
    pub fn new(ctx: GitContext, config: Config) -> Result<Self> {
        let worktrees = ctx.list_worktrees()?;
        let current_path = GitContext::current_worktree_path().ok();
        Ok(Self {
            ctx,
            config,
            worktrees,
            selected: 0,
            mode: AppMode::Normal,
            current_path,
            message: None,
            should_quit: false,
            switch_path: None,
        })
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.worktrees = self.ctx.list_worktrees()?;
        if self.selected >= self.worktrees.len() && !self.worktrees.is_empty() {
            self.selected = self.worktrees.len() - 1;
        }
        self.message = None;
        Ok(())
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

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                input::handle_key(&mut app, key);
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(io::stderr(), LeaveAlternateScreen)?;

    Ok(app.switch_path)
}
