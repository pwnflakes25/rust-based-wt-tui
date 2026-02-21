use anyhow::Result;

use crate::git::GitContext;

pub fn run(ctx: &GitContext) -> Result<()> {
    let worktrees = ctx.list_worktrees()?;

    if worktrees.is_empty() {
        println!("No worktrees found.");
        return Ok(());
    }

    // Determine column widths
    let max_name = worktrees
        .iter()
        .map(|wt| wt.display_name().len())
        .max()
        .unwrap_or(4)
        .max(4);

    let max_path = worktrees
        .iter()
        .map(|wt| wt.path.display().to_string().len())
        .max()
        .unwrap_or(4)
        .max(4);

    // Header
    println!(
        "{:<width_n$}  {:<width_p$}  HEAD",
        "NAME",
        "PATH",
        width_n = max_name,
        width_p = max_path,
    );
    println!(
        "{:-<width_n$}  {:-<width_p$}  {:-<8}",
        "",
        "",
        "",
        width_n = max_name,
        width_p = max_path,
    );

    let current_path = GitContext::current_worktree_path().ok();

    for wt in &worktrees {
        let marker = if current_path.as_ref() == Some(&wt.path) {
            " *"
        } else {
            ""
        };
        let name = format!("{}{marker}", wt.display_name());
        let short_head = if wt.head.len() > 7 {
            &wt.head[..7]
        } else {
            &wt.head
        };

        println!(
            "{:<width_n$}  {:<width_p$}  {}",
            name,
            wt.path.display(),
            short_head,
            width_n = max_name + 2,
            width_p = max_path,
        );
    }

    Ok(())
}
