//! Live events pane + actor presence strip. The actor→color hash lives here
//! since it's only ever needed to render these two widgets.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use chrono::{DateTime, Utc};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::model::{Actor, Change, FeedEvent, Status};

use super::colors;

const ACTOR_COLORS: [Color; 6] = [
    colors::CYAN,
    colors::MAGENTA,
    colors::BLUE,
    colors::GREEN,
    colors::YELLOW,
    colors::RED,
];

/// Stable actor→color hash: `DefaultHasher::new()` uses fixed keys (not
/// randomized per-process), so the same actor name always lands on the same
/// color, run to run. Always hashes the full raw actor string, even where
/// the display label is shortened, so identity stays stable.
fn actor_color(actor: &Actor) -> Color {
    let mut hasher = DefaultHasher::new();
    actor.0.hash(&mut hasher);
    ACTOR_COLORS[(hasher.finish() % ACTOR_COLORS.len() as u64) as usize]
}

/// Compact display label for an actor: the email local part, preferring what
/// follows a `+` (GitHub noreply addresses like
/// `50377477+caretak3r@users.noreply.github.com` → `caretak3r`, not the
/// meaningless numeric id), else hard-truncates at a char boundary so the
/// board/feed panes stay narrow. Detail views should show `actor.0` in full.
pub(crate) fn short_name(actor: &Actor) -> &str {
    let raw = actor.0.as_str();
    if let Some(at) = raw.find('@') {
        let local = &raw[..at];
        return match local.find('+') {
            Some(plus) if plus + 1 < local.len() => &local[plus + 1..],
            _ => local,
        };
    }
    const MAX: usize = 24;
    match raw.char_indices().nth(MAX) {
        Some((byte_idx, _)) => &raw[..byte_idx],
        None => raw,
    }
}

fn fmt_age(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3600)
    }
}

pub(crate) fn draw_actors(f: &mut Frame, area: Rect, app: &App) {
    let now = Utc::now();
    let mut names: Vec<(&Actor, &DateTime<Utc>)> = app.actors.iter().collect();
    names.sort_by(|a, b| (a.0).0.cmp(&(b.0).0));

    let mut spans = Vec::new();
    for (actor, last_seen) in names {
        if !spans.is_empty() {
            spans.push(Span::raw("   "));
        }
        let age = (now - *last_seen).num_seconds();
        let live = age < 120;
        let selected = app.actor_filter.as_ref() == Some(actor);
        let dot_color = if live {
            actor_color(actor)
        } else {
            colors::DIM
        };
        let mut name_style = if live {
            Style::default().fg(colors::FG)
        } else {
            Style::default().fg(colors::DIM)
        };
        if selected {
            name_style = name_style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        }
        spans.push(Span::styled(
            if live { "●" } else { "○" },
            Style::default().fg(dot_color),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(short_name(actor).to_string(), name_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            fmt_age(age),
            Style::default().fg(colors::DIMMER),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER))
        .title(" ACTORS ")
        .title_top(
            Line::from(Span::styled(
                "[a]ctor filter",
                Style::default().fg(colors::DIMMER),
            ))
            .right_aligned(),
        );
    f.render_widget(
        Paragraph::new(Line::from(spans))
            .block(block)
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub(crate) fn status_label(status: &Status) -> &str {
    match status {
        Status::Open => "open",
        Status::InProgress => "in_progress",
        Status::Closed => "closed",
        Status::Deferred => "deferred",
        Status::Other(raw) => raw,
    }
}

fn fmt_prio(p: Option<i32>) -> String {
    p.map(|p| format!("P{p}"))
        .unwrap_or_else(|| "—".to_string())
}

pub(crate) fn change_label(change: &Change) -> String {
    match change {
        Change::Created => "created".to_string(),
        Change::StatusChanged { old, new } => {
            format!("status {} → {}", status_label(old), status_label(new))
        }
        Change::PriorityChanged { old, new } => {
            format!("priority {} → {}", fmt_prio(*old), fmt_prio(*new))
        }
        Change::Touched => "touched".to_string(),
        Change::FieldChanged { field, old, new } => format!(
            "{field} {} → {}",
            old.as_deref().unwrap_or("—"),
            new.as_deref().unwrap_or("—")
        ),
    }
}

fn event_lines(event: &FeedEvent, now: DateTime<Utc>) -> Vec<Line<'static>> {
    let age = (now - event.timestamp).num_seconds();
    let actor_name = event
        .actor
        .as_ref()
        .map(|a| short_name(a).to_string())
        .unwrap_or_else(|| "derived".to_string());
    let actor_style = match &event.actor {
        Some(actor) => Style::default().fg(actor_color(actor)),
        None => Style::default().fg(colors::DIMMER),
    };

    let head = Line::from(vec![
        Span::styled(fmt_age(age), Style::default().fg(colors::DIMMER)),
        Span::raw("  "),
        Span::styled(actor_name, actor_style),
        Span::raw("  "),
        Span::styled(event.issue_id.clone(), Style::default().fg(colors::DIM)),
    ]);

    let change_style = if event.derived {
        Style::default()
            .fg(colors::DIMMER)
            .add_modifier(Modifier::ITALIC)
    } else {
        Style::default().fg(colors::FG)
    };
    let mut change_text = change_label(&event.change);
    if event.derived {
        change_text.push_str(" · derived");
    }

    let mut lines = vec![
        head,
        Line::from(Span::styled(format!("  {change_text}"), change_style)),
    ];
    if let Some(reason) = &event.reason {
        lines.push(Line::from(Span::styled(
            format!("  \u{201c}{reason}\u{201d}"),
            Style::default()
                .fg(colors::DIM)
                .add_modifier(Modifier::ITALIC),
        )));
    }
    lines
}

pub(crate) fn draw_events(f: &mut Frame, area: Rect, app: &App) {
    let now = Utc::now();
    let mut lines = Vec::new();
    for event in app.feed.iter().filter(|e| matches_actor_filter(e, app)) {
        lines.extend(event_lines(event, now));
    }

    let title = match &app.actor_filter {
        Some(actor) => format!(" LIVE EVENTS · {} ", short_name(actor)),
        None => " LIVE EVENTS ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER))
        .title(title);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn matches_actor_filter(event: &FeedEvent, app: &App) -> bool {
    match &app.actor_filter {
        None => true,
        Some(filter) => event.actor.as_ref() == Some(filter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_color_is_stable_across_calls() {
        let a = Actor("sonnet-impl-2".to_string());
        assert_eq!(actor_color(&a), actor_color(&a));
    }

    #[test]
    fn short_name_prefers_username_over_noreply_id() {
        let gh = Actor("50377477+caretak3r@users.noreply.github.com".to_string());
        assert_eq!(short_name(&gh), "caretak3r");
        let plain = Actor("gudi.k.rohit@gmail.com".to_string());
        assert_eq!(short_name(&plain), "gudi.k.rohit");
        let bare = Actor("fable-orchestrator".to_string());
        assert_eq!(short_name(&bare), "fable-orchestrator");
        let long = Actor("x".repeat(40));
        assert_eq!(short_name(&long).chars().count(), 24);
    }

    #[test]
    fn fmt_age_buckets_seconds_minutes_hours() {
        assert_eq!(fmt_age(4), "4s");
        assert_eq!(fmt_age(187), "3m");
        assert_eq!(fmt_age(7384), "2h");
    }

    #[test]
    fn change_label_covers_every_change_variant() {
        assert_eq!(change_label(&Change::Created), "created");
        assert_eq!(
            change_label(&Change::StatusChanged {
                old: Status::Open,
                new: Status::Closed
            }),
            "status open → closed"
        );
        assert_eq!(
            change_label(&Change::PriorityChanged {
                old: Some(2),
                new: Some(0)
            }),
            "priority P2 → P0"
        );
        assert_eq!(change_label(&Change::Touched), "touched");
        assert_eq!(
            change_label(&Change::FieldChanged {
                field: "owner".to_string(),
                old: None,
                new: Some("bob".to_string())
            }),
            "owner — → bob"
        );
    }
}
