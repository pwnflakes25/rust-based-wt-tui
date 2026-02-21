# wt — Git Worktree Manager

A CLI tool for managing git worktrees, featuring a TUI dashboard and direct subcommands.

## Quick Install

### Prerequisites

- [Rust toolchain](https://rustup.rs) (cargo)
- git
- [GitHub CLI](https://cli.github.com) (`gh`) — optional, needed for the `wt pr` command

### One-liner

```bash
git clone <repo-url> && cd wt-cli && ./install.sh
```

This builds the release binary and copies it to `~/.local/bin/wt`. If `~/.local/bin` is not in your PATH, the script will tell you what to add.

To install somewhere else:

```bash
INSTALL_DIR=/usr/local/bin ./install.sh
```

### Manual install

```bash
cargo build --release
cp target/release/wt ~/.local/bin/   # or anywhere on your PATH
```

### Shell integration

`wt switch` prints a path to stdout so your shell can `cd` into it. Add this helper to your `~/.zshrc` or `~/.bashrc`:

```bash
wts() {
  local dir
  dir="$(wt switch "$@")" && cd "$dir"
}
```

Or run `wt init` to print the snippet.

## Usage

Run `wt` with no arguments to launch the **TUI dashboard**, or use subcommands directly:

```
wt              # open interactive TUI dashboard
wt list         # list all worktrees
wt status       # show current worktree info
wt new <branch> # create a new worktree
wt switch       # interactively pick a worktree to switch to
wt remove       # interactively pick a worktree to remove
wt env <src>    # copy .env files from another worktree
wt pr <number>  # create a worktree from a GitHub PR
wt merge <br>   # merge a worktree branch into the current branch
wt init         # print shell integration snippet
```

### Creating worktrees

```bash
wt new feature/login              # branch off the default base (main)
wt new feature/login --base dev   # branch off dev
wt new feature/login --copy-env   # also copy .env files from current worktree
```

### Switching

```bash
wts                    # interactive fuzzy select, then cd
wts feature/login      # cd directly
```

### Removing

```bash
wt remove                    # interactive select
wt remove feature/login      # by name
wt remove feature/login --force  # force even with uncommitted changes
```

### Merging

```bash
wt merge feature/login            # merge and prompt to delete worktree
wt merge feature/login --delete   # merge and auto-delete
wt merge feature/login --no-delete  # merge and keep worktree
```

### Env file copying

```bash
wt env main              # copy .env files from "main" worktree into current
wt env main feature/login  # copy from main into feature/login
```

## Configuration

Optional config file at `~/.config/wt/config.toml`:

```toml
# Glob patterns for env files to discover and copy
env_patterns = [".env", ".env.local", ".env.*"]

# Automatically copy env files when creating a new worktree
auto_copy_env = true

# Default base branch for new worktrees
default_base = "main"
```

## TUI Dashboard

Running `wt` with no arguments opens an interactive dashboard:

- **j/k** or arrow keys — navigate worktrees
- **Enter** — switch to selected worktree (cd on exit)
- **n** — create new worktree
- **d** — delete selected worktree
- **p** — create worktree from PR
- **q / Esc** — quit

## Uninstall

```bash
rm "$(which wt)"
```
