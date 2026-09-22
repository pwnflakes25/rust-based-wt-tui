use crossterm::event::{KeyCode, KeyEvent};

use super::{App, AppMode, Autocomplete};
use crate::env::{copy_env_files, copy_path_entries};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    match &app.mode {
        AppMode::Normal => handle_normal(app, key),
        AppMode::ConfirmDelete => handle_confirm_delete(app, key),
        AppMode::NewInput(_) => handle_new_input(app, key),
        AppMode::NewBaseInput { .. } => handle_new_base_input(app, key),
        AppMode::PrInput(_) => handle_pr_input(app, key),
    }
}

fn handle_normal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            if app.message.is_some() {
                app.message = None;
            } else {
                app.should_quit = true;
            }
        }
        KeyCode::Char('q') => {
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
            let branches = app.ctx.list_local_branches();
            app.autocomplete = Some(Autocomplete::new(branches));
            app.mode = AppMode::NewInput(String::new());
            app.message = None;
        }
        KeyCode::Char('d') => {
            if let Some(wt) = app.selected_worktree() {
                if wt.is_main {
                    app.message = Some("Cannot delete the main worktree.".to_owned());
                } else if app.deleting_paths.contains(&wt.path) {
                    app.message = Some("Deletion already in progress.".to_owned());
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
        KeyCode::Char('c') => {
            // Open selected worktree in Cursor
            if let Some(wt) = app.selected_worktree() {
                let path = wt.path.display().to_string();
                match std::process::Command::new("cursor")
                    .arg(&path)
                    .spawn()
                {
                    Ok(_) => {
                        app.message = Some(format!("Opened Cursor at {path}"));
                    }
                    Err(e) => {
                        app.message = Some(format!("Failed to open Cursor: {e}"));
                    }
                }
            }
        }
        KeyCode::Char('e') => handle_copy(app),
        KeyCode::Char('p') => {
            app.mode = AppMode::PrInput(String::new());
        }
        KeyCode::Char('m') => {
            // Merge selected into current
            handle_merge(app);
        }
        KeyCode::Char('o') => {
            app.cycle_sort();
            app.message = None;
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

/// Copy env files and extra paths from selected worktree to current.
fn handle_copy(app: &mut App) {
    let Some(wt) = app.selected_worktree() else {
        return;
    };
    let Some(current) = &app.current_path else {
        return;
    };

    if &wt.path == current {
        app.message = Some("Cannot copy to the same worktree.".to_owned());
        return;
    }

    let source = wt.path.clone();
    match copy_env_files(&source, current, &app.config.env_patterns) {
        Ok(copied) => {
            let extra =
                copy_path_entries(&source, current, &app.config.copy_paths).unwrap_or_default();
            let total = copied.len() + extra.len();
            if total == 0 {
                app.message = Some("No files found to copy.".to_owned());
            } else {
                let mut all: Vec<String> = copied;
                all.extend(extra);
                app.message = Some(format!("Copied {} item(s): {}", total, all.join(", ")));
            }
        }
        Err(e) => {
            app.message = Some(format!("Error: {e}"));
        }
    }
}

fn handle_confirm_delete(app: &mut App, key: KeyEvent) {
    if !matches!(key.code, KeyCode::Char('y' | 'Y')) {
        app.mode = AppMode::Normal;
        app.message = None;
        return;
    }

    let Some(wt) = app.selected_worktree().cloned() else {
        app.mode = AppMode::Normal;
        return;
    };

    app.mode = AppMode::Normal;

    let path = wt.path.clone();
    let name = wt.display_name();
    app.deleting_paths.insert(path.clone());

    let ctx = app.ctx.clone();
    let tx = app.delete_tx.clone();

    std::thread::spawn(move || {
        let result = ctx.remove_worktree_at(&wt, false).or_else(|e| {
            let err_str = e.to_string();
            if err_str.contains("modified or untracked") || err_str.contains("--force") {
                ctx.remove_worktree_at(&wt, true)
            } else {
                Err(e)
            }
        });
        let _ = tx.send((path, name, result));
    });
}

fn handle_new_input(app: &mut App, key: KeyEvent) {
    let current_input = if let AppMode::NewInput(s) = &app.mode {
        s.clone()
    } else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.autocomplete = None;
            app.message = None;
        }
        KeyCode::Enter => {
            let branch = current_input.trim().to_owned();
            if branch.is_empty() {
                app.message = Some("Branch name cannot be empty.".to_owned());
                app.mode = AppMode::Normal;
                app.autocomplete = None;
                return;
            }

            app.autocomplete = None;

            if app.ctx.branch_exists(&branch) {
                // Branch exists — base is irrelevant, create immediately
                create_worktree(app, &branch, "");
            } else {
                // New branch — ask for base
                let default_base = app.ctx.detect_default_branch();
                let branches = app.ctx.list_all_branches();
                let mut ac = Autocomplete::new(branches);
                ac.update_filter(&default_base);
                app.autocomplete = Some(ac);
                app.mode = AppMode::NewBaseInput {
                    branch,
                    base: default_base,
                };
            }
        }
        KeyCode::Tab => {
            if let Some(ac) = &app.autocomplete {
                if let Some(val) = ac.selected_value() {
                    let val = val.to_owned();
                    app.mode = AppMode::NewInput(val.clone());
                    if let Some(ac) = &mut app.autocomplete {
                        ac.update_filter(&val);
                    }
                }
            }
        }
        KeyCode::Down => {
            if let Some(ac) = &mut app.autocomplete {
                ac.next();
            }
        }
        KeyCode::Up => {
            if let Some(ac) = &mut app.autocomplete {
                ac.prev();
            }
        }
        KeyCode::Backspace => {
            let mut s = current_input;
            s.pop();
            if let Some(ac) = &mut app.autocomplete {
                ac.update_filter(&s);
            }
            app.mode = AppMode::NewInput(s);
        }
        KeyCode::Char(c) => {
            let mut s = current_input;
            s.push(c);
            if let Some(ac) = &mut app.autocomplete {
                ac.update_filter(&s);
            }
            app.mode = AppMode::NewInput(s);
        }
        _ => {}
    }
}

fn handle_new_base_input(app: &mut App, key: KeyEvent) {
    let (branch, current_base) = if let AppMode::NewBaseInput { branch, base } = &app.mode {
        (branch.clone(), base.clone())
    } else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.autocomplete = None;
            app.message = None;
        }
        KeyCode::Enter => {
            let base = current_base.trim().to_owned();
            if base.is_empty() {
                app.message = Some("Base branch cannot be empty.".to_owned());
                app.mode = AppMode::Normal;
                app.autocomplete = None;
                return;
            }
            app.autocomplete = None;
            create_worktree(app, &branch, &base);
        }
        KeyCode::Tab => {
            if let Some(ac) = &app.autocomplete {
                if let Some(val) = ac.selected_value() {
                    let val = val.to_owned();
                    app.mode = AppMode::NewBaseInput {
                        branch,
                        base: val.clone(),
                    };
                    if let Some(ac) = &mut app.autocomplete {
                        ac.update_filter(&val);
                    }
                }
            }
        }
        KeyCode::Down => {
            if let Some(ac) = &mut app.autocomplete {
                ac.next();
            }
        }
        KeyCode::Up => {
            if let Some(ac) = &mut app.autocomplete {
                ac.prev();
            }
        }
        KeyCode::Backspace => {
            let mut s = current_base;
            s.pop();
            if let Some(ac) = &mut app.autocomplete {
                ac.update_filter(&s);
            }
            app.mode = AppMode::NewBaseInput { branch, base: s };
        }
        KeyCode::Char(c) => {
            let mut s = current_base;
            s.push(c);
            if let Some(ac) = &mut app.autocomplete {
                ac.update_filter(&s);
            }
            app.mode = AppMode::NewBaseInput { branch, base: s };
        }
        _ => {}
    }
}

/// Shared helper to create a worktree and handle auto-copy env.
fn create_worktree(app: &mut App, branch: &str, base: &str) {
    app.mode = AppMode::Normal;
    match app.ctx.create_worktree(branch, base) {
        Ok(path) => {
            if app.config.auto_copy_env {
                if let Some(current) = &app.current_path {
                    let _ =
                        crate::env::copy_env_files(current, &path, &app.config.env_patterns);
                    let _ =
                        crate::env::copy_path_entries(current, &path, &app.config.copy_paths);
                }
            }
            let _ = app.refresh_ex(false);
            app.select_path(&path);
            app.message = Some(format!("Created worktree '{branch}'."));
        }
        Err(e) => {
            app.message = Some(format!("Error: {e}"));
        }
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
                        let _ = app.refresh_ex(false);
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
