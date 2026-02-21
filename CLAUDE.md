# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**wt-cli** is a Rust CLI tool (`wt`) for managing git worktrees, featuring both a TUI dashboard (Ratatui) and direct subcommands. Binary name is `wt`, Rust edition 2021.

## Commands

```bash
# Build
cargo build                    # debug build → ./target/debug/wt
cargo build --release          # release build → ./target/release/wt

# Test
cargo test                     # all tests
cargo test <test_name>         # single test, e.g. cargo test default_config_has_env_patterns
cargo test -- --nocapture      # with stdout

# Lint & Format
cargo clippy                   # lint (all=deny, pedantic=warn)
cargo fmt                      # format (group_imports = "StdExternalCrate")
```

## Architecture

Two execution modes in `main.rs`:
- **No subcommand** → launches TUI dashboard (`tui::run_dashboard`), renders to stderr, outputs switch path to stdout on exit
- **With subcommand** → dispatches to `commands::*` modules via `run_command()`

### Module Map

- `cli.rs` — Clap 4 derive-based argument parsing, defines `Command` enum
- `config.rs` — Loads `~/.config/wt/config.toml` (env_patterns, auto_copy_env, default_base) with serde defaults
- `git.rs` — Core module. `GitContext` discovers repo root, manages `.worktrees/` directory. `Worktree` struct parsed from `git worktree list --porcelain`. All git/gh subprocess calls go through `run_git()`/`run_gh()`
- `env.rs` — Recursive env file discovery with glob matching, skips noise dirs (node_modules, .git, target, etc.), preserves nested paths during copy
- `commands/` — One file per subcommand (list, status, new, remove, switch, env_copy, pr, merge), each exports `run()`
- `tui/mod.rs` — `App` struct holds state, `AppMode` enum (Normal, ConfirmDelete, ConfirmForceDelete, NewInput, PrInput)
- `tui/ui.rs` — Ratatui layout: header | body (40/60 split list+detail) | env bar | footer
- `tui/input.rs` — Keyboard event handler, mode-dependent key dispatch

### Key Patterns

- Shell integration: `wt switch` outputs ONLY the path to stdout; all messages go to stderr. This enables `cd "$(wt switch)"`.
- Worktrees live in `<repo>/.worktrees/<sanitized-branch>/` — slashes in branch names become hyphens.
- Main worktree is protected from deletion and risky operations.
- Error handling: `GitError` (thiserror) for git-specific errors, `anyhow::Result` everywhere else.
- Clippy is strict: `all = deny`, `redundant_clone = deny`, `pedantic = warn`.

## Conventions

- Conventional commits: `feat:`, `fix:`, etc.
- Imports grouped: std → external → crate (enforced by rustfmt)
- Each command module follows the pattern: `pub fn run(ctx: &GitContext, ...) -> Result<()>`
