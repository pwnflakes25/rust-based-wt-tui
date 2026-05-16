use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use super::{App, AppMode};

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(5),   // body
            Constraint::Length(3), // footer
        ])
        .split(f.area());

    render_header(f, chunks[0], app);
    render_body(f, chunks[1], app);
    render_footer(f, chunks[2], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let mode_str = match &app.mode {
        AppMode::Normal => "",
        AppMode::ConfirmDelete => " [CONFIRM DELETE]",
        AppMode::NewInput(_) | AppMode::NewBaseInput { .. } => " [NEW WORKTREE]",
        AppMode::PrInput(_) => " [PR INPUT]",
    };

    let title = Line::from(vec![
        Span::styled(
            " wt dashboard",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(mode_str, Style::default().fg(Color::Yellow)),
    ]);

    let header = Paragraph::new(title).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Git Worktree Manager "),
    );
    f.render_widget(header, area);
}

fn render_body(f: &mut Frame, area: Rect, app: &App) {
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_worktree_list(f, body_chunks[0], app);

    let show_autocomplete = matches!(
        app.mode,
        AppMode::NewInput(_) | AppMode::NewBaseInput { .. }
    ) && app.autocomplete.is_some();

    if show_autocomplete {
        render_autocomplete(f, body_chunks[1], app);
    } else {
        render_worktree_info(f, body_chunks[1], app);
    }
}

fn render_worktree_list(f: &mut Frame, area: Rect, app: &App) {
    const SPINNER: [char; 4] = ['|', '/', '-', '\\'];

    let items: Vec<ListItem> = app
        .worktrees
        .iter()
        .enumerate()
        .map(|(i, wt)| {
            let is_current = app.current_path.as_ref() == Some(&wt.path);
            let is_selected = i == app.selected;
            let is_deleting = app.deleting_paths.contains(&wt.path);

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if is_deleting {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::DIM)
            } else if is_current {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let mut spans = if is_deleting {
                let frame = SPINNER[app.spinner_frame % 4];
                vec![
                    Span::styled(
                        format!("{frame} "),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(wt.display_name(), style),
                ]
            } else {
                vec![Span::styled(wt.display_name(), style)]
            };

            if is_current {
                spans.push(Span::styled(" *", style));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Worktrees "),
    );

    f.render_widget(list, area);
}

fn render_worktree_info(f: &mut Frame, area: Rect, app: &App) {
    let info = if let Some(wt) = app.selected_worktree() {
        let dirty = app.dirty_cache.get(&wt.path).copied().unwrap_or(false);
        let (ahead, behind) = app.ahead_behind_cache.get(&wt.path).copied().unwrap_or((0, 0));

        let status_str = if dirty { "dirty" } else { "clean" };
        let status_color = if dirty { Color::Red } else { Color::Green };

        let mut lines = vec![
            Line::from(vec![
                Span::raw("  Branch:   "),
                Span::styled(
                    wt.display_name(),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("  Path:     "),
                Span::raw(wt.path.display().to_string()),
            ]),
            Line::from(vec![
                Span::raw("  HEAD:     "),
                Span::styled(
                    &wt.head[..7.min(wt.head.len())],
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::raw("  Status:   "),
                Span::styled(status_str, Style::default().fg(status_color)),
            ]),
        ];

        if ahead > 0 || behind > 0 {
            lines.push(Line::from(vec![
                Span::raw("  Tracking: "),
                Span::styled(
                    format!("{ahead} ahead, {behind} behind"),
                    Style::default().fg(Color::Magenta),
                ),
            ]));
        }

        if wt.is_main {
            lines.push(Line::from(vec![
                Span::raw("  Type:     "),
                Span::styled("main worktree", Style::default().fg(Color::Blue)),
            ]));
        }

        // Env files section
        if let Some(files) = app.env_files_cache.get(&wt.path) {
            lines.push(Line::from("")); // blank separator
            if files.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  Env:      "),
                    Span::styled("No .env files", Style::default().fg(Color::DarkGray)),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("  Env:      "),
                    Span::styled(
                        format!("{} file(s)", files.len()),
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
                for file in files {
                    lines.push(Line::from(vec![
                        Span::raw("            "),
                        Span::styled(file.as_str(), Style::default().fg(Color::Green)),
                    ]));
                }
            }
        }

        lines
    } else {
        vec![Line::from("  No worktree selected")]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Info ");

    let paragraph = Paragraph::new(info).block(block);
    f.render_widget(paragraph, area);
}

fn render_autocomplete(f: &mut Frame, area: Rect, app: &App) {
    let Some(ac) = &app.autocomplete else {
        return;
    };

    let title = match &app.mode {
        AppMode::NewInput(_) => " Branches ",
        AppMode::NewBaseInput { .. } => " Base Branches ",
        _ => " Suggestions ",
    };

    let items: Vec<ListItem> = ac
        .filtered
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if ac.selected == Some(i) {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Span::styled(format!("  {name}"), style))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title),
    );

    f.render_widget(list, area);
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let msg = if let Some(msg) = &app.message {
        Line::from(vec![
            Span::styled("  ", Style::default().fg(Color::Yellow)),
            Span::styled(msg.as_str(), Style::default().fg(Color::Yellow)),
        ])
    } else {
        match &app.mode {
            AppMode::Normal => Line::from(vec![
                Span::styled(" [n]", Style::default().fg(Color::Cyan)),
                Span::raw("ew "),
                Span::styled("[d]", Style::default().fg(Color::Cyan)),
                Span::raw("el "),
                Span::styled("[c]", Style::default().fg(Color::Cyan)),
                Span::raw("ursor "),
                Span::styled("[e]", Style::default().fg(Color::Cyan)),
                Span::raw("nv "),
                Span::styled("[s]", Style::default().fg(Color::Cyan)),
                Span::raw("witch "),
                Span::styled("[p]", Style::default().fg(Color::Cyan)),
                Span::raw("r "),
                Span::styled("[m]", Style::default().fg(Color::Cyan)),
                Span::raw("erge "),
                Span::styled("[r]", Style::default().fg(Color::Cyan)),
                Span::raw("efresh "),
                Span::styled("[q]", Style::default().fg(Color::Cyan)),
                Span::raw("uit"),
            ]),
            AppMode::ConfirmDelete => Line::from(vec![
                Span::styled(
                    " Delete selected worktree? ",
                    Style::default().fg(Color::Red),
                ),
                Span::styled("[y]", Style::default().fg(Color::Cyan)),
                Span::raw("es "),
                Span::styled("[n]", Style::default().fg(Color::Cyan)),
                Span::raw("o"),
            ]),
            AppMode::NewInput(s) => Line::from(vec![
                Span::raw(" Branch: "),
                Span::styled(s.as_str(), Style::default().fg(Color::Cyan)),
                Span::raw("_ "),
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::raw(":complete "),
                Span::styled("↑↓", Style::default().fg(Color::Cyan)),
                Span::raw(":navigate "),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::raw(":confirm "),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::raw(":cancel"),
            ]),
            AppMode::NewBaseInput { base, .. } => Line::from(vec![
                Span::raw(" Base: "),
                Span::styled(base.as_str(), Style::default().fg(Color::Cyan)),
                Span::raw("_ "),
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::raw(":complete "),
                Span::styled("↑↓", Style::default().fg(Color::Cyan)),
                Span::raw(":navigate "),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::raw(":create "),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::raw(":cancel"),
            ]),
            AppMode::PrInput(s) => Line::from(vec![
                Span::raw(" PR #: "),
                Span::styled(s.as_str(), Style::default().fg(Color::Cyan)),
                Span::raw("_ (Enter to confirm, Esc to cancel)"),
            ]),
        }
    };

    let footer = Paragraph::new(msg).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Actions "),
    );
    f.render_widget(footer, area);
}
