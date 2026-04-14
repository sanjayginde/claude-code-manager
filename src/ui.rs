use std::time::SystemTime;

use crate::app::{App, Pane};
use crate::data::{Session, SessionTitle};
use ratatui::{
    prelude::*,
    widgets::*,
};

// ── Theme ─────────────────────────────────────────────────────────────────────

pub struct Theme {
    /// Active pane border and edit-mode cursor/text.
    pub active: Color,
    /// Inactive pane border and selection highlight background.
    pub inactive: Color,
    /// Secondary / meta text (counts, branch, age, size).
    pub meta: Color,
    /// Preview pane body text.
    pub preview_text: Color,
    /// Transient app status messages (e.g. "Copied!").
    pub status_msg: Color,
    /// Keyboard hint bar text.
    pub hint: Color,
    /// Danger / destructive action (delete confirm).
    pub danger: Color,
}

impl Theme {
    pub fn gruvbox_dark() -> Self {
        Self {
            active:       Color::Rgb(250, 189, 47),
            inactive:     Color::Rgb(80, 73, 69),
            meta:         Color::Rgb(168, 153, 132),
            preview_text: Color::Gray,
            status_msg:   Color::Yellow,
            hint:         Color::DarkGray,
            danger:       Color::Red,
        }
    }

    pub fn catppuccin_mocha() -> Self {
        let c = &catppuccin::PALETTE.mocha.colors;
        Self {
            active:       c.yellow.into(),
            inactive:     c.surface1.into(),
            meta:         c.subtext0.into(),
            preview_text: c.overlay2.into(),
            status_msg:   c.peach.into(),
            hint:         c.overlay0.into(),
            danger:       c.red.into(),
        }
    }

    pub fn catppuccin_macchiato() -> Self {
        let c = &catppuccin::PALETTE.macchiato.colors;
        Self {
            active:       c.yellow.into(),
            inactive:     c.surface1.into(),
            meta:         c.subtext0.into(),
            preview_text: c.overlay2.into(),
            status_msg:   c.peach.into(),
            hint:         c.overlay0.into(),
            danger:       c.red.into(),
        }
    }

    pub fn catppuccin_frappe() -> Self {
        let c = &catppuccin::PALETTE.frappe.colors;
        Self {
            active:       c.yellow.into(),
            inactive:     c.surface1.into(),
            meta:         c.subtext0.into(),
            preview_text: c.overlay2.into(),
            status_msg:   c.peach.into(),
            hint:         c.overlay0.into(),
            danger:       c.red.into(),
        }
    }

    pub fn catppuccin_latte() -> Self {
        let c = &catppuccin::PALETTE.latte.colors;
        Self {
            active:       c.yellow.into(),
            inactive:     c.surface1.into(),
            meta:         c.subtext0.into(),
            preview_text: c.overlay2.into(),
            status_msg:   c.peach.into(),
            hint:         c.overlay0.into(),
            danger:       c.red.into(),
        }
    }

    /// Look up a theme by name. Returns `None` for unknown names.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "gruvbox-dark"          => Some(Self::gruvbox_dark()),
            "catppuccin-mocha"      => Some(Self::catppuccin_mocha()),
            "catppuccin-macchiato"  => Some(Self::catppuccin_macchiato()),
            "catppuccin-frappe"     => Some(Self::catppuccin_frappe()),
            "catppuccin-latte"      => Some(Self::catppuccin_latte()),
            _                       => None,
        }
    }

    /// Apply per-color overrides from the config file on top of this theme.
    pub fn apply_overrides(&mut self, overrides: &crate::config::ColorOverrides) {
        macro_rules! apply {
            ($field:ident) => {
                if let Some(ref hex) = overrides.$field {
                    if let Some(color) = parse_hex_color(hex) {
                        self.$field = color;
                    }
                }
            };
        }
        apply!(active);
        apply!(inactive);
        apply!(meta);
        apply!(preview_text);
        apply!(status_msg);
        apply!(hint);
        apply!(danger);
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

// ── Session display formatting ────────────────────────────────────────────────

pub fn session_title(s: &Session) -> String {
    if let SessionTitle::Loaded(t) = &s.title {
        return t.clone();
    }
    if let Some(msg) = &s.first_message {
        // Collapse to a single line — first_message may contain newlines
        // (e.g. from multi-part skill commands) that don't render well in a list row.
        let oneline: String = msg.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let truncated: String = oneline.chars().take(70).collect();
        if oneline.chars().count() > 70 {
            return format!("{}…", truncated);
        }
        return truncated;
    }
    format!("[{}]", &s.uuid[..8.min(s.uuid.len())])
}

pub fn session_age(s: &Session) -> String {
    let secs = SystemTime::now()
        .duration_since(s.last_modified)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_age_secs(secs)
}

fn format_age_secs(secs: u64) -> String {
    if secs < 60 {
        return "just now".into();
    }
    let m = secs / 60;
    if m < 60 {
        return format!("{m}m ago");
    }
    let h = m / 60;
    if h < 24 {
        return format!("{h}h ago");
    }
    let d = h / 24;
    if d < 7 {
        return format!("{d}d ago");
    }
    let w = d / 7;
    if w < 5 {
        return format!("{w}w ago");
    }
    format!("{}mo ago", d / 30)
}

pub fn session_size(s: &Session) -> String {
    let b = s.size_bytes;
    if b < 1_024 {
        format!("{b}B")
    } else if b < 1_024 * 1_024 {
        format!("{:.0}KB", b as f64 / 1_024.0)
    } else {
        format!("{:.1}MB", b as f64 / (1_024.0 * 1_024.0))
    }
}

pub fn render(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = frame.area();

    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);

    let panes =
        Layout::horizontal([Constraint::Percentage(28), Constraint::Percentage(72)])
            .split(chunks[0]);

    let right = Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).split(panes[1]);

    // Derive list states once per frame — fresh, never stale.
    let mut proj_state = app.projects_list_state();
    let mut sess_state = app.sessions_list_state();

    render_projects(frame, app, panes[0], &mut proj_state, theme);
    render_sessions(frame, app, right[0], &mut sess_state, theme);
    render_preview(frame, app, right[1], theme);
    render_status(frame, app, chunks[1], theme);

    if app.delete_pending() {
        render_confirm(frame, area, theme);
    }
}

fn render_projects(frame: &mut Frame, app: &App, area: Rect, state: &mut ratatui::widgets::ListState, theme: &Theme) {
    let active = app.active_pane() == Pane::Projects;
    let border_style = if active {
        Style::default().fg(theme.active)
    } else {
        Style::default().fg(theme.inactive)
    };

    let items: Vec<ListItem> = app
        .projects()
        .iter()
        .map(|p| {
            let count = p.sessions.len();
            ListItem::new(Line::from(vec![
                Span::raw(format!(" {} ", p.label)),
                Span::styled(
                    format!("[{count}]"),
                    Style::default().fg(theme.meta),
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
                .bg(theme.inactive)
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, state);
}

fn render_sessions(frame: &mut Frame, app: &App, area: Rect, state: &mut ratatui::widgets::ListState, theme: &Theme) {
    let active = app.active_pane() == Pane::Sessions;
    let border_style = if active {
        Style::default().fg(theme.active)
    } else {
        Style::default().fg(theme.inactive)
    };

    let selected_idx = app.sessions_list_state().selected();
    let editing_buf = app.editing_title();

    let items: Vec<ListItem> = app
        .current_sessions()
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let title_line = if let Some(buf) = editing_buf && selected_idx == Some(i) {
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(buf, Style::default().fg(theme.active)),
                    Span::styled("█", Style::default().fg(theme.active)),
                ])
            } else {
                Line::from(Span::raw(format!(" {}", session_title(s))))
            };
            let branch = if s.is_degraded() {
                "[parse error]"
            } else {
                s.git_branch.as_deref().unwrap_or("?")
            };
            let meta = format!(
                "   {} · {} · {}",
                branch,
                session_age(s),
                session_size(s)
            );
            ListItem::new(Text::from(vec![
                title_line,
                Line::from(Span::styled(meta, Style::default().fg(theme.meta))),
            ]))
        })
        .collect();

    let title = app
        .current_project_label()
        .map(|l| format!(" {} — Sessions ", l))
        .unwrap_or_else(|| " Sessions ".to_string());

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(title)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(theme.inactive)
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, state);
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let text = app
        .current_session()
        .and_then(|s| s.first_message.as_deref())
        .unwrap_or("");

    let border_style = if app.active_pane() == Pane::Sessions {
        Style::default().fg(theme.active)
    } else {
        Style::default().fg(theme.inactive)
    };

    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(
                Block::bordered()
                    .title(" Message ")
                    .border_style(border_style),
            )
            .style(Style::default().fg(theme.preview_text)),
        area,
    );
}

fn render_status(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let status = app.status();
    let (msg, style): (&str, Style) = if app.editing_title().is_some() {
        (
            " [Enter] Confirm title  [Esc] Cancel",
            Style::default().fg(theme.active),
        )
    } else if !status.is_empty() {
        (status, Style::default().fg(theme.status_msg))
    } else {
        (
            " [↑↓/jk] Navigate  [Tab] Switch pane  [Enter] Resume  [e] Edit title  [y] Copy  [d] Delete  [q] Quit",
            Style::default().fg(theme.hint),
        )
    };
    frame.render_widget(Paragraph::new(Span::styled(msg, style)), area);
}

fn render_confirm(frame: &mut Frame, area: Rect, theme: &Theme) {
    let popup = centered_rect(44, 6, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Delete this session? This cannot be undone.",
                Style::default()
                    .fg(theme.danger)
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
                .border_style(Style::default().fg(theme.danger)),
        ),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_session(uuid: &str, title: Option<&str>, first_message: Option<&str>, size_bytes: u64) -> Session {
        Session {
            uuid: uuid.to_string(),
            jsonl_path: PathBuf::from("/tmp/test.jsonl"),
            cwd: PathBuf::from("/tmp"),
            git_branch: None,
            first_message: first_message.map(String::from),
            title: match title {
                Some(t) => SessionTitle::Loaded(t.to_string()),
                None => SessionTitle::Absent,
            },
            last_modified: SystemTime::UNIX_EPOCH,
            size_bytes,
            parse_error: None,
        }
    }

    // ── session_title ─────────────────────────────────────────────────────────

    #[test]
    fn title_prefers_cached_title() {
        let s = make_session("abc", Some("My Title"), Some("first msg"), 0);
        assert_eq!(session_title(&s), "My Title");
    }

    #[test]
    fn title_falls_back_to_first_message() {
        let s = make_session("abc", None, Some("hello world"), 0);
        assert_eq!(session_title(&s), "hello world");
    }

    #[test]
    fn title_truncates_long_first_message() {
        let long = "a".repeat(80);
        let s = make_session("abc", None, Some(&long), 0);
        let result = session_title(&s);
        assert!(result.ends_with('…'));
        assert_eq!(result.chars().count(), 71); // 70 chars + ellipsis
    }

    #[test]
    fn title_does_not_truncate_exactly_70_chars() {
        let exact = "a".repeat(70);
        let s = make_session("abc", None, Some(&exact), 0);
        assert_eq!(session_title(&s), exact);
    }

    #[test]
    fn title_falls_back_to_uuid_prefix() {
        let s = make_session("abcd1234efgh", None, None, 0);
        assert_eq!(session_title(&s), "[abcd1234]");
    }

    #[test]
    fn title_collapses_multiline_first_message_to_single_line() {
        let multiline = "improve-codebase-architecture\n/improve-codebase-architecture\nBase directory";
        let s = make_session("abc", None, Some(multiline), 0);
        let title = session_title(&s);
        assert!(!title.contains('\n'), "title should not contain newlines: {title:?}");
        assert!(title.starts_with("improve-codebase-architecture"), "should start with first line: {title:?}");
    }

    // ── session_size ──────────────────────────────────────────────────────────

    #[test]
    fn size_bytes() {
        let s = make_session("x", None, None, 512);
        assert_eq!(session_size(&s), "512B");
    }

    #[test]
    fn size_kilobytes() {
        let s = make_session("x", None, None, 2_048);
        assert_eq!(session_size(&s), "2KB");
    }

    #[test]
    fn size_megabytes() {
        let s = make_session("x", None, None, 2_097_152);
        assert_eq!(session_size(&s), "2.0MB");
    }

    // ── format_age_secs ───────────────────────────────────────────────────────

    #[test]
    fn age_just_now() {
        assert_eq!(format_age_secs(30), "just now");
    }

    #[test]
    fn age_minutes() {
        assert_eq!(format_age_secs(5 * 60), "5m ago");
    }

    #[test]
    fn age_hours() {
        assert_eq!(format_age_secs(3 * 3600), "3h ago");
    }

    #[test]
    fn age_days() {
        assert_eq!(format_age_secs(4 * 86400), "4d ago");
    }

    #[test]
    fn age_weeks() {
        assert_eq!(format_age_secs(2 * 7 * 86400), "2w ago");
    }

    #[test]
    fn age_months() {
        assert_eq!(format_age_secs(60 * 86400), "2mo ago");
    }
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
