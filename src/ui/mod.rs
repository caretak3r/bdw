//! Screen layout: header, 46/54 main split, footer keybar. The tasks/actors
//! and events panes own their own drawing; this module owns the outer grid
//! and the two single-line chrome bars, plus the shared color palette.

pub(crate) mod board;
pub(crate) mod detail;
pub(crate) mod feed;
pub(crate) mod markdown;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Mode, Orientation};

/// The three content rects for one frame, plus the `main` area they were
/// carved from — `main` is what mouse hit-testing measures drag position
/// against, so it travels with the panes it produced.
pub(crate) struct Regions {
    pub(crate) main: Rect,
    pub(crate) board: Rect,
    pub(crate) actors: Rect,
    pub(crate) feed: Rect,
}

/// Splits `main` into tasks/actors/events rects for the current orientation
/// and splitter ratio. Shared by `draw` (rendering) and the mouse handlers in
/// `main.rs` (hit-testing/dragging), so the two can never disagree about
/// where the splitter line actually is.
pub(crate) fn compute_regions(main: Rect, app: &App) -> Regions {
    let actor_h = actor_strip_height(app);
    match app.orientation {
        Orientation::Horizontal => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(app.split_ratio),
                    Constraint::Percentage(100 - app.split_ratio),
                ])
                .split(main);
            let left = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(actor_h)])
                .split(cols[0]);
            Regions {
                main,
                board: left[0],
                actors: left[1],
                feed: cols[1],
            }
        }
        Orientation::Vertical => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(app.split_ratio),
                    Constraint::Length(actor_h),
                    Constraint::Min(0),
                ])
                .split(main);
            Regions {
                main,
                board: rows[0],
                actors: rows[1],
                feed: rows[2],
            }
        }
    }
}

/// Column (Horizontal) or row (Vertical) where the tasks/events boundary
/// sits — the line a mouse drag grabs to resize.
fn splitter_coord(regions: &Regions, app: &App) -> u16 {
    match app.orientation {
        Orientation::Horizontal => regions.board.x + regions.board.width,
        Orientation::Vertical => regions.board.y + regions.board.height,
    }
}

/// Whether `(col, row)` is within one cell of the splitter line — the
/// tolerance covers both panes' adjoining border characters.
pub(crate) fn near_splitter(regions: &Regions, app: &App, col: u16, row: u16) -> bool {
    let target = splitter_coord(regions, app);
    let pos = match app.orientation {
        Orientation::Horizontal => col,
        Orientation::Vertical => row,
    };
    pos.abs_diff(target) <= 1
}

/// New split ratio for a drag to `(col, row)`, clamped to `MIN_SPLIT..=MAX_SPLIT`.
pub(crate) fn ratio_from_pos(regions: &Regions, app: &App, col: u16, row: u16) -> u16 {
    let (pos, origin, span) = match app.orientation {
        Orientation::Horizontal => (col, regions.main.x, regions.main.width),
        Orientation::Vertical => {
            let actor_h = actor_strip_height(app);
            (
                row,
                regions.main.y,
                regions.main.height.saturating_sub(actor_h),
            )
        }
    };
    let span = span.max(1);
    let offset = pos.saturating_sub(origin);
    let ratio = (offset as u32 * 100 / span as u32) as u16;
    ratio.clamp(crate::app::MIN_SPLIT, crate::app::MAX_SPLIT)
}

/// Whether terminal cell `(col, row)` falls inside `rect` — the point-in-rect
/// check mouse hit-testing needs for wheel/click routing between panes.
pub(crate) fn region_contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// Rows the "recently touched" banner needs this frame: 0 when nothing is
/// currently pulsing (so it takes no space at all), else one bordered row
/// per touched issue. Shared by `draw` and `main_area` so mouse hit-testing
/// never disagrees with what was actually rendered.
fn banner_height(app: &App) -> u16 {
    let n = app.active_board().len() as u16;
    if n == 0 {
        0
    } else {
        n + 2
    }
}

/// The content area between the 1-row header (plus banner, when present)
/// and the 1-row footer — shared by `draw` and `main.rs`'s mouse handler so
/// both agree on where the header/banner and footer end without duplicating
/// the split.
pub(crate) fn main_area(full: Rect, app: &App) -> Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(banner_height(app)),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(full)[2]
}

pub(crate) fn draw(f: &mut Frame, app: &App) {
    if let Some(bg) = app.theme.bg {
        f.render_widget(Block::default().style(Style::default().bg(bg)), f.area());
    }

    let banner_h = banner_height(app);
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(banner_h),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, outer[0], app);
    if banner_h > 0 {
        draw_banner(f, outer[1], app);
    }

    let regions = compute_regions(outer[2], app);
    board::draw(f, regions.board, app);
    feed::draw_actors(f, regions.actors, app);
    feed::draw_events(f, regions.feed, app);

    draw_footer(f, outer[3], app);

    if app.mode == Mode::Detail {
        detail::draw(f, f.area(), app);
    }
}

/// One bordered row of actors, growing by a row for every ~4 actors so a
/// long presence list doesn't get clipped.
fn actor_strip_height(app: &App) -> u16 {
    let rows = (app.actors.len() as u16 / 4).saturating_add(1);
    rows + 2
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(20)])
        .split(area);

    let counts = &app.counts;
    let line = Line::from(vec![
        Span::styled(
            app.project_name.clone(),
            Style::default()
                .fg(app.theme.fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} issues", counts.total_issues),
            Style::default().fg(app.theme.dim),
        ),
        Span::raw("  "),
        Span::styled(
            format!("○ {} open", counts.open_issues),
            Style::default().fg(app.theme.fg),
        ),
        Span::raw("  "),
        Span::styled(
            format!("◐ {} wip", counts.in_progress_issues),
            Style::default().fg(app.theme.yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("❄ {} blocked", counts.blocked_issues),
            Style::default().fg(app.theme.cyan),
        ),
        Span::raw("  "),
        Span::styled(
            format!("✓ {} closed", counts.closed_issues),
            Style::default().fg(app.theme.green),
        ),
    ]);
    f.render_widget(Paragraph::new(line), chunks[0]);

    let freshness = app
        .last_update
        .map(|at| (chrono::Utc::now() - at).num_seconds());
    let (upd_text, upd_style) = match freshness {
        Some(secs) if secs < 3 => (
            format!("updated {secs}s ago"),
            Style::default().fg(app.theme.green),
        ),
        Some(secs) => (
            format!("updated {secs}s ago"),
            Style::default().fg(app.theme.dimmer),
        ),
        None => (
            "updating…".to_string(),
            Style::default().fg(app.theme.dimmer),
        ),
    };
    let upd =
        Paragraph::new(Line::from(Span::styled(upd_text, upd_style))).alignment(Alignment::Right);
    f.render_widget(upd, chunks[1]);
}

/// ACTIVE board: every `in_progress` issue, persistently listed like an
/// airline departures board — not just whatever flashed in the last few
/// seconds. Freshly touched entries glow and float to the top; issues that
/// have been in progress a while but are quiet just sit there, still
/// visible. An issue that leaves `in_progress` (e.g. closes) gets one last
/// `BANNER_MS` grace period, still glowing, before it drops off.
fn draw_banner(f: &mut Frame, area: Rect, app: &App) {
    let now = chrono::Utc::now();
    let lines: Vec<Line> = app
        .active_board()
        .into_iter()
        .map(|issue| {
            let pulse = app.banner_pulse(&issue.id);
            let fg = crate::theme::lerp_rgb(app.theme.dim, app.theme.highlight_fg, pulse);
            let touched_at = app.issue_pulses.get(&issue.id).copied();
            let change = touched_at
                .and_then(|_| app.feed.iter().find(|e| e.issue_id == issue.id))
                .map(|e| feed::change_label(&e.change))
                .unwrap_or_else(|| feed::status_label(&issue.status).to_string());
            let age = touched_at
                .or(issue.updated_at)
                .map(|at| format!("{}s", (now - at).num_seconds().max(0)))
                .unwrap_or_else(|| "—".to_string());
            Line::from(vec![
                Span::styled(age, Style::default().fg(app.theme.dimmer)),
                Span::raw("  "),
                Span::styled(
                    issue.id.clone(),
                    Style::default().fg(fg).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(issue.title.clone(), Style::default().fg(fg)),
                Span::raw("  "),
                Span::styled(format!("· {change}"), Style::default().fg(app.theme.dim)),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.yellow))
        .title(" ACTIVE ");
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(46)])
        .split(area);

    let key_style = Style::default().fg(app.theme.dim);
    let line = Line::from(vec![
        Span::styled("↑↓/jk", key_style),
        Span::raw(" select  "),
        Span::styled("⏎", key_style),
        Span::raw(" detail  "),
        Span::styled("esc", key_style),
        Span::raw(" close/clear  "),
        Span::styled("e/s", key_style),
        Span::raw(" epic/status  "),
        Span::styled("v", key_style),
        Span::raw(" orientation  "),
        Span::styled("t", key_style),
        Span::raw(" theme  "),
        Span::styled("/", key_style),
        Span::raw(" search  "),
        Span::styled("a", key_style),
        Span::raw(" actor  "),
        Span::styled("g/G", key_style),
        Span::raw(" top/bottom  "),
        Span::styled("q", key_style),
        Span::raw(" quit"),
    ]);
    f.render_widget(Paragraph::new(line), chunks[0]);

    let ro = Paragraph::new(Line::from(Span::styled(
        "bd --readonly · watch: .beads/ · debounce 250ms",
        Style::default().fg(app.theme.dimmer),
    )))
    .alignment(Alignment::Right);
    f.render_widget(ro, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::RefreshOutcome;
    use crate::model::{Counts, Issue};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;

    fn issue(id: &str, status: &str) -> Issue {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "status": status,
            "title": "Fix the thing",
        }))
        .unwrap()
    }

    /// Headless render smoke test: the project currently has no other gate
    /// that catches a `draw` panic (bad `Rect` split, out-of-range slice,
    /// ...), so this is the one check that actually calls it.
    #[test]
    fn draw_renders_project_name_bucket_label_and_glyph() {
        let mut app = App::new(PathBuf::from("/tmp"), "voltrol".into());
        app.apply_refresh(RefreshOutcome {
            issues: vec![issue("v-1", "open")],
            counts: Counts::default(),
            events: Vec::new(),
            malformed: 0,
            at: chrono::Utc::now(),
        });

        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(rendered.contains("voltrol"));
        assert!(rendered.contains("OPEN"));
        assert!(rendered.contains('○'));
    }

    #[test]
    fn horizontal_orientation_places_board_and_feed_side_by_side() {
        let app = App::new(PathBuf::from("/tmp"), "voltrol".into());
        let main = Rect::new(0, 0, 100, 40);
        let regions = compute_regions(main, &app);
        assert_eq!(regions.board.y, main.y);
        assert_eq!(regions.feed.y, main.y);
        assert_eq!(regions.feed.x, regions.board.x + regions.board.width);
    }

    #[test]
    fn vertical_orientation_stacks_board_above_feed() {
        let mut vertical = App::new(PathBuf::from("/tmp"), "voltrol".into());
        vertical.apply_action(crate::app::Action::ToggleOrientation);
        let main = Rect::new(0, 0, 100, 40);
        let regions = compute_regions(main, &vertical);
        assert_eq!(regions.board.y, main.y);
        assert_eq!(regions.board.x, main.x);
        assert!(regions.feed.y > regions.board.y);
        assert_eq!(
            regions.board.width, main.width,
            "vertical panes span full width"
        );
        assert_eq!(regions.feed.width, main.width);
    }

    #[test]
    fn near_splitter_detects_boundary_within_one_cell() {
        let app = App::new(PathBuf::from("/tmp"), "voltrol".into());
        let main = Rect::new(0, 0, 100, 40);
        let regions = compute_regions(main, &app);
        let boundary = regions.board.x + regions.board.width;
        assert!(near_splitter(&regions, &app, boundary, 5));
        assert!(near_splitter(&regions, &app, boundary - 1, 5));
        assert!(!near_splitter(&regions, &app, boundary + 5, 5));
    }

    #[test]
    fn ratio_from_pos_clamps_to_bounds() {
        let app = App::new(PathBuf::from("/tmp"), "voltrol".into());
        let main = Rect::new(0, 0, 100, 40);
        let regions = compute_regions(main, &app);
        assert_eq!(ratio_from_pos(&regions, &app, 0, 5), crate::app::MIN_SPLIT);
        assert_eq!(
            ratio_from_pos(&regions, &app, 100, 5),
            crate::app::MAX_SPLIT
        );
        let mid = ratio_from_pos(&regions, &app, 50, 5);
        assert!((45..=55).contains(&mid));
    }
}
