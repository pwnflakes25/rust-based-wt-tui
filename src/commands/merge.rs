use anyhow::Result;
use dialoguer::Confirm;

use crate::git::GitContext;

pub fn run(
    ctx: &GitContext,
    branch: &str,
    auto_delete: bool,
    no_delete: bool,
) -> Result<()> {
    let source_wt = ctx.find_worktree(branch)?;
    let current_path = GitContext::current_worktree_path()?;

    let source_branch = source_wt
        .branch
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Source worktree has no branch (detached HEAD)"))?;

    let current_branch = ctx.current_branch()?;

    // Safety: check source worktree for uncommitted changes
    eprintln!("Checking {source_branch} for uncommitted changes...");
    if ctx.is_worktree_dirty(&source_wt.path)? {
        anyhow::bail!(
            "Worktree '{source_branch}' has uncommitted changes. Commit or stash them first, or use --force."
        );
    }
    eprintln!("  clean");

    // Safety: check current worktree for uncommitted changes
    eprintln!("Checking current worktree ({current_branch})...");
    if ctx.is_worktree_dirty(&current_path)? {
        anyhow::bail!(
            "Current worktree ({current_branch}) has uncommitted changes. Commit or stash them first."
        );
    }
    eprintln!("  clean");

    // Perform merge
    eprintln!("Merging {source_branch} into {current_branch}...");
    let success = ctx.merge_branch(source_branch, &current_path)?;

    if success {
        eprintln!("Merge successful.");
    } else {
        eprintln!("Merge resulted in conflicts. Resolve them manually, then run:");
        eprintln!("  git add . && git commit");
        return Ok(());
    }

    // Prompt to remove source worktree
    if !no_delete {
        let should_delete = if auto_delete {
            true
        } else {
            Confirm::new()
                .with_prompt(format!("Remove worktree {source_branch}?"))
                .default(false)
                .interact()?
        };

        if should_delete {
            ctx.remove_worktree(branch, false)?;
            eprintln!("Worktree '{source_branch}' removed.");
        }
    }

    Ok(())
}
