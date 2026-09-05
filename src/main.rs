mod app;
mod model;
mod source;
mod ui;

use std::collections::HashMap;
use std::io::Stdout;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::Parser;
use crossterm::event::{Event, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, RefreshOutcome};
use source::bd::BdClient;
use source::events::{audit_to_feed_event, diff_snapshots, Tailer};

/// Never invoke `bd` more than once per second, even under a storm of fs
/// events or manual repeated saves.
const REFRESH_FLOOR: Duration = Duration::from_secs(1);
/// How long to wait after the last fs event before refreshing.
const DEBOUNCE: Duration = Duration::from_millis(250);
/// Fallback refresh cadence when nothing has touched `.beads/`.
const FALLBACK_TICK: Duration = Duration::from_secs(10);

/// Real-time terminal dashboard for one beads (bd) project.
#[derive(Parser)]
#[command(name = "bdw")]
struct Cli {
    /// Directory inside (or above) the bd project. Defaults to the current directory.
    path: Option<PathBuf>,

    /// Run one refresh and print a summary instead of launching the TUI.
    #[arg(long)]
    check: bool,
}

/// Walks up from `start` looking for a directory containing `.beads/`.
fn discover_root(start: &Path) -> Result<PathBuf> {
    let start = start
        .canonicalize()
        .with_context(|| format!("cannot resolve path: {}", start.display()))?;

    let mut dir = start.as_path();
    loop {
        if dir.join(".beads").is_dir() {
            return Ok(dir.to_path_buf());
        }
        dir = match dir.parent() {
            Some(parent) => parent,
            None => bail!("no .beads/ directory found above {}", start.display()),
        };
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("bdw: error: {err:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let start = cli.path.unwrap_or_else(|| PathBuf::from("."));
    let root = discover_root(&start)?;

    let runtime = tokio::runtime::Runtime::new().context("failed to start tokio runtime")?;
    if cli.check {
        return runtime.block_on(run_check(root));
    }
    runtime.block_on(run_tui(root))
}

async fn run_check(root: PathBuf) -> Result<()> {
    let client = BdClient::new(root.clone());
    let issues = client.snapshot().await.context("bd list failed")?;
    let counts = client.counts().await.context("bd status failed")?;

    let mut tailer = Tailer::new();
    let tail = tailer
        .read_new(&client.interactions_path())
        .context("reading interactions.jsonl failed")?;

    let next: HashMap<String, model::Issue> = issues
        .iter()
        .cloned()
        .map(|issue| (issue.id.clone(), issue))
        .collect();
    let prev: HashMap<String, model::Issue> = HashMap::new();

    let mut feed_events: Vec<_> = tail.events.iter().map(audit_to_feed_event).collect();
    feed_events.extend(diff_snapshots(&prev, &next, &tail.events));

    println!("root: {}", root.display());
    println!("issues parsed: {}", issues.len());
    println!(
        "counts: open={} in_progress={} blocked={} closed={} deferred={} ready={} total={}",
        counts.open_issues,
        counts.in_progress_issues,
        counts.blocked_issues,
        counts.closed_issues,
        counts.deferred_issues,
        counts.ready_issues,
        counts.total_issues,
    );
    println!("audit events tailed: {}", tail.events.len());
    println!("malformed audit lines: {}", tail.malformed);
    println!(
        "feed events (audit + derived vs empty prev): {}",
        feed_events.len()
    );
    Ok(())
}

// ── TUI wiring ──────────────────────────────────────────────────────────

type Term = Terminal<CrosstermBackend<Stdout>>;

async fn run_tui(root: PathBuf) -> Result<()> {
    let project_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    let mut terminal = setup_terminal()?;
    let app = App::new(root.clone(), project_name);
    let result = app_loop(&mut terminal, app, root).await;
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<Term> {
    crossterm::terminal::enable_raw_mode().context("enabling raw mode")?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)
        .context("entering alternate screen")?;
    install_panic_hook();
    Terminal::new(CrosstermBackend::new(stdout)).context("creating terminal")
}

/// Undoes `setup_terminal`. Also installed as a panic hook so a mid-draw
/// panic never leaves the user's shell in raw/alternate-screen mode.
fn restore_terminal(terminal: &mut Term) -> Result<()> {
    crossterm::terminal::disable_raw_mode().context("disabling raw mode")?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )
    .context("leaving alternate screen")?;
    Ok(())
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        default_hook(info);
    }));
}

/// tokio::select! over three sources: terminal input, refresh results, and a
/// 1s tick (redraws the header's "updated Ns ago" even when nothing else
/// changed).
async fn app_loop(terminal: &mut Term, mut app: App, root: PathBuf) -> Result<()> {
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<Event>(64);
    spawn_input_reader(input_tx);

    let (refresh_tx, mut refresh_rx) = tokio::sync::mpsc::channel::<RefreshOutcome>(8);
    tokio::spawn(refresh_loop(root, refresh_tx));

    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    terminal
        .draw(|f| ui::draw(f, &app))
        .context("initial draw")?;

    loop {
        tokio::select! {
            Some(event) = input_rx.recv() => {
                if let Event::Key(key) = event {
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key.code);
                    }
                }
            }
            Some(outcome) = refresh_rx.recv() => {
                app.apply_refresh(outcome);
            }
            _ = ticker.tick() => {}
        }

        terminal.draw(|f| ui::draw(f, &app)).context("draw")?;
        if app.should_quit {
            break;
        }
    }
    Ok(())
}

/// `crossterm::event::read()` is blocking, so it gets its own OS thread
/// rather than fighting the tokio runtime for a worker.
fn spawn_input_reader(tx: tokio::sync::mpsc::Sender<Event>) {
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            if tx.blocking_send(event).is_err() {
                break;
            }
        }
    });
}

/// notify watch on `.beads/` (non-recursive — `last-touched` and
/// `interactions.jsonl` live directly under it; the heavier
/// `embeddeddolt/` subtree would just add debounce noise) → 250ms debounce
/// → refresh, plus a 10s fallback tick and a 1/s refresh floor.
async fn refresh_loop(root: PathBuf, tx: tokio::sync::mpsc::Sender<RefreshOutcome>) {
    let client = BdClient::new(root.clone());
    let mut tailer = Tailer::new();
    let mut prev: HashMap<String, model::Issue> = HashMap::new();

    let (dirty_tx, mut dirty_rx) = tokio::sync::mpsc::channel::<()>(64);
    // Kept alive for the loop's lifetime; if setup fails, `dirty_rx` just
    // never receives anything and we fall back to the 10s tick alone.
    let _watcher = spawn_watcher(root.join(".beads"), dirty_tx);

    if let Ok(outcome) = do_refresh(&client, &mut tailer, &mut prev).await {
        if tx.send(outcome).await.is_err() {
            return;
        }
    }
    let mut last_run = tokio::time::Instant::now();
    let mut fallback = tokio::time::interval(FALLBACK_TICK);
    fallback.reset();

    loop {
        tokio::select! {
            _ = fallback.tick() => {}
            Some(()) = dirty_rx.recv() => {
                debounce(&mut dirty_rx, DEBOUNCE).await;
            }
        }

        let wait = floor_wait(last_run.elapsed(), REFRESH_FLOOR);
        if wait > Duration::ZERO {
            tokio::time::sleep(wait).await;
        }
        last_run = tokio::time::Instant::now();

        // A transient bd failure (e.g. a concurrent writer briefly holding a
        // lock) is silently retried on the next signal or fallback tick.
        if let Ok(outcome) = do_refresh(&client, &mut tailer, &mut prev).await {
            if tx.send(outcome).await.is_err() {
                break;
            }
        }
    }
}

/// Drains `dirty_rx` until `delay` passes with no new signal, coalescing a
/// burst of fs events into a single refresh.
async fn debounce(dirty_rx: &mut tokio::sync::mpsc::Receiver<()>, delay: Duration) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(delay) => break,
            more = dirty_rx.recv() => {
                if more.is_none() {
                    break;
                }
            }
        }
    }
}

/// How much longer to wait before the next refresh is allowed to run.
fn floor_wait(elapsed_since_last: Duration, floor: Duration) -> Duration {
    floor.saturating_sub(elapsed_since_last)
}

fn spawn_watcher(
    beads_dir: PathBuf,
    tx: tokio::sync::mpsc::Sender<()>,
) -> notify::Result<notify::RecommendedWatcher> {
    use notify::Watcher;

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.blocking_send(());
        }
    })?;
    watcher.watch(&beads_dir, notify::RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

async fn do_refresh(
    client: &BdClient,
    tailer: &mut Tailer,
    prev: &mut HashMap<String, model::Issue>,
) -> Result<RefreshOutcome> {
    let issues = client.snapshot().await.context("bd list failed")?;
    let counts = client.counts().await.context("bd status failed")?;
    let tail = tailer
        .read_new(&client.interactions_path())
        .context("reading interactions.jsonl failed")?;

    let next: HashMap<String, model::Issue> = issues
        .iter()
        .cloned()
        .map(|issue| (issue.id.clone(), issue))
        .collect();

    let mut events: Vec<model::FeedEvent> = tail.events.iter().map(audit_to_feed_event).collect();
    events.extend(diff_snapshots(prev, &next, &tail.events));
    events.sort_by_key(|e| std::cmp::Reverse(e.timestamp));

    *prev = next;

    Ok(RefreshOutcome {
        issues,
        counts,
        events,
        malformed: tail.malformed,
        at: chrono::Utc::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_wait_is_zero_once_the_floor_has_elapsed() {
        assert_eq!(
            floor_wait(Duration::from_millis(1500), REFRESH_FLOOR),
            Duration::ZERO
        );
    }

    #[test]
    fn floor_wait_is_the_remainder_before_the_floor() {
        assert_eq!(
            floor_wait(Duration::from_millis(400), REFRESH_FLOOR),
            Duration::from_millis(600)
        );
    }
}
