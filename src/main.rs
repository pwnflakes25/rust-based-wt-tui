mod cli;
mod commands;
mod config;
mod env;
mod git;
mod tui;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};
use config::Config;
use git::GitContext;

fn main() -> Result<()> {
    let args = Cli::parse();
    let config = Config::load();

    match args.command {
        None => {
            // Launch TUI dashboard
            let ctx = GitContext::discover()?;
            if let Some(path) = tui::run_dashboard(ctx, config)? {
                // Print switch path to stdout after terminal is restored
                println!("{path}");
            }
        }
        Some(cmd) => {
            let ctx = GitContext::discover()?;
            run_command(cmd, &ctx, &config)?;
        }
    }

    Ok(())
}

fn run_command(cmd: Command, ctx: &GitContext, config: &Config) -> Result<()> {
    match cmd {
        Command::List => commands::list::run(ctx),
        Command::Status => commands::status::run(ctx, config),
        Command::New {
            branch,
            base,
            copy_env,
        } => commands::new::run(ctx, config, &branch, base.as_deref(), copy_env),
        Command::Remove { name, force } => {
            commands::remove::run(ctx, name.as_deref(), force)
        }
        Command::Switch { name } => commands::switch::run(ctx, name.as_deref()),
        Command::Env { source, target } => {
            commands::env_copy::run(ctx, config, &source, target.as_deref())
        }
        Command::Pr { number } => commands::pr::run(ctx, config, number),
        Command::Merge {
            branch,
            delete,
            no_delete,
        } => commands::merge::run(ctx, &branch, delete, no_delete),
        Command::Init => {
            print_init_snippet();
            Ok(())
        }
    }
}

fn print_init_snippet() {
    println!(
        r#"# Add to your .zshrc or .bashrc:
wts() {{
  local dir
  dir="$(wt switch "$@")" && cd "$dir"
}}"#
    );
}
