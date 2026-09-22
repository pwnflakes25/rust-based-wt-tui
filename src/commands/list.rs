use anyhow::Result;

use crate::cli::SortKey;
use crate::config::Config;
use crate::git::{format_age, sort_worktrees, GitContext};

pub fn run(
    ctx: &GitContext,
    config: &Config,
    sort: Option<SortKey>,
    reverse: bool,
) -> Result<()> {
    let mut worktrees = ctx.list_worktrees()?;

    if worktrees.is_empty() {
        println!("No worktrees found.");
        return Ok(());
    }

    let mode = match sort {
        Some(key) => key.to_mode(reverse),
        None if reverse => config.sort_mode().reversed(),
        None => config.sort_mode(),
    };
    sort_worktrees(&mut worktrees, mode);

    let ages: Vec<String> = worktrees
        .iter()
        .map(|wt| format_age(wt.created))
        .collect();

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

    let max_age = ages
        .iter()
        .map(|age| age.chars().count())
        .max()
        .unwrap_or(3)
        .max(3);

    // Header
    println!(
        "{:<width_n$}  {:<width_p$}  {:<7}  {:>width_a$}",
        "NAME",
        "PATH",
        "HEAD",
        "AGE",
        width_n = max_name + 2,
        width_p = max_path,
        width_a = max_age,
    );
    println!(
        "{:-<width_n$}  {:-<width_p$}  {:-<7}  {:-<width_a$}",
        "",
        "",
        "",
        "",
        width_n = max_name + 2,
        width_p = max_path,
        width_a = max_age,
    );

    let current_path = GitContext::current_worktree_path().ok();

    for (wt, age) in worktrees.iter().zip(&ages) {
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
            "{:<width_n$}  {:<width_p$}  {:<7}  {:>width_a$}",
            name,
            wt.path.display(),
            short_head,
            age,
            width_n = max_name + 2,
            width_p = max_path,
            width_a = max_age,
        );
    }

    Ok(())
}
