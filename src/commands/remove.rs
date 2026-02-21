use anyhow::Result;
use dialoguer::FuzzySelect;

use crate::git::GitContext;

pub fn run(ctx: &GitContext, name: Option<&str>, force: bool) -> Result<()> {
    let target_name = if let Some(n) = name {
        n.to_owned()
    } else {
        // Interactive select
        let worktrees = ctx.list_worktrees()?;
        let removable: Vec<_> = worktrees
            .iter()
            .filter(|wt| !wt.is_main)
            .collect();

        if removable.is_empty() {
            eprintln!("No removable worktrees (main worktree cannot be removed).");
            return Ok(());
        }

        let names: Vec<String> = removable
            .iter()
            .map(|wt| {
                let dirty = ctx.is_worktree_dirty(&wt.path).unwrap_or(false);
                let marker = if dirty { " (dirty)" } else { "" };
                format!("{}{marker}", wt.display_name())
            })
            .collect();

        let selection = FuzzySelect::new()
            .with_prompt("Select worktree to remove")
            .items(&names)
            .default(0)
            .interact()?;

        removable[selection].display_name()
    };

    eprintln!("Removing worktree '{target_name}'...");
    ctx.remove_worktree(&target_name, force)?;
    eprintln!("Worktree '{target_name}' removed.");

    Ok(())
}
