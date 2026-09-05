//! Tasks pane: issues grouped by status. Epic grouping and search are
//! Phase 3; this phase only ever groups by status.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::App;
use crate::model::{Issue, Status};

use super::colors;

enum Row<'a> {
    Group(&'static str),
    Item(&'a Issue, bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Wip,
    Blocked,
    Open,
    Closed,
    Other,
}

const BUCKET_ORDER: [(Bucket, &str); 5] = [
    (Bucket::Wip, "IN PROGRESS"),
    (Bucket::Blocked, "BLOCKED"),
    (Bucket::Open, "OPEN"),
    (Bucket::Closed, "CLOSED"),
    (Bucket::Other, "OTHER"),
];

fn bucket_for(status: &Status, blocked: bool) -> Bucket {
    match status {
        Status::InProgress => Bucket::Wip,
        Status::Open if blocked => Bucket::Blocked,
        Status::Open => Bucket::Open,
        Status::Closed => Bucket::Closed,
        Status::Deferred | Status::Other(_) => Bucket::Other,
    }
}

fn rows(app: &App) -> Vec<Row<'_>> {
    let mut out = Vec::new();
    for (bucket, label) in BUCKET_ORDER {
        let mut group: Vec<&Issue> = app
            .issues
            .iter()
            .filter(|issue| bucket_for(&issue.status, app.is_blocked(issue)) == bucket)
            .collect();
        if group.is_empty() {
            continue;
        }
        group.sort_by(|a, b| a.id.cmp(&b.id));
        out.push(Row::Group(label));
        for issue in group {
            out.push(Row::Item(issue, app.is_blocked(issue)));
        }
    }
    out
}

/// Number of *selectable* rows (issues, not group headers) — used by `App`
/// to clamp `selected` after a refresh or a key press.
pub(crate) fn row_count(app: &App) -> usize {
    rows(app)
        .iter()
        .filter(|row| matches!(row, Row::Item(..)))
        .count()
}

/// `Status` has variants beyond the four glyphs in the spec's hard
/// constraints (`deferred`, and the `Other` catch-all); both get a distinct,
/// clearly-secondary glyph so this match can stay exhaustive.
fn glyph(status: &Status, blocked: bool) -> (&'static str, Color) {
    if blocked {
        return ("❄", colors::CYAN);
    }
    match status {
        Status::Open => ("○", colors::FG),
        Status::InProgress => ("◐", colors::YELLOW),
        Status::Closed => ("✓", colors::GREEN),
        Status::Deferred => ("·", colors::DIM),
        Status::Other(_) => ("?", colors::DIM),
    }
}

fn priority_span(priority: Option<i32>) -> Option<Span<'static>> {
    priority.map(|p| {
        let color = match p {
            0 => colors::RED,
            1 => colors::ORANGE,
            _ => colors::DIM,
        };
        Span::styled(format!("● P{p}"), Style::default().fg(color))
    })
}

pub(crate) fn draw(f: &mut Frame, area: Rect, app: &App) {
    let all_rows = rows(app);
    let mut items = Vec::with_capacity(all_rows.len());
    let mut selectable_positions = Vec::new();

    for row in &all_rows {
        match row {
            Row::Group(label) => {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("▾ {label}"),
                    Style::default().fg(colors::DIMMER),
                ))));
            }
            Row::Item(issue, blocked) => {
                selectable_positions.push(items.len());
                let (icon, icon_color) = glyph(&issue.status, *blocked);
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    Span::styled(issue.id.clone(), Style::default().fg(colors::DIM)),
                    Span::raw(" "),
                    Span::raw(issue.title.clone()),
                ];
                if let Some(prio) = priority_span(issue.priority) {
                    spans.push(Span::raw("  "));
                    spans.push(prio);
                }
                items.push(ListItem::new(Line::from(spans)));
            }
        }
    }

    let mut state = ListState::default();
    if let Some(&pos) = selectable_positions.get(app.selected) {
        state.select(Some(pos));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER))
        .title(" TASKS · status ");
    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(colors::SEL_BG)
            .add_modifier(Modifier::BOLD),
    );

    f.render_stateful_widget(list, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, RefreshOutcome};
    use crate::model::{Counts, Dependency};
    use std::path::PathBuf;

    fn issue(id: &str, status: &str, priority: i32) -> Issue {
        serde_json::from_value(
            serde_json::json!({ "id": id, "status": status, "priority": priority }),
        )
        .unwrap()
    }

    fn app_with(issues: Vec<Issue>) -> App {
        let mut app = App::new(PathBuf::from("/tmp"), "proj".into());
        app.apply_refresh(RefreshOutcome {
            issues,
            counts: Counts::default(),
            events: Vec::new(),
            malformed: 0,
            at: chrono::Utc::now(),
        });
        app
    }

    #[test]
    fn buckets_are_ordered_wip_blocked_open_closed() {
        let app = app_with(vec![
            issue("a-closed", "closed", 2),
            issue("a-open", "open", 2),
            issue("a-wip", "in_progress", 2),
        ]);
        let all = rows(&app);
        let labels: Vec<&str> = all
            .iter()
            .filter_map(|r| match r {
                Row::Group(l) => Some(*l),
                Row::Item(..) => None,
            })
            .collect();
        assert_eq!(labels, vec!["IN PROGRESS", "OPEN", "CLOSED"]);
    }

    #[test]
    fn blocked_open_issue_lands_in_blocked_bucket() {
        let mut blocked_issue = issue("a-1", "open", 1);
        blocked_issue.dependencies.push(Dependency {
            issue_id: "a-1".to_string(),
            depends_on_id: "a-2".to_string(),
            dep_type: "blocks".to_string(),
            created_at: None,
            created_by: None,
            metadata: String::new(),
        });
        let app = app_with(vec![blocked_issue, issue("a-2", "open", 2)]);
        let all = rows(&app);
        let labels: Vec<&str> = all
            .iter()
            .filter_map(|r| match r {
                Row::Group(l) => Some(*l),
                Row::Item(..) => None,
            })
            .collect();
        assert!(labels.contains(&"BLOCKED"));
        assert_eq!(row_count(&app), 2);
    }
}
