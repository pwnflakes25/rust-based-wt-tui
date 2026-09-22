use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use anyhow::{Context, Result};
use git2::Repository;

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum GitError {
    #[error("Not inside a git repository")]
    NotARepo,
    #[error("Worktree '{0}' already exists")]
    WorktreeExists(String),
    #[error("Worktree '{0}' not found")]
    WorktreeNotFound(String),
    #[error("Worktree '{0}' has uncommitted changes")]
    WorktreeDirty(String),
    #[error("gh CLI not found — install from https://cli.github.com")]
    GhNotInstalled,
    #[error("git command failed: {0}")]
    CommandFailed(String),
}

/// Represents a single worktree entry from `git worktree list --porcelain`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub is_bare: bool,
    pub is_main: bool,
    pub created: Option<SystemTime>,
}

impl Worktree {
    /// Short name for display, derived from branch or path.
    pub fn display_name(&self) -> String {
        if let Some(branch) = &self.branch {
            branch.clone()
        } else {
            self.path
                .file_name()
                .map_or_else(|| self.path.display().to_string(), |n| n.to_string_lossy().into_owned())
        }
    }
}

/// Ordering applied to the worktree list before it is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    #[default]
    DateDesc,
    DateAsc,
    NameAsc,
    NameDesc,
}

impl SortMode {
    /// Next mode in the dashboard's `o` cycle.
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::DateDesc => Self::DateAsc,
            Self::DateAsc => Self::NameAsc,
            Self::NameAsc => Self::NameDesc,
            Self::NameDesc => Self::DateDesc,
        }
    }

    /// Flip the direction, keeping the key.
    #[must_use]
    pub fn reversed(self) -> Self {
        match self {
            Self::DateDesc => Self::DateAsc,
            Self::DateAsc => Self::DateDesc,
            Self::NameAsc => Self::NameDesc,
            Self::NameDesc => Self::NameAsc,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::DateDesc => "date \u{2193}",
            Self::DateAsc => "date \u{2191}",
            Self::NameAsc => "name \u{2191}",
            Self::NameDesc => "name \u{2193}",
        }
    }

    /// Parse a config value such as `date-desc`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "date-desc" => Some(Self::DateDesc),
            "date-asc" => Some(Self::DateAsc),
            "name-asc" => Some(Self::NameAsc),
            "name-desc" => Some(Self::NameDesc),
            _ => None,
        }
    }
}

/// Order worktrees for display: the main worktree is pinned first, then the
/// chosen key. Worktrees with an unknown creation time sink to the bottom in
/// both date directions, and equal keys fall back to name so the order stays
/// stable across refreshes.
pub fn sort_worktrees(worktrees: &mut [Worktree], mode: SortMode) {
    worktrees.sort_by(|a, b| {
        b.is_main
            .cmp(&a.is_main)
            .then_with(|| match mode {
                SortMode::DateDesc => compare_created(a, b, true),
                SortMode::DateAsc => compare_created(a, b, false),
                SortMode::NameAsc => name_key(a).cmp(&name_key(b)),
                SortMode::NameDesc => name_key(b).cmp(&name_key(a)),
            })
            .then_with(|| name_key(a).cmp(&name_key(b)))
    });
}

fn name_key(wt: &Worktree) -> String {
    wt.display_name().to_lowercase()
}

fn compare_created(a: &Worktree, b: &Worktree, newest_first: bool) -> Ordering {
    match (a.created, b.created) {
        (Some(x), Some(y)) => {
            if newest_first {
                y.cmp(&x)
            } else {
                x.cmp(&y)
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Compact relative age for display: `<1m`, `45m`, `2h`, `3d`, `6w`, `14mo`,
/// `2y`, or `-` when the creation time is unknown.
#[must_use]
pub fn format_age(created: Option<SystemTime>) -> String {
    let Some(created) = created else {
        return "-".to_owned();
    };

    let secs = SystemTime::now()
        .duration_since(created)
        .map_or(0, |d| d.as_secs());

    match secs {
        s if s < 60 => "<1m".to_owned(),
        s if s < 3_600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3_600),
        s if s < 604_800 => format!("{}d", s / 86_400),
        s if s < 2_592_000 => format!("{}w", s / 604_800),
        s if s < 31_536_000 => format!("{}mo", s / 2_592_000),
        s => format!("{}y", s / 31_536_000),
    }
}

/// Falls back to mtime on filesystems that don't record a birth time.
fn created_at(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    meta.created().or_else(|_| meta.modified()).ok()
}

/// Context for all git operations, anchored to a repo root.
#[derive(Debug, Clone)]
pub struct GitContext {
    pub repo_root: PathBuf,
    pub worktrees_dir: PathBuf,
}

impl GitContext {
    /// Discover the git repo root from the current directory.
    pub fn discover() -> Result<Self> {
        let output = run_git(&["rev-parse", "--show-toplevel"], None)
            .map_err(|_| GitError::NotARepo)?;
        let repo_root = PathBuf::from(output.trim());
        let worktrees_dir = repo_root.join(".worktrees");
        Ok(Self {
            repo_root,
            worktrees_dir,
        })
    }

    /// List all worktrees via `git worktree list --porcelain`.
    /// When `prune` is true, runs `git worktree prune` first to clean stale entries.
    pub fn list_worktrees_ex(&self, prune: bool) -> Result<Vec<Worktree>> {
        if prune {
            let _ = run_git(&["worktree", "prune"], Some(&self.repo_root));
        }

        let output = run_git(
            &["worktree", "list", "--porcelain"],
            Some(&self.repo_root),
        )?;
        let mut worktrees = parse_porcelain(&output, &self.repo_root);
        for wt in &mut worktrees {
            wt.created = created_at(&wt.path);
        }
        Ok(worktrees)
    }

    /// List all worktrees (with prune). Convenience wrapper for backward compat.
    pub fn list_worktrees(&self) -> Result<Vec<Worktree>> {
        self.list_worktrees_ex(true)
    }

    /// Find a worktree by branch name or directory name.
    pub fn find_worktree(&self, name: &str) -> Result<Worktree> {
        let worktrees = self.list_worktrees_ex(false)?;
        worktrees
            .into_iter()
            .find(|wt| {
                wt.branch.as_deref() == Some(name)
                    || wt.path.file_name().is_some_and(|f| f.to_string_lossy() == name)
                    || wt.path.file_name().is_some_and(|f| {
                        f.to_string_lossy() == sanitize_branch_name(name)
                    })
            })
            .ok_or_else(|| GitError::WorktreeNotFound(name.to_owned()).into())
    }

    /// Check if a local branch exists.
    pub fn branch_exists(&self, branch: &str) -> bool {
        run_git(
            &["rev-parse", "--verify", branch],
            Some(&self.repo_root),
        )
        .is_ok()
    }

    /// Create a new worktree. Auto-detects whether the branch already exists:
    /// - Existing branch: checks it out into the worktree
    /// - New branch: creates it from `base`
    pub fn create_worktree(&self, branch: &str, base: &str) -> Result<PathBuf> {
        // Ensure .worktrees dir exists
        std::fs::create_dir_all(&self.worktrees_dir)
            .context("Failed to create .worktrees directory")?;

        // Ensure .worktrees is in .gitignore
        self.ensure_gitignore()?;

        let dir_name = sanitize_branch_name(branch);
        let worktree_path = self.worktrees_dir.join(&dir_name);

        if worktree_path.exists() {
            return Err(GitError::WorktreeExists(branch.to_owned()).into());
        }

        // Check for directory name collision
        if let Ok(worktrees) = self.list_worktrees() {
            for wt in &worktrees {
                if wt.path.file_name().is_some_and(|f| f.to_string_lossy() == dir_name) {
                    return Err(GitError::WorktreeExists(branch.to_owned()).into());
                }
            }
        }

        let path_str = worktree_path.to_str().unwrap_or_default();

        if self.branch_exists(branch) {
            // Branch exists — just check it out into the worktree
            run_git(
                &["worktree", "add", path_str, branch],
                Some(&self.repo_root),
            )?;
        } else {
            // New branch — create from base
            run_git(
                &["worktree", "add", "-b", branch, path_str, base],
                Some(&self.repo_root),
            )?;
        }

        Ok(worktree_path)
    }

    /// Remove a worktree by name (looks it up first).
    pub fn remove_worktree(&self, name: &str, force: bool) -> Result<()> {
        let wt = self.find_worktree(name)?;
        self.remove_worktree_at(&wt, force)
    }

    /// Remove a worktree by reference, skipping the name-based lookup.
    pub fn remove_worktree_at(&self, wt: &Worktree, force: bool) -> Result<()> {
        if wt.is_main {
            anyhow::bail!("Cannot remove the main worktree");
        }

        let path_str = wt.path.to_string_lossy();
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&path_str);

        run_git(&args, Some(&self.repo_root))?;

        // Also delete the branch
        if let Some(branch) = &wt.branch {
            let _ = run_git(&["branch", "-D", branch], Some(&self.repo_root));
        }

        Ok(())
    }

    /// Check if a worktree has uncommitted changes using libgit2.
    #[allow(clippy::unused_self)]
    pub fn is_worktree_dirty(&self, path: &Path) -> Result<bool> {
        let repo = Repository::open(path)
            .context("Failed to open repository")?;

        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(false)
            .exclude_submodules(true);

        let statuses = repo.statuses(Some(&mut opts))?;
        Ok(!statuses.is_empty())
    }

    /// Get the current branch name.
    #[allow(clippy::unused_self)]
    pub fn current_branch(&self) -> Result<String> {
        let output = run_git(
            &["rev-parse", "--abbrev-ref", "HEAD"],
            None,
        )?;
        Ok(output.trim().to_owned())
    }

    /// Get the current worktree path.
    pub fn current_worktree_path() -> Result<PathBuf> {
        let output = run_git(
            &["rev-parse", "--show-toplevel"],
            None,
        )?;
        Ok(PathBuf::from(output.trim()))
    }

    /// Get ahead/behind counts relative to upstream using libgit2.
    /// If `branch_name` is provided, skips detecting the branch from HEAD.
    #[allow(clippy::unused_self)]
    pub fn ahead_behind(&self, path: &Path, branch_name: Option<&str>) -> (u32, u32) {
        let Ok(repo) = Repository::open(path) else {
            return (0, 0);
        };

        let branch = if let Some(name) = branch_name {
            name.to_owned()
        } else {
            match repo.head() {
                Ok(head) => head.shorthand().unwrap_or("HEAD").to_owned(),
                Err(_) => return (0, 0),
            }
        };

        let local_ref = format!("refs/heads/{branch}");
        let remote_ref = format!("refs/remotes/origin/{branch}");

        let Ok(local_oid) = repo.refname_to_id(&local_ref) else {
            return (0, 0);
        };
        let Ok(remote_oid) = repo.refname_to_id(&remote_ref) else {
            return (0, 0);
        };

        repo.graph_ahead_behind(local_oid, remote_oid)
            .map(|(a, b)| (u32::try_from(a).unwrap_or(0), u32::try_from(b).unwrap_or(0)))
            .unwrap_or((0, 0))
    }

    /// List local branch names (sorted).
    pub fn list_local_branches(&self) -> Vec<String> {
        let output = run_git(
            &["branch", "--format=%(refname:short)"],
            Some(&self.repo_root),
        );
        match output {
            Ok(s) => {
                let mut branches: Vec<String> = s
                    .lines()
                    .map(|l| l.trim().to_owned())
                    .filter(|l| !l.is_empty())
                    .collect();
                branches.sort();
                branches
            }
            Err(_) => Vec::new(),
        }
    }

    /// List all branches (local + remote), stripping `origin/` prefix, deduplicated and sorted.
    pub fn list_all_branches(&self) -> Vec<String> {
        let output = run_git(
            &["branch", "-a", "--format=%(refname:short)"],
            Some(&self.repo_root),
        );
        match output {
            Ok(s) => {
                let mut seen = std::collections::BTreeSet::new();
                for line in s.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.contains("HEAD") {
                        continue;
                    }
                    let name = trimmed.strip_prefix("origin/").unwrap_or(trimmed);
                    seen.insert(name.to_owned());
                }
                seen.into_iter().collect()
            }
            Err(_) => Vec::new(),
        }
    }

    /// Detect the default branch (main or master).
    pub fn detect_default_branch(&self) -> String {
        // Try symbolic ref first
        if let Ok(output) = run_git(
            &["symbolic-ref", "refs/remotes/origin/HEAD"],
            Some(&self.repo_root),
        ) {
            if let Some(branch) = output.trim().strip_prefix("refs/remotes/origin/") {
                return branch.to_owned();
            }
        }

        // Fall back to checking if main/master exist
        if run_git(
            &["rev-parse", "--verify", "main"],
            Some(&self.repo_root),
        )
        .is_ok()
        {
            return "main".to_owned();
        }

        "master".to_owned()
    }

    /// Merge a branch into the current branch (from a given worktree directory).
    #[allow(clippy::unused_self)]
    pub fn merge_branch(&self, branch: &str, cwd: &Path) -> Result<bool> {
        let result = run_git(&["merge", branch], Some(cwd));
        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("CONFLICT") || err_str.contains("conflict") {
                    Ok(false) // merge conflict
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Ensure .worktrees is in .gitignore.
    pub fn ensure_gitignore(&self) -> Result<()> {
        let gitignore = self.repo_root.join(".gitignore");
        let entry = ".worktrees/";

        if gitignore.exists() {
            let contents = std::fs::read_to_string(&gitignore)?;
            if contents.lines().any(|line| line.trim() == entry) {
                return Ok(());
            }
            // Append
            let mut new_contents = contents;
            if !new_contents.ends_with('\n') {
                new_contents.push('\n');
            }
            new_contents.push_str(entry);
            new_contents.push('\n');
            std::fs::write(&gitignore, new_contents)?;
        } else {
            std::fs::write(&gitignore, format!("{entry}\n"))?;
        }

        Ok(())
    }
}

/// Run a git command, returning trimmed stdout.
pub fn run_git(args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd.output().context("Failed to execute git")?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(GitError::CommandFailed(stderr).into())
    }
}

/// Run `gh` CLI command, returning trimmed stdout.
pub fn run_gh(args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let mut cmd = Command::new("gh");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd.output().map_err(|_| GitError::GhNotInstalled)?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(GitError::CommandFailed(stderr).into())
    }
}

/// Sanitize a branch name for use as a directory name.
/// `feature/login` -> `feature-login`
pub fn sanitize_branch_name(name: &str) -> String {
    name.replace('/', "-")
}

/// Parse `git worktree list --porcelain` output.
fn parse_porcelain(output: &str, repo_root: &Path) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head = String::new();
    let mut current_branch: Option<String> = None;
    let mut is_bare = false;
    let mut first = true;

    for line in output.lines() {
        if line.starts_with("worktree ") {
            // Save previous worktree if any
            if let Some(path) = current_path.take() {
                let is_main = first;
                first = false;
                worktrees.push(Worktree {
                    path,
                    head: std::mem::take(&mut current_head),
                    branch: current_branch.take(),
                    is_bare,
                    is_main,
                    created: None,
                });
                is_bare = false;
            }
            current_path = Some(PathBuf::from(line.trim_start_matches("worktree ").trim()));
        } else if line.starts_with("HEAD ") {
            line.trim_start_matches("HEAD ").trim().clone_into(&mut current_head);
        } else if line.starts_with("branch ") {
            let full_ref = line.trim_start_matches("branch ").trim();
            current_branch = Some(
                full_ref
                    .strip_prefix("refs/heads/")
                    .unwrap_or(full_ref)
                    .to_owned(),
            );
        } else if line.trim() == "bare" {
            is_bare = true;
        }
    }

    // Don't forget the last entry
    if let Some(path) = current_path {
        let is_main = first;
        worktrees.push(Worktree {
            path,
            head: current_head,
            branch: current_branch,
            is_bare,
            is_main,
            created: None,
        });
    }

    // Mark first non-bare as main if none marked
    let _ = repo_root; // used for context only
    worktrees
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn fixture(name: &str, created: Option<SystemTime>, is_main: bool) -> Worktree {
        Worktree {
            path: PathBuf::from(format!("/repo/.worktrees/{name}")),
            head: "abc1234".to_owned(),
            branch: Some(name.to_owned()),
            is_bare: false,
            is_main,
            created,
        }
    }

    fn names(worktrees: &[Worktree]) -> Vec<String> {
        worktrees.iter().map(Worktree::display_name).collect()
    }

    #[test]
    fn sanitize_replaces_slashes() {
        assert_eq!(sanitize_branch_name("feature/login"), "feature-login");
        assert_eq!(sanitize_branch_name("main"), "main");
        assert_eq!(
            sanitize_branch_name("fix/auth/token"),
            "fix-auth-token"
        );
    }

    #[test]
    fn parse_porcelain_single_worktree() {
        let output = "worktree /home/user/repo\nHEAD abc123\nbranch refs/heads/main\n";
        let result = parse_porcelain(output, Path::new("/home/user/repo"));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].branch.as_deref(), Some("main"));
        assert!(result[0].is_main);
    }

    #[test]
    fn parse_porcelain_multiple_worktrees() {
        let output = "\
worktree /home/user/repo
HEAD abc123
branch refs/heads/main

worktree /home/user/repo/.worktrees/feature-login
HEAD def456
branch refs/heads/feature/login
";
        let result = parse_porcelain(output, Path::new("/home/user/repo"));
        assert_eq!(result.len(), 2);
        assert!(result[0].is_main);
        assert!(!result[1].is_main);
        assert_eq!(result[1].branch.as_deref(), Some("feature/login"));
    }

    #[test]
    fn worktree_display_name_uses_branch() {
        let wt = Worktree {
            path: PathBuf::from("/repo/.worktrees/feat-auth"),
            head: "abc".to_owned(),
            branch: Some("feat/auth".to_owned()),
            is_bare: false,
            is_main: false,
            created: None,
        };
        assert_eq!(wt.display_name(), "feat/auth");
    }

    #[test]
    fn worktree_display_name_falls_back_to_dir() {
        let wt = Worktree {
            path: PathBuf::from("/repo/.worktrees/detached-head"),
            head: "abc".to_owned(),
            branch: None,
            is_bare: false,
            is_main: false,
            created: None,
        };
        assert_eq!(wt.display_name(), "detached-head");
    }

    #[test]
    fn sort_pins_main_and_orders_by_date() {
        let now = SystemTime::now();
        let tie = now - Duration::from_secs(300);
        let mut worktrees = vec![
            fixture("old", Some(now - Duration::from_secs(600)), false),
            fixture("unknown", None, false),
            fixture("bravo", Some(tie), false),
            fixture("new", Some(now), false),
            fixture("alpha", Some(tie), false),
            fixture("main", Some(now - Duration::from_secs(9_999)), true),
        ];

        sort_worktrees(&mut worktrees, SortMode::DateDesc);
        assert_eq!(
            names(&worktrees),
            ["main", "new", "alpha", "bravo", "old", "unknown"]
        );

        sort_worktrees(&mut worktrees, SortMode::DateAsc);
        assert_eq!(
            names(&worktrees),
            ["main", "old", "alpha", "bravo", "new", "unknown"]
        );
    }

    #[test]
    fn sort_by_name_is_case_insensitive() {
        let mut worktrees = vec![
            fixture("Hotfix/log", None, false),
            fixture("chore/deps", None, false),
            fixture("feat/checkout", None, false),
        ];

        sort_worktrees(&mut worktrees, SortMode::NameAsc);
        assert_eq!(names(&worktrees), ["chore/deps", "feat/checkout", "Hotfix/log"]);

        sort_worktrees(&mut worktrees, SortMode::NameDesc);
        assert_eq!(names(&worktrees), ["Hotfix/log", "feat/checkout", "chore/deps"]);
    }

    #[test]
    fn format_age_bucket_boundaries() {
        let now = SystemTime::now();
        let age = |secs| format_age(Some(now - Duration::from_secs(secs)));

        assert_eq!(age(59), "<1m");
        assert_eq!(age(60), "1m");
        assert_eq!(age(3_600), "1h");
        assert_eq!(age(86_400), "1d");
        assert_eq!(age(604_800), "1w");
        assert_eq!(age(2_592_000), "1mo");
        assert_eq!(age(31_536_000), "1y");
        assert_eq!(format_age(None), "-");
    }
}
