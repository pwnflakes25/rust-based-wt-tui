use crossterm::event::{KeyCode, KeyEvent};

use super::{App, AppMode};
use crate::env::copy_env_files;

pub fn handle_key(app: &mut App, key: KeyEvent) {
    match &app.mode {
        AppMode::Normal => handle_normal(app, key),
        AppMode::ConfirmDelete => handle_confirm_delete(app, key),
        AppMode::PrInput(_) => handle_pr_input(app, key),
    }
}

fn handle_normal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if !app.worktrees.is_empty() {
                app.selected = (app.selected + 1) % app.worktrees.len();
                app.message = None;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if !app.worktrees.is_empty() {
                app.selected = app
                    .selected
                    .checked_sub(1)
                    .unwrap_or(app.worktrees.len() - 1);
                app.message = None;
            }
        }
        KeyCode::Char('n') => {
            // New worktree: we can't easily prompt in TUI, so show a message
            app.message = Some(
                "Use 'wt new <branch>' from the command line to create a worktree.".to_owned(),
            );
        }
        KeyCode::Char('d') => {
            if let Some(wt) = app.selected_worktree() {
                if wt.is_main {
                    app.message = Some("Cannot delete the main worktree.".to_owned());
                } else {
                    app.mode = AppMode::ConfirmDelete;
                }
            }
        }
        KeyCode::Char('s') | KeyCode::Enter => {
            if let Some(wt) = app.selected_worktree() {
                app.switch_path = Some(wt.path.display().to_string());
                app.should_quit = true;
            }
        }
        KeyCode::Char('e') => {
            // Copy env from selected to current
            if let Some(wt) = app.selected_worktree() {
                if let Some(current) = &app.current_path {
                    if &wt.path == current {
                        app.message = Some("Cannot copy env to the same worktree.".to_owned());
                    } else {
                        match copy_env_files(&wt.path, current, &app.config.env_patterns) {
                            Ok(copied) if copied.is_empty() => {
                                app.message = Some("No .env files found to copy.".to_owned());
                            }
                            Ok(copied) => {
                                app.message = Some(format!(
                                    "Copied {} env file(s): {}",
                                    copied.len(),
                                    copied.join(", ")
                                ));
                            }
                            Err(e) => {
                                app.message = Some(format!("Error: {e}"));
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('p') => {
            app.mode = AppMode::PrInput(String::new());
        }
        KeyCode::Char('m') => {
            // Merge selected into current
            handle_merge(app);
        }
        KeyCode::Char('r') => {
            match app.refresh() {
                Ok(()) => app.message = Some("Refreshed.".to_owned()),
                Err(e) => app.message = Some(format!("Refresh error: {e}")),
            }
        }
        _ => {}
    }
}

fn handle_confirm_delete(app: &mut App, key: KeyEvent) {
    if let KeyCode::Char('y' | 'Y') = key.code {
        if let Some(wt) = app.selected_worktree().cloned() {
            let name = wt.display_name();
            match app.ctx.remove_worktree(&name, false) {
                Ok(()) => {
                    app.message = Some(format!("Removed '{name}'."));
                    let _ = app.refresh();
                }
                Err(e) => {
                    app.message = Some(format!("Error: {e}"));
                }
            }
        }
        app.mode = AppMode::Normal;
    } else {
        app.mode = AppMode::Normal;
        app.message = None;
    }
}

fn handle_pr_input(app: &mut App, key: KeyEvent) {
    let current_input = if let AppMode::PrInput(s) = &app.mode {
        s.clone()
    } else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.message = None;
        }
        KeyCode::Enter => {
            if let Ok(num) = current_input.parse::<u64>() {
                app.mode = AppMode::Normal;
                match crate::commands::pr::run(&app.ctx, &app.config, num) {
                    Ok(()) => {
                        app.message = Some(format!("PR #{num} worktree created."));
                        let _ = app.refresh();
                    }
                    Err(e) => {
                        app.message = Some(format!("PR error: {e}"));
                    }
                }
            } else {
                app.message = Some("Invalid PR number.".to_owned());
                app.mode = AppMode::Normal;
            }
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            let mut s = current_input;
            s.push(c);
            app.mode = AppMode::PrInput(s);
        }
        KeyCode::Backspace => {
            let mut s = current_input;
            s.pop();
            app.mode = AppMode::PrInput(s);
        }
        _ => {}
    }
}

fn handle_merge(app: &mut App) {
    let Some(wt) = app.selected_worktree().cloned() else {
        return;
    };

    let Some(current) = &app.current_path else {
        app.message = Some("Cannot determine current worktree.".to_owned());
        return;
    };

    if &wt.path == current {
        app.message = Some("Cannot merge a worktree into itself.".to_owned());
        return;
    }

    let Some(source_branch) = &wt.branch else {
        app.message = Some("Selected worktree has no branch.".to_owned());
        return;
    };

    // Check source is clean
    match app.ctx.is_worktree_dirty(&wt.path) {
        Ok(true) => {
            app.message = Some(format!("'{source_branch}' has uncommitted changes."));
            return;
        }
        Err(e) => {
            app.message = Some(format!("Error checking status: {e}"));
            return;
        }
        Ok(false) => {}
    }

    // Check current is clean
    match app.ctx.is_worktree_dirty(current) {
        Ok(true) => {
            app.message = Some("Current worktree has uncommitted changes.".to_owned());
            return;
        }
        Err(e) => {
            app.message = Some(format!("Error checking status: {e}"));
            return;
        }
        Ok(false) => {}
    }

    match app.ctx.merge_branch(source_branch, current) {
        Ok(true) => {
            app.message = Some(format!("Merged {source_branch} successfully."));
        }
        Ok(false) => {
            app.message = Some("Merge conflict! Resolve manually.".to_owned());
        }
        Err(e) => {
            app.message = Some(format!("Merge error: {e}"));
        }
    }
}
