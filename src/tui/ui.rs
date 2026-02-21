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
            Constraint::Length(3),  // header
            Constraint::Min(5),    // body
            Constraint::Length(3), // env/info bar
            Constraint::Length(3), // footer
        ])
        .split(f.area());

    render_header(f, chunks[0], app);
    render_body(f, chunks[1], app);
    render_env_bar(f, chunks[2], app);
    render_footer(f, chunks[3], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let mode_str = match &app.mode {
        AppMode::Normal => "",
        AppMode::ConfirmDelete => " [CONFIRM DELETE]",
        AppMode::ConfirmForceDelete => " [FORCE DELETE]",
        AppMode::NewInput(_) => " [NEW WORKTREE]",
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
    render_worktree_info(f, body_chunks[1], app);
}

fn render_worktree_list(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .worktrees
        .iter()
        .enumerate()
        .map(|(i, wt)| {
            let is_current = app.current_path.as_ref() == Some(&wt.path);
            let is_selected = i == app.selected;

            let marker = if is_current { " *" } else { "" };
            let name = format!("{}{marker}", wt.display_name());

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(Span::styled(name, style))
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
        let dirty = app
            .ctx
            .is_worktree_dirty(&wt.path)
            .unwrap_or(false);
        let (ahead, behind) = app.ctx.ahead_behind(&wt.path).unwrap_or((0, 0));

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

fn render_env_bar(f: &mut Frame, area: Rect, app: &App) {
    let content = if let Some(wt) = app.selected_worktree() {
        let files = crate::env::find_env_files(&wt.path, &app.config.env_patterns)
            .unwrap_or_default();
        if files.is_empty() {
            "  No .env files".to_owned()
        } else {
            format!("  .env files: {}", files.join(", "))
        }
    } else {
        String::new()
    };

    let bar = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Env "),
    );
    f.render_widget(bar, area);
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let msg = if let Some(msg) = &app.message {
        Line::from(Span::styled(
            format!("  {msg}"),
            Style::default().fg(Color::Yellow),
        ))
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
            AppMode::ConfirmForceDelete => Line::from(vec![
                Span::styled(
                    " Has local changes. Force delete? ",
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
                Span::raw("_ (Enter to create, Esc to cancel)"),
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
