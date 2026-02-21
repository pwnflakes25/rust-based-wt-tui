use anyhow::Result;

use crate::config::Config;
use crate::env::copy_env_files;
use crate::git::GitContext;

pub fn run(
    ctx: &GitContext,
    config: &Config,
    branch: &str,
    base: Option<&str>,
    copy_env: bool,
) -> Result<()> {
    let base_branch = if let Some(b) = base {
        b.to_owned()
    } else {
        // Use config default, or detect
        let default = &config.default_base;
        if default == "main" || default == "master" {
            ctx.detect_default_branch()
        } else {
            default.clone()
        }
    };

    if ctx.branch_exists(branch) {
        eprintln!("Branch '{branch}' exists, creating worktree from it...");
    } else {
        eprintln!("Creating worktree for new branch '{branch}' from '{base_branch}'...");
    }

    let worktree_path = ctx.create_worktree(branch, &base_branch)?;

    eprintln!("Worktree created at: {}", worktree_path.display());

    // Copy env if requested or auto_copy_env
    let should_copy = copy_env || config.auto_copy_env;
    if should_copy {
        let current = GitContext::current_worktree_path()?;
        let copied = copy_env_files(&current, &worktree_path, &config.env_patterns)?;
        if copied.is_empty() {
            eprintln!("No .env files found to copy.");
        } else {
            eprintln!("Copied env files: {}", copied.join(", "));
        }
    }

    eprintln!("\nTo switch to this worktree:");
    eprintln!("  cd {}", worktree_path.display());

    Ok(())
}
