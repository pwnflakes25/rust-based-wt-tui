use anyhow::Result;

use crate::config::Config;
use crate::env::{copy_env_files, find_env_files};
use crate::git::GitContext;

pub fn run(
    ctx: &GitContext,
    config: &Config,
    source: &str,
    target: Option<&str>,
) -> Result<()> {
    let source_wt = ctx.find_worktree(source)?;

    let target_path = match target {
        Some(name) => ctx.find_worktree(name)?.path,
        None => GitContext::current_worktree_path()?,
    };

    let available = find_env_files(&source_wt.path, &config.env_patterns)?;
    if available.is_empty() {
        eprintln!("No .env files found in '{source}'.");
        return Ok(());
    }

    eprintln!(
        "Copying from '{}' to '{}':",
        source_wt.display_name(),
        target_path.display()
    );

    let copied = copy_env_files(&source_wt.path, &target_path, &config.env_patterns)?;
    for file in &copied {
        eprintln!("  {file}");
    }
    eprintln!("{} file(s) copied.", copied.len());

    Ok(())
}
