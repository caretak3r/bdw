//! Screen layout: header, 46/54 main split, footer keybar. The tasks/actors
//! and events panes own their own drawing; this module owns the outer grid
//! and the two single-line chrome bars, plus the shared color palette.

pub(crate) mod board;
pub(crate) mod feed;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

/// Colors lifted from `docs/tui-mock.html`'s CSS variables so the TUI reads
/// as the same palette in a real terminal.
pub(crate) mod colors {
    use ratatui::style::Color;

    pub(crate) const FG: Color = Color::Rgb(0xc9, 0xd1, 0xd9);
    pub(crate) const DIM: Color = Color::Rgb(0x6e, 0x76, 0x81);
    pub(crate) const DIMMER: Color = Color::Rgb(0x48, 0x4f, 0x58);
    pub(crate) const BORDER: Color = Color::Rgb(0x30, 0x36, 0x3d);
    pub(crate) const GREEN: Color = Color::Rgb(0x3f, 0xb9, 0x50);
    pub(crate) const YELLOW: Color = Color::Rgb(0xd2, 0x99, 0x22);
    pub(crate) const RED: Color = Color::Rgb(0xf8, 0x51, 0x49);
    pub(crate) const ORANGE: Color = Color::Rgb(0xdb, 0x6d, 0x28);
    pub(crate) const CYAN: Color = Color::Rgb(0x39, 0xc5, 0xcf);
    pub(crate) const BLUE: Color = Color::Rgb(0x58, 0xa6, 0xff);
    pub(crate) const MAGENTA: Color = Color::Rgb(0xbc, 0x8c, 0xff);
    pub(crate) const SEL_BG: Color = Color::Rgb(0x1c, 0x2a, 0x3a);
}

pub(crate) fn draw(f: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, outer[0], app);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(outer[1]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(actor_strip_height(app)),
        ])
        .split(main[0]);

    board::draw(f, left[0], app);
    feed::draw_actors(f, left[1], app);
    feed::draw_events(f, main[1], app);

    draw_footer(f, outer[2]);
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
            Style::default().fg(colors::FG).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} issues", counts.total_issues),
            Style::default().fg(colors::DIM),
        ),
        Span::raw("  "),
        Span::styled(
            format!("○ {} open", counts.open_issues),
            Style::default().fg(colors::FG),
        ),
        Span::raw("  "),
        Span::styled(
            format!("◐ {} wip", counts.in_progress_issues),
            Style::default().fg(colors::YELLOW),
        ),
        Span::raw("  "),
        Span::styled(
            format!("❄ {} blocked", counts.blocked_issues),
            Style::default().fg(colors::CYAN),
        ),
        Span::raw("  "),
        Span::styled(
            format!("✓ {} closed", counts.closed_issues),
            Style::default().fg(colors::GREEN),
        ),
    ]);
    f.render_widget(Paragraph::new(line), chunks[0]);

    let freshness = app
        .last_update
        .map(|at| (chrono::Utc::now() - at).num_seconds());
    let (upd_text, upd_style) = match freshness {
        Some(secs) if secs < 3 => (
            format!("updated {secs}s ago"),
            Style::default().fg(colors::GREEN),
        ),
        Some(secs) => (
            format!("updated {secs}s ago"),
            Style::default().fg(colors::DIMMER),
        ),
        None => ("updating…".to_string(), Style::default().fg(colors::DIMMER)),
    };
    let upd =
        Paragraph::new(Line::from(Span::styled(upd_text, upd_style))).alignment(Alignment::Right);
    f.render_widget(upd, chunks[1]);
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(46)])
        .split(area);

    let key_style = Style::default().fg(colors::DIM);
    let line = Line::from(vec![
        Span::styled("↑↓/jk", key_style),
        Span::raw(" select   "),
        Span::styled("g/G", key_style),
        Span::raw(" top/bottom   "),
        Span::styled("q", key_style),
        Span::raw(" quit"),
    ]);
    f.render_widget(Paragraph::new(line), chunks[0]);

    let ro = Paragraph::new(Line::from(Span::styled(
        "bd --readonly · watch: .beads/ · debounce 250ms",
        Style::default().fg(colors::DIMMER),
    )))
    .alignment(Alignment::Right);
    f.render_widget(ro, chunks[1]);
}
