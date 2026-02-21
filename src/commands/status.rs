use anyhow::Result;

use crate::config::Config;
use crate::env::find_env_files;
use crate::git::GitContext;

pub fn run(ctx: &GitContext, config: &Config) -> Result<()> {
    let current_path = GitContext::current_worktree_path()?;
    let worktrees = ctx.list_worktrees()?;

    let current = worktrees
        .iter()
        .find(|wt| wt.path == current_path)
        .ok_or_else(|| anyhow::anyhow!("Current directory is not a known worktree"))?;

    let dirty = ctx.is_worktree_dirty(&current.path)?;
    let (ahead, behind) = ctx.ahead_behind(&current.path)?;

    println!("Worktree:  {}", current.display_name());
    println!("Path:      {}", current.path.display());
    println!("HEAD:      {}", &current.head[..7.min(current.head.len())]);
    println!(
        "Status:    {}",
        if dirty { "dirty" } else { "clean" }
    );

    if ahead > 0 || behind > 0 {
        println!("Tracking:  {ahead} ahead, {behind} behind");
    }

    if current.is_main {
        println!("Type:      main worktree");
    }

    // Show env files
    let env_files = find_env_files(&current.path, &config.env_patterns)?;
    if env_files.is_empty() {
        println!("Env files: (none)");
    } else {
        println!("Env files: {}", env_files.join(", "));
    }

    Ok(())
}
