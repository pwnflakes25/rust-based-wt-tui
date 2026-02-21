use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "wt",
    about = "Git worktree manager with TUI dashboard",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List all worktrees (plain text table)
    List,

    /// Show current worktree info
    Status,

    /// Create a new worktree
    New {
        /// Branch name for the new worktree
        branch: String,

        /// Base branch to create from (defaults to config or main)
        #[arg(long)]
        base: Option<String>,

        /// Copy .env* files from the current worktree
        #[arg(long)]
        copy_env: bool,
    },

    /// Remove a worktree
    Remove {
        /// Name of the worktree to remove (interactive select if omitted)
        name: Option<String>,

        /// Force removal even with uncommitted changes
        #[arg(long)]
        force: bool,
    },

    /// Print cd-able path for a worktree
    Switch {
        /// Name of the worktree to switch to (interactive select if omitted)
        name: Option<String>,
    },

    /// Copy .env* files between worktrees
    Env {
        /// Source worktree name
        source: String,

        /// Target worktree name (defaults to current)
        target: Option<String>,
    },

    /// Create worktree from a GitHub PR
    Pr {
        /// PR number
        number: u64,
    },

    /// Merge a worktree branch into the current branch
    Merge {
        /// Branch/worktree name to merge
        branch: String,

        /// Auto-delete worktree after merge
        #[arg(long)]
        delete: bool,

        /// Keep worktree after merge (skip prompt)
        #[arg(long)]
        no_delete: bool,
    },

    /// Print shell integration snippet
    Init,
}
