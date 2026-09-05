# bdw — beads watch TUI: implementation spec

Single Rust binary `bdw`. Real-time terminal dashboard for one beads (bd) project:
task board + epic tree on the left, live agent-event feed on the right, actor
presence strip, detail overlay. Multiple agents mutate the bd database
concurrently at different times; bdw shows what is happening at a glance.

Repo: `/Users/rohit/Documents/bdw/`. Visual contract: open `docs/tui-mock.html`
in a browser — the built TUI must match that layout, glyphs, and color language.

## Hard constraints

- **Never open the Dolt database directly.** bd stores state in embedded Dolt
  (`.beads/embeddeddolt/`). The only data access is shelling out to the `bd`
  CLI with `--readonly --json` (plus `-C <root>`). This is lock-safe against
  concurrent agent writes and stable across bd versions.
- **State is always rebuilt from bd snapshots, never mutated from events.**
  A missed or malformed event must never corrupt the board.
- **No emoji icons anywhere.** Status glyphs: `○` open, `◐` in_progress,
  `❄` blocked, `✓` closed. Priority: `● P0`..`● P4` (P0 red, P1 orange, rest dim).
- Refresh floor: never invoke bd more than once per second.
- Total budget ≈ 1,800 LOC. No feature not in this spec.

## Verified bd 1.1.2 facts (do not re-derive; fixtures in `tests/fixtures/`)

- Global flags: `--json`, `--readonly`, `-C <dir>`, `--actor <name>`.
- `bd list --all --flat --json` → array of issues. Fields (all optional except
  `id`; use `#[serde(default)]`, ignore unknown): `id`, `title`, `description`,
  `status`, `priority` (int), `issue_type`, `parent`, `owner`, `created_by`,
  `created_at`, `updated_at`, `dependencies`, `dependency_count`,
  `dependent_count`, `comment_count`, `notes`, `design`, `acceptance_criteria`.
  Fixture: `tests/fixtures/list_voltrol.json` (real data — inspect it for the
  `dependencies` element shape before typing it; fall back to
  `serde_json::Value` only if the shape varies).
- Observed `status` strings: `open`, `in_progress`, `closed`, `deferred`.
  `blocked` is NOT a stored status — derive it (open + unmet dependency).
  Parse into `enum Status { Open, InProgress, Closed, Deferred, Other(String) }`.
- `bd status --json` → `{ schema_version, summary: { open_issues, in_progress_issues,
  blocked_issues, closed_issues, ready_issues, total_issues, ... } }`.
  Fixture: `tests/fixtures/status_voltrol.json`. Use its counts for the header
  (it already knows blocked/ready).
- `.beads/interactions.jsonl` — append-only audit log. One JSON object per line:
  `{"id":"int-<hex>","kind":"field_change","created_at":"<RFC3339>","actor":"<name>",
  "issue_id":"<bead-id>","extra":{"field":"status","new_value":"closed",
  "old_value":"open","reason":"<free text>"}}`.
  **Census across 1,129 real events: `kind` is always `field_change`.** Creates,
  notes, and comments never appear here — the snapshot differ (below) is the only
  source for those. Parse `kind` as `String`; unknown kinds must not error.
  Fixture: `tests/fixtures/interactions_voltrol.jsonl`.
- `.beads/last-touched` is touched on writes — cheap change signal.
- `bd show <id> --json` and `bd history <id>` exist for the detail overlay
  (try `--json` on history; if output isn't JSON, render the raw lines).

## Architecture

```
notify watch .beads/ ──┐
10s fallback tick ─────┤→ debounce 250ms → refresh:
                       │     bd list --all --flat --json   → Snapshot
                       │     bd status --json              → Counts
                       │     tail interactions.jsonl       → Vec<AuditEvent>
                       │     diff(prev, next snapshot)     → Vec<FeedEvent> (derived)
                       └──────────────→ mpsc → App reducer → ratatui draw
```

Two event sources merged into one feed, newest first:
1. **Audit tail**: high-water byte offset into `interactions.jsonl`. On refresh,
   if file len < offset → reset offset to 0 (rotation guard). Parse new lines;
   skip malformed lines, keep a skipped-count. Rich: has `actor` + `reason`.
2. **Snapshot differ**: compare `HashMap<id, Issue>` prev vs next → synthesize
   `Created`, `StatusChanged`, `PriorityChanged`, generic `Touched`
   (updated_at moved). Tag `derived: true`, actor = issue `owner`/`created_by`
   when the event is `Created`, else `None` (render as dim italic `derived`).
   Suppress a derived event when an audit event for the same issue+field arrived
   in the same refresh (audit wins — it has reason/actor).

## Crates

ratatui 0.29, crossterm 0.28, tokio (rt-multi-thread, macros, process, sync,
time), notify 7, serde + derive, serde_json, clap 4 derive, anyhow (main),
thiserror (source errors), chrono (serde). Nothing else without a reason
recorded in the phase bead.

## Module layout

```
src/main.rs        CLI (clap): `bdw [path] [--check]`; discover .beads/ walking
                   up from cwd; terminal setup/teardown; tokio runtime; app loop.
src/model.rs       Issue, Status, Counts, AuditEvent, FeedEvent, Change, Actor.
src/source/bd.rs   BdClient: tokio::process bd invocations, 10s timeout,
                   thiserror BdError { Spawn, NonZero{status,stderr}, Parse }.
src/source/events.rs  JSONL tailer (offset, rotation guard) + snapshot differ.
src/app.rs         App state + reducer: snapshot, feed ring (cap 500), selection,
                   grouping mode (epic|status), search filter, actor filter,
                   actor presence (last-seen per actor).
src/ui/mod.rs      layout: header, main split 46/54, footer keybar.
src/ui/board.rs    tasks pane: epic tree (parent field; dotted-id fallback
                   `x.1` → parent `x`) and status grouping; selection; search.
src/ui/feed.rs     events pane + actors strip; stable actor→color hash
                   (cyan, magenta, blue, green, yellow, red cycle).
src/ui/detail.rs   overlay: bd show + bd history for selected issue, scrollable.
```

`--check` mode (no TUI): run one refresh against the target project, print
`counts`, issue total, feed events parsed, malformed-line count, and exit 0/1.
This is the smoke gate for scripts and phase verification.

## Keys

`q` quit · `↑↓`/`jk` select · `⏎` detail · `esc` close overlay / clear search ·
`e` epic grouping · `s` status grouping · `/` incremental search ·
`a` cycle actor filter (all → each actor) · `g/G` top/bottom.

## Phases (one bead each; gate before the next)

**Phase 1 — data layer.** Files: `Cargo.toml`, `src/main.rs` (CLI + `--check`
only, no TUI), `src/model.rs`, `src/source/bd.rs`, `src/source/events.rs`.
Unit tests inline against `tests/fixtures/`; plus `tests/integration.rs`:
if `bd` is on PATH → `bd init` a tempdir, create/close issues via
`std::process` with `--actor test-actor`, assert BdClient snapshot + tailer
see them; else print skip and pass.

**Phase 2 — app core + TUI shell.** Files: `src/app.rs`, `src/ui/mod.rs`,
`src/ui/board.rs`, `src/ui/feed.rs`, edit `src/main.rs`. Watcher + debounce +
event loop (tokio select: input stream, refresh channel, 1s tick). Header,
board (status grouping), feed, actors, keybar. Reducer unit tests.

**Phase 3 — detail overlay, epic tree, search, actor filter, polish.**
Files: `src/ui/detail.rs`, edits to `app.rs`, `board.rs`, `feed.rs`,
`src/ui/mod.rs`. Staleness dimming (>7d untouched), closed-row strikethrough,
feed flash on arrival is optional — skip if it fights ratatui.

**Gate for every phase** (all must pass, run from repo root):
```
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo run -- /Users/rohit/Documents/voltrol --check   # phases 1-3
```

## Code standards

- No `unwrap()`/`expect()` outside tests; `?` + context everywhere.
- Exhaustive matches on our own enums — no `_` arm on `Status`/`Change`.
- `pub(crate)` by default; comments only for non-obvious constraints, written
  like a human; rustfmt defaults; clippy clean with `-D warnings`.
- Commits: Conventional Commits, small. **Never add AI/Claude attribution
  trailers, `Co-Authored-By`, or session links.** No pushes, no remotes.

## Task tracking

Phase bead IDs are given in the worker prompt. At start:
`bd -C /Users/rohit/Documents/bdw update <bead> --status in_progress --actor <your-worker-name>`.
Append a completion note with the verification commands actually run and their
results: `bd -C /Users/rohit/Documents/bdw note <bead> "<text>" --actor <your-worker-name>`.
Do not close beads; the orchestrator closes after review. Never run
`bd migrate`, `bd dolt push`, or any git push.
