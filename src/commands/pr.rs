use anyhow::{Context, Result};

use crate::config::Config;
use crate::env::copy_env_files;
use crate::git::{run_gh, GitContext, GitError};

pub fn run(ctx: &GitContext, config: &Config, number: u64) -> Result<()> {
    // Verify gh is installed
    run_gh(&["--version"], None).map_err(|_| GitError::GhNotInstalled)?;

    eprintln!("Fetching PR #{number}...");

    // Get PR branch name
    let branch = run_gh(
        &[
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "headRefName",
            "-q",
            ".headRefName",
        ],
        Some(&ctx.repo_root),
    )
    .context("Failed to fetch PR info")?;

    let branch = branch.trim();
    eprintln!("PR #{number} is on branch '{branch}'");

    // Fetch the branch
    let _ = crate::git::run_git(
        &["fetch", "origin", &format!("{branch}:{branch}")],
        Some(&ctx.repo_root),
    );

    // Create worktree from that branch (don't create new branch, use existing)
    let dir_name = crate::git::sanitize_branch_name(branch);
    let worktree_path = ctx.worktrees_dir.join(&dir_name);

    std::fs::create_dir_all(&ctx.worktrees_dir)
        .context("Failed to create .worktrees directory")?;

    ctx.ensure_gitignore()?;

    crate::git::run_git(
        &[
            "worktree",
            "add",
            worktree_path.to_str().unwrap_or_default(),
            branch,
        ],
        Some(&ctx.repo_root),
    )?;

    eprintln!("Worktree created at: {}", worktree_path.display());

    // Copy env if auto
    if config.auto_copy_env {
        let current = GitContext::current_worktree_path()?;
        let copied = copy_env_files(&current, &worktree_path, &config.env_patterns)?;
        if !copied.is_empty() {
            eprintln!("Copied env files: {}", copied.join(", "));
        }
    }

    eprintln!("\nTo switch to this worktree:");
    eprintln!("  cd {}", worktree_path.display());

    Ok(())
}
