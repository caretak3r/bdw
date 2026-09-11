//! Detail overlay: `⏎` on a selected issue opens this centered popup with the
//! full issue record. Everything here comes from the loaded snapshot + feed
//! ring — `Issue` already carries every field the mock's `bd show`/`bd
//! history` panel needs, so this never shells out on its own.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

use super::feed::{change_label, status_label};

/// Most-recent events shown in the history panel; the ring itself can be
/// much longer, but a popup only has so many lines.
const HISTORY_CAP: usize = 8;

pub(crate) fn draw(f: &mut Frame, area: Rect, app: &App) {
    let Some(issue) = app.selected_issue() else {
        return;
    };

    let popup = centered_rect(80, 80, area);
    f.render_widget(Clear, popup);
    // -2 for the block's left/right borders; markdown code-block/table rules
    // are sized to this so they don't overrun the popup.
    let content_width = popup.width.saturating_sub(2) as usize;

    let theme = &app.theme;
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(format!(" {} ", issue.id))
        .title_top(
            Line::from(Span::styled("esc close", Style::default().fg(theme.dimmer)))
                .right_aligned(),
        );
    if let Some(bg) = theme.bg {
        block = block.style(Style::default().bg(bg));
    }

    let mut lines = vec![
        Line::from(Span::styled(
            issue.title.clone(),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        kv_line("status", status_label(&issue.status).to_string(), theme),
        kv_line("priority", fmt_prio(issue.priority), theme),
        kv_line("type", fmt_opt_str(&non_empty(&issue.issue_type)), theme),
        kv_line("parent", fmt_opt_str(&issue.parent), theme),
        kv_line("owner", fmt_opt_str(&issue.owner), theme),
        kv_line("created by", fmt_opt_str(&issue.created_by), theme),
        kv_line(
            "deps",
            format!(
                "depends on {} · blocks {}",
                issue.dependency_count, issue.dependent_count
            ),
            theme,
        ),
    ];

    push_section(
        &mut lines,
        "DESCRIPTION",
        non_empty(&issue.description),
        theme,
        content_width,
    );
    push_section(
        &mut lines,
        "NOTES",
        issue.notes.clone(),
        theme,
        content_width,
    );
    push_section(
        &mut lines,
        "DESIGN",
        issue.design.clone(),
        theme,
        content_width,
    );
    push_section(
        &mut lines,
        "ACCEPTANCE",
        issue.acceptance_criteria.clone(),
        theme,
        content_width,
    );

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "HISTORY",
        Style::default().fg(theme.dimmer),
    )));
    let history: Vec<_> = app
        .feed
        .iter()
        .filter(|e| e.issue_id == issue.id)
        .take(HISTORY_CAP)
        .collect();
    if history.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no events this session",
            Style::default().fg(theme.dim),
        )));
    }
    for event in history {
        // Full raw actor name here, unlike the compact board/feed panes —
        // this is the one place the mock shows an identity in full.
        let actor = event
            .actor
            .as_ref()
            .map(|a| a.0.clone())
            .unwrap_or_else(|| "derived".to_string());
        lines.push(Line::from(vec![
            Span::styled(format!("  {actor}  "), Style::default().fg(theme.dim)),
            Span::raw(change_label(&event.change)),
        ]));
    }

    // Approximate: wrapping can reflow `lines` into more visual rows than
    // this, so the clamp is a slight overestimate of how far there is to
    // scroll — harmless, worst case is a screen of trailing blank space.
    let inner_h = popup.height.saturating_sub(2);
    let max_scroll = (lines.len() as u16).saturating_sub(inner_h);
    let scroll = app.detail_scroll.min(max_scroll);
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: true })
            .scroll((scroll, 0)),
        popup,
    );
}

fn kv_line(key: &str, value: String, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:>10}  "), Style::default().fg(theme.dim)),
        Span::styled(value, Style::default().fg(theme.fg)),
    ])
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn fmt_opt_str(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| "—".to_string())
}

fn fmt_prio(p: Option<i32>) -> String {
    p.map(|p| format!("P{p}"))
        .unwrap_or_else(|| "—".to_string())
}

fn push_section(
    lines: &mut Vec<Line<'static>>,
    label: &'static str,
    body: Option<String>,
    theme: &Theme,
    content_width: usize,
) {
    let Some(body) = body.filter(|b| !b.is_empty()) else {
        return;
    };
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        label,
        Style::default().fg(theme.dimmer),
    )));
    lines.extend(super::markdown::render(&body, theme, content_width));
}

fn centered_rect(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vertical[1])[1]
}
