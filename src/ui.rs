use crate::app::{App, Pane};
use ratatui::{
    prelude::*,
    widgets::*,
};

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);

    let panes =
        Layout::horizontal([Constraint::Percentage(28), Constraint::Percentage(72)])
            .split(chunks[0]);

    let right = Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).split(panes[1]);

    render_projects(frame, app, panes[0]);
    render_sessions(frame, app, right[0]);
    render_preview(frame, app, right[1]);
    render_status(frame, app, chunks[1]);

    if app.show_delete_confirm {
        render_confirm(frame, area);
    }
}

fn render_projects(frame: &mut Frame, app: &mut App, area: Rect) {
    let active = app.active_pane == Pane::Projects;
    let border_style = if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|p| {
            let count = p.sessions.len();
            ListItem::new(Line::from(vec![
                Span::raw(format!(" {} ", p.label)),
                Span::styled(
                    format!("[{count}]"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Projects ")
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut app.projects_state);
}

fn render_sessions(frame: &mut Frame, app: &mut App, area: Rect) {
    let active = app.active_pane == Pane::Sessions;
    let border_style = if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let sessions = app
        .projects
        .get(app.selected_project)
        .map(|p| p.sessions.as_slice())
        .unwrap_or(&[]);

    let items: Vec<ListItem> = sessions
        .iter()
        .map(|s| {
            let title = s.display_title();
            let branch = s.git_branch.as_deref().unwrap_or("?");
            let meta = format!(
                "   {} · {} · {}",
                branch,
                s.age_display(),
                s.size_display()
            );
            ListItem::new(Text::from(vec![
                Line::from(Span::raw(format!(" {}", title))),
                Line::from(Span::styled(meta, Style::default().fg(Color::DarkGray))),
            ]))
        })
        .collect();

    let title = app
        .projects
        .get(app.selected_project)
        .map(|p| format!(" {} — Sessions ", p.label))
        .unwrap_or_else(|| " Sessions ".to_string());

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(title)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut app.sessions_state);
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect) {
    let text = app
        .current_session()
        .and_then(|s| s.first_message.as_deref())
        .unwrap_or("");

    let border_style = if app.active_pane == Pane::Sessions {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(
                Block::bordered()
                    .title(" Message ")
                    .border_style(border_style),
            )
            .style(Style::default().fg(Color::Gray)),
        area,
    );
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let (msg, style): (&str, Style) = if !app.status.is_empty() {
        (&app.status, Style::default().fg(Color::Yellow))
    } else {
        (
            " [↑↓/jk] Navigate  [Tab] Switch pane  [Enter] Resume  [d] Delete  [q] Quit",
            Style::default().fg(Color::DarkGray),
        )
    };
    frame.render_widget(Paragraph::new(Span::styled(msg, style)), area);
}

fn render_confirm(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(44, 6, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Delete this session? This cannot be undone.",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  [y] Confirm    [n / Esc] Cancel",
                Style::default(),
            )),
        ])
        .block(
            Block::bordered()
                .title(" Confirm Delete ")
                .border_style(Style::default().fg(Color::Red)),
        ),
        popup,
    );
}

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}
