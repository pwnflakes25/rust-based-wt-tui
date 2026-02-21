use anyhow::Result;
use dialoguer::FuzzySelect;

use crate::git::GitContext;

/// Prints the worktree path to stdout. All other output goes to stderr.
pub fn run(ctx: &GitContext, name: Option<&str>) -> Result<()> {
    let target_name = if let Some(n) = name {
        n.to_owned()
    } else {
        // Interactive select on stderr
        let worktrees = ctx.list_worktrees()?;

        if worktrees.is_empty() {
            anyhow::bail!("No worktrees found.");
        }

        let current_path = GitContext::current_worktree_path().ok();

        let names: Vec<String> = worktrees
            .iter()
            .map(|wt| {
                let marker = if current_path.as_ref() == Some(&wt.path) {
                    " *"
                } else {
                    ""
                };
                format!("{}{marker}", wt.display_name())
            })
            .collect();

        let selection = FuzzySelect::new()
            .with_prompt("Select worktree")
            .items(&names)
            .default(0)
            .interact()?;

        worktrees[selection].display_name()
    };

    let wt = ctx.find_worktree(&target_name)?;

    // ONLY the path goes to stdout -- critical for shell integration
    println!("{}", wt.path.display());

    Ok(())
}
