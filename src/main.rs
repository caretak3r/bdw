mod model;
mod source;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Parser;

use source::bd::BdClient;
use source::events::{audit_to_feed_event, diff_snapshots, Tailer};

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

    if !cli.check {
        println!("TUI lands in Phase 2");
        return Ok(());
    }

    let runtime = tokio::runtime::Runtime::new().context("failed to start tokio runtime")?;
    runtime.block_on(run_check(root))
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
