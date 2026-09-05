//! Tasks pane: issues grouped by status or by epic, with an optional
//! id/title search filter. This module is the single source of truth for
//! selectable-row order — `App::visible_row_count`/`selected_issue` and this
//! module's own `draw` all walk the same `rows()`, so selection, clamping,
//! and rendering can never disagree about what row N is.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::{App, GroupBy, Mode};
use crate::model::{Issue, Status};

use super::colors;

/// Issues untouched longer than this render dimmed (mock: `.row.stale`).
const STALE_AFTER_DAYS: i64 = 7;

enum Row<'a> {
    Group(Line<'a>),
    /// issue, blocked, indented-as-epic-child
    Item(&'a Issue, bool, bool),
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

fn rows_by_status(app: &App) -> Vec<Row<'_>> {
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
        out.push(Row::Group(Line::from(Span::styled(
            format!("▾ {label}"),
            Style::default().fg(colors::DIMMER),
        ))));
        for issue in group {
            out.push(Row::Item(issue, app.is_blocked(issue), false));
        }
    }
    out
}

/// Resolves an issue's epic: its `parent` field, falling back to the
/// dotted-id prefix (`x.1` → `x`) when `parent` is absent and an issue with
/// that id actually exists — a bare id-prefix match with no such issue is
/// just a coincidence, not a relationship.
fn effective_parent<'a>(issue: &'a Issue, app: &'a App) -> Option<&'a str> {
    if let Some(parent) = issue.parent.as_deref() {
        return Some(parent);
    }
    let prefix = issue.id.rfind('.').map(|idx| &issue.id[..idx])?;
    app.by_id.contains_key(prefix).then_some(prefix)
}

/// Epics are issues at least one other issue resolves to via
/// `effective_parent` — this covers both `issue_type: epic` and any task
/// that merely happens to have subtasks, matching what the spec calls "epic
/// tree (parent field...)" rather than trusting `issue_type` alone.
fn rows_by_epic(app: &App) -> Vec<Row<'_>> {
    let mut children: std::collections::HashMap<&str, Vec<&Issue>> =
        std::collections::HashMap::new();
    for issue in &app.issues {
        if let Some(parent) = effective_parent(issue, app) {
            children.entry(parent).or_default().push(issue);
        }
    }

    let mut epics: Vec<&Issue> = app
        .issues
        .iter()
        .filter(|issue| children.contains_key(issue.id.as_str()) || issue.issue_type == "epic")
        .collect();
    epics.sort_by(|a, b| a.id.cmp(&b.id));

    let mut out = Vec::new();
    for epic in &epics {
        let mut kids = children.remove(epic.id.as_str()).unwrap_or_default();
        kids.sort_by(|a, b| a.id.cmp(&b.id));
        out.push(Row::Group(Line::from(vec![
            Span::styled("▾ ", Style::default().fg(colors::DIMMER)),
            Span::styled(epic.id.clone(), Style::default().fg(colors::MAGENTA)),
            Span::styled(
                format!(" {}", epic.title),
                Style::default().fg(colors::DIMMER),
            ),
        ])));
        for child in kids {
            out.push(Row::Item(child, app.is_blocked(child), true));
        }
    }

    let epic_ids: std::collections::HashSet<&str> = epics.iter().map(|e| e.id.as_str()).collect();
    let mut orphans: Vec<&Issue> = app
        .issues
        .iter()
        .filter(|issue| !epic_ids.contains(issue.id.as_str()))
        .filter(|issue| {
            effective_parent(issue, app)
                .map(|parent| !epic_ids.contains(parent))
                .unwrap_or(true)
        })
        .collect();
    if !orphans.is_empty() {
        orphans.sort_by(|a, b| a.id.cmp(&b.id));
        out.push(Row::Group(Line::from(Span::styled(
            "▾ (no epic)",
            Style::default().fg(colors::DIMMER),
        ))));
        for issue in orphans {
            out.push(Row::Item(issue, app.is_blocked(issue), true));
        }
    }
    out
}

/// Drops group headers that end up with no matching children, so a live
/// search never leaves a heading floating over nothing.
fn apply_search<'a>(rows: Vec<Row<'a>>, query: &str) -> Vec<Row<'a>> {
    if query.is_empty() {
        return rows;
    }
    let query = query.to_lowercase();
    let mut out = Vec::with_capacity(rows.len());
    let mut pending_group = None;
    for row in rows {
        match row {
            Row::Group(_) => pending_group = Some(row),
            Row::Item(issue, ..) => {
                let haystack = format!("{} {}", issue.id, issue.title).to_lowercase();
                if !haystack.contains(&query) {
                    continue;
                }
                if let Some(group) = pending_group.take() {
                    out.push(group);
                }
                out.push(row);
            }
        }
    }
    out
}

fn rows(app: &App) -> Vec<Row<'_>> {
    let raw = match app.group_by {
        GroupBy::Status => rows_by_status(app),
        GroupBy::Epic => rows_by_epic(app),
    };
    apply_search(raw, &app.search)
}

/// Number of *selectable* rows (issues, not group headers) — used by `App`
/// to clamp `selected` after a refresh or a key press.
pub(crate) fn row_count(app: &App) -> usize {
    rows(app)
        .iter()
        .filter(|row| matches!(row, Row::Item(..)))
        .count()
}

/// The issue at `app.selected` under the current grouping/search — `None`
/// once the list is empty (e.g. a search with no matches).
pub(crate) fn selected_issue(app: &App) -> Option<&Issue> {
    rows(app)
        .into_iter()
        .filter_map(|row| match row {
            Row::Item(issue, ..) => Some(issue),
            Row::Group(_) => None,
        })
        .nth(app.selected)
}

fn is_stale(issue: &Issue) -> bool {
    issue
        .updated_at
        .map(|at| (chrono::Utc::now() - at).num_days() > STALE_AFTER_DAYS)
        .unwrap_or(false)
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

    for row in all_rows {
        match row {
            Row::Group(label) => items.push(ListItem::new(label)),
            Row::Item(issue, blocked, indented) => {
                selectable_positions.push(items.len());
                let (icon, icon_color) = glyph(&issue.status, blocked);
                let stale = is_stale(issue);
                let id_color = if stale { colors::DIMMER } else { colors::DIM };
                let closed = issue.status == Status::Closed;
                let title_color = if stale || closed {
                    colors::DIMMER
                } else {
                    colors::FG
                };
                let mut title_style = Style::default().fg(title_color);
                if closed {
                    title_style = title_style.add_modifier(Modifier::CROSSED_OUT);
                }

                let mut spans = vec![
                    Span::raw(if indented { "    " } else { "  " }),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    Span::styled(issue.id.clone(), Style::default().fg(id_color)),
                    Span::raw(" "),
                    Span::styled(issue.title.clone(), title_style),
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

    let mode_label = match app.group_by {
        GroupBy::Status => "status",
        GroupBy::Epic => "epic",
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER))
        .title(format!(" TASKS · {mode_label} "))
        .title_top(
            Line::from(Span::styled(
                "[e]pic [s]tatus",
                Style::default().fg(colors::DIMMER),
            ))
            .right_aligned(),
        );
    if app.mode == Mode::Search || !app.search.is_empty() {
        block = block.title_bottom(
            Line::from(Span::styled(
                format!("/{}", app.search),
                Style::default().fg(colors::YELLOW),
            ))
            .left_aligned(),
        );
    }
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

    fn group_labels(rows: &[Row<'_>]) -> Vec<String> {
        rows.iter()
            .filter_map(|r| match r {
                Row::Group(line) => Some(line.to_string()),
                Row::Item(..) => None,
            })
            .collect()
    }

    #[test]
    fn buckets_are_ordered_wip_blocked_open_closed() {
        let app = app_with(vec![
            issue("a-closed", "closed", 2),
            issue("a-open", "open", 2),
            issue("a-wip", "in_progress", 2),
        ]);
        let labels = group_labels(&rows(&app));
        assert_eq!(labels, vec!["▾ IN PROGRESS", "▾ OPEN", "▾ CLOSED"]);
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
        let labels = group_labels(&rows(&app));
        assert!(labels.contains(&"▾ BLOCKED".to_string()));
        assert_eq!(row_count(&app), 2);
    }

    #[test]
    fn epic_grouping_nests_children_and_buckets_orphans() {
        let mut epic = issue("proj-1", "open", 1);
        epic.issue_type = "epic".to_string();
        let mut child = issue("proj-1.1", "open", 2);
        child.parent = Some("proj-1".to_string());
        // Dotted-id fallback: no explicit `parent`, but `proj-1.2` prefix
        // resolves to an existing issue.
        let fallback_child = issue("proj-1.2", "open", 2);
        let orphan = issue("proj-9", "open", 3);

        let mut app = app_with(vec![epic, child, fallback_child, orphan]);
        app.group_by = GroupBy::Epic;

        let all = rows(&app);
        let labels = group_labels(&all);
        assert_eq!(labels, vec!["▾ proj-1 ", "▾ (no epic)"]);
        let item_ids: Vec<&str> = all
            .iter()
            .filter_map(|r| match r {
                Row::Item(issue, _, _) => Some(issue.id.as_str()),
                Row::Group(_) => None,
            })
            .collect();
        assert_eq!(item_ids, vec!["proj-1.1", "proj-1.2", "proj-9"]);
    }

    #[test]
    fn search_filters_items_and_drops_empty_groups() {
        let app = app_with(vec![issue("a-open", "open", 2), issue("b-open", "open", 2)]);
        let mut app = app;
        app.search = "a-open".to_string();
        let all = rows(&app);
        assert_eq!(row_count(&app), 1);
        let labels = group_labels(&all);
        assert_eq!(labels, vec!["▾ OPEN"]);
    }

    #[test]
    fn selected_issue_tracks_app_selected_through_filtering() {
        let mut app = app_with(vec![issue("a-open", "open", 2), issue("b-open", "open", 2)]);
        app.selected = 1;
        assert_eq!(selected_issue(&app).unwrap().id, "b-open");
        app.search = "a-open".to_string();
        app.selected = 0;
        assert_eq!(selected_issue(&app).unwrap().id, "a-open");
    }
}
