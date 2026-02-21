use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

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

/// Context for all git operations, anchored to a repo root.
#[derive(Debug)]
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
    pub fn list_worktrees(&self) -> Result<Vec<Worktree>> {
        // Prune stale worktrees first
        let _ = run_git(&["worktree", "prune"], Some(&self.repo_root));

        let output = run_git(
            &["worktree", "list", "--porcelain"],
            Some(&self.repo_root),
        )?;
        Ok(parse_porcelain(&output, &self.repo_root))
    }

    /// Find a worktree by branch name or directory name.
    pub fn find_worktree(&self, name: &str) -> Result<Worktree> {
        let worktrees = self.list_worktrees()?;
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

    /// Create a new worktree.
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

        run_git(
            &[
                "worktree",
                "add",
                "-b",
                branch,
                worktree_path.to_str().unwrap_or_default(),
                base,
            ],
            Some(&self.repo_root),
        )?;

        Ok(worktree_path)
    }

    /// Remove a worktree.
    pub fn remove_worktree(&self, name: &str, force: bool) -> Result<()> {
        let wt = self.find_worktree(name)?;

        if wt.is_main {
            anyhow::bail!("Cannot remove the main worktree");
        }

        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        let path_str = wt.path.to_string_lossy().to_string();
        args.push(&path_str);

        run_git(&args, Some(&self.repo_root))?;

        // Also delete the branch
        if let Some(branch) = &wt.branch {
            let _ = run_git(&["branch", "-D", branch], Some(&self.repo_root));
        }

        Ok(())
    }

    /// Check if a worktree has uncommitted changes.
    #[allow(clippy::unused_self)]
    pub fn is_worktree_dirty(&self, path: &Path) -> Result<bool> {
        let output = run_git(
            &["status", "--porcelain"],
            Some(path),
        )?;
        Ok(!output.trim().is_empty())
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

    /// Get ahead/behind counts relative to upstream.
    #[allow(clippy::unused_self)]
    pub fn ahead_behind(&self, path: &Path) -> Result<(u32, u32)> {
        let branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], Some(path))?;
        let branch = branch.trim();

        let upstream = format!("origin/{branch}");
        let result = run_git(
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("{branch}...{upstream}"),
            ],
            Some(path),
        );

        match result {
            Ok(output) => {
                let parts: Vec<&str> = output.trim().split('\t').collect();
                if parts.len() == 2 {
                    let ahead = parts[0].parse().unwrap_or(0);
                    let behind = parts[1].parse().unwrap_or(0);
                    Ok((ahead, behind))
                } else {
                    Ok((0, 0))
                }
            }
            Err(_) => Ok((0, 0)), // No upstream tracking
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
        });
    }

    // Mark first non-bare as main if none marked
    let _ = repo_root; // used for context only
    worktrees
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        assert_eq!(wt.display_name(), "detached-head");
    }
}
