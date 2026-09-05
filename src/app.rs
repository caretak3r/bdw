//! App state and the two reducers that mutate it: key presses and refresh
//! results. Both are kept as small, pure-ish functions/methods so they are
//! testable without a terminal or a running `bd`.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use crossterm::event::KeyCode;

use crate::model::{Actor, Counts, FeedEvent, Issue, Status};

/// Cap on the merged feed ring; oldest events fall off the back.
pub(crate) const FEED_CAP: usize = 500;

/// Everything one refresh cycle produces: a fresh bd snapshot, its counts,
/// and this cycle's audit + derived feed events (already newest-first).
pub(crate) struct RefreshOutcome {
    pub(crate) issues: Vec<Issue>,
    pub(crate) counts: Counts,
    pub(crate) events: Vec<FeedEvent>,
    pub(crate) malformed: usize,
    pub(crate) at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Quit,
    Up,
    Down,
    Top,
    Bottom,
}

/// Pure key→action mapping, kept separate from `App` so it needs no
/// terminal or state to unit test.
pub(crate) fn key_to_action(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::Down),
        KeyCode::Char('g') => Some(Action::Top),
        KeyCode::Char('G') => Some(Action::Bottom),
        _ => None,
    }
}

pub(crate) struct App {
    /// Unused until Phase 3's detail overlay shells out `bd show`/`bd
    /// history` for the selected issue.
    #[allow(dead_code)]
    pub(crate) root: PathBuf,
    pub(crate) project_name: String,
    pub(crate) issues: Vec<Issue>,
    pub(crate) by_id: HashMap<String, usize>,
    pub(crate) counts: Counts,
    pub(crate) feed: VecDeque<FeedEvent>,
    pub(crate) actors: HashMap<Actor, DateTime<Utc>>,
    pub(crate) selected: usize,
    pub(crate) last_update: Option<DateTime<Utc>>,
    pub(crate) malformed: usize,
    pub(crate) should_quit: bool,
}

impl App {
    pub(crate) fn new(root: PathBuf, project_name: String) -> Self {
        Self {
            root,
            project_name,
            issues: Vec::new(),
            by_id: HashMap::new(),
            counts: Counts::default(),
            feed: VecDeque::new(),
            actors: HashMap::new(),
            selected: 0,
            last_update: None,
            malformed: 0,
            should_quit: false,
        }
    }

    /// Rebuilds board state wholesale from a fresh bd snapshot and merges
    /// this cycle's events onto the feed ring. `issues`/`counts` are always
    /// replaced from the snapshot, never patched from events, so a missed or
    /// malformed event can never leave the board in a wrong state.
    pub(crate) fn apply_refresh(&mut self, outcome: RefreshOutcome) {
        self.counts = outcome.counts;
        self.malformed = outcome.malformed;
        self.last_update = Some(outcome.at);

        for event in &outcome.events {
            if let Some(actor) = &event.actor {
                let is_newer = self
                    .actors
                    .get(actor)
                    .map(|seen| event.timestamp > *seen)
                    .unwrap_or(true);
                if is_newer {
                    self.actors.insert(actor.clone(), event.timestamp);
                }
            }
        }
        // `outcome.events` is newest-first; pushing front in reverse (oldest
        // of the batch first) keeps the whole ring newest-first afterwards.
        for event in outcome.events.into_iter().rev() {
            self.feed.push_front(event);
        }
        while self.feed.len() > FEED_CAP {
            self.feed.pop_back();
        }

        self.issues = outcome.issues;
        self.by_id = self
            .issues
            .iter()
            .enumerate()
            .map(|(idx, issue)| (issue.id.clone(), idx))
            .collect();
        self.clamp_selection();
    }

    /// An issue is blocked when it is still open and has an unmet `blocks`
    /// dependency (the depended-on issue is not closed). Closed/in-progress
    /// issues never show the blocked glyph, even if a stale `blocks` link
    /// still points at something open.
    pub(crate) fn is_blocked(&self, issue: &Issue) -> bool {
        if issue.status != Status::Open {
            return false;
        }
        issue.dependencies.iter().any(|dep| {
            dep.dep_type == "blocks"
                && self
                    .by_id
                    .get(&dep.depends_on_id)
                    .map(|&idx| self.issues[idx].status != Status::Closed)
                    .unwrap_or(false)
        })
    }

    pub(crate) fn apply_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Up => self.selected = self.selected.saturating_sub(1),
            Action::Down => {
                let row_count = self.visible_row_count();
                if row_count > 0 {
                    self.selected = (self.selected + 1).min(row_count - 1);
                }
            }
            Action::Top => self.selected = 0,
            Action::Bottom => self.selected = self.visible_row_count().saturating_sub(1),
        }
    }

    pub(crate) fn visible_row_count(&self) -> usize {
        crate::ui::board::row_count(self)
    }

    fn clamp_selection(&mut self) {
        let row_count = self.visible_row_count();
        if self.selected >= row_count {
            self.selected = row_count.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Change, Dependency};

    fn issue(id: &str, status: &str) -> Issue {
        serde_json::from_value(serde_json::json!({ "id": id, "status": status })).unwrap()
    }

    fn feed_event(issue_id: &str, actor: Option<&str>, at: DateTime<Utc>) -> FeedEvent {
        FeedEvent {
            issue_id: issue_id.to_string(),
            timestamp: at,
            actor: actor.map(|a| Actor(a.to_string())),
            change: Change::Touched,
            reason: None,
            derived: false,
        }
    }

    #[test]
    fn key_to_action_maps_vim_and_arrow_keys() {
        assert_eq!(key_to_action(KeyCode::Char('q')), Some(Action::Quit));
        assert_eq!(key_to_action(KeyCode::Up), Some(Action::Up));
        assert_eq!(key_to_action(KeyCode::Char('k')), Some(Action::Up));
        assert_eq!(key_to_action(KeyCode::Down), Some(Action::Down));
        assert_eq!(key_to_action(KeyCode::Char('j')), Some(Action::Down));
        assert_eq!(key_to_action(KeyCode::Char('g')), Some(Action::Top));
        assert_eq!(key_to_action(KeyCode::Char('G')), Some(Action::Bottom));
        assert_eq!(key_to_action(KeyCode::Char('x')), None);
        assert_eq!(key_to_action(KeyCode::Esc), None);
    }

    #[test]
    fn apply_refresh_replaces_snapshot_and_merges_feed_newest_first() {
        let mut app = App::new(PathBuf::from("/tmp"), "proj".into());
        let t0 = Utc::now();
        app.apply_refresh(RefreshOutcome {
            issues: vec![issue("a-1", "open")],
            counts: Counts::default(),
            events: vec![
                feed_event("a-1", Some("alice"), t0 + chrono::Duration::seconds(2)),
                feed_event("a-1", Some("alice"), t0 + chrono::Duration::seconds(1)),
            ],
            malformed: 0,
            at: t0,
        });
        assert_eq!(app.issues.len(), 1);
        assert_eq!(app.feed.len(), 2);
        assert_eq!(app.feed[0].timestamp, t0 + chrono::Duration::seconds(2));
        assert_eq!(app.feed[1].timestamp, t0 + chrono::Duration::seconds(1));
        assert_eq!(
            app.actors.get(&Actor("alice".into())),
            Some(&(t0 + chrono::Duration::seconds(2)))
        );

        // A second, later refresh must fully replace `issues`/`counts` (never
        // patch from events) and prepend its events ahead of the old ones.
        app.apply_refresh(RefreshOutcome {
            issues: vec![issue("a-1", "closed"), issue("a-2", "open")],
            counts: Counts::default(),
            events: vec![feed_event("a-2", None, t0 + chrono::Duration::seconds(3))],
            malformed: 0,
            at: t0 + chrono::Duration::seconds(3),
        });
        assert_eq!(app.issues.len(), 2);
        assert_eq!(app.issues[0].status, Status::Closed);
        assert_eq!(app.feed.len(), 3);
        assert_eq!(app.feed[0].timestamp, t0 + chrono::Duration::seconds(3));
    }

    #[test]
    fn feed_ring_is_capped() {
        let mut app = App::new(PathBuf::from("/tmp"), "proj".into());
        let t0 = Utc::now();
        // `apply_refresh` expects the batch newest-first, matching what
        // `do_refresh` guarantees in production.
        let events = (0..(FEED_CAP + 10))
            .rev()
            .map(|i| feed_event("a-1", None, t0 + chrono::Duration::seconds(i as i64)))
            .collect();
        app.apply_refresh(RefreshOutcome {
            issues: vec![issue("a-1", "open")],
            counts: Counts::default(),
            events,
            malformed: 0,
            at: t0,
        });
        assert_eq!(app.feed.len(), FEED_CAP);
        // newest survives, oldest is evicted
        assert_eq!(
            app.feed.front().unwrap().timestamp,
            t0 + chrono::Duration::seconds((FEED_CAP + 9) as i64)
        );
    }

    #[test]
    fn is_blocked_true_only_for_open_issue_with_unmet_blocks_dependency() {
        fn with_dep(id: &str, status: &str, blocks_on: &str) -> Issue {
            let mut i = issue(id, status);
            i.dependencies.push(Dependency {
                issue_id: id.to_string(),
                depends_on_id: blocks_on.to_string(),
                dep_type: "blocks".to_string(),
                created_at: None,
                created_by: None,
                metadata: String::new(),
            });
            i
        }

        let mut app = App::new(PathBuf::from("/tmp"), "proj".into());
        app.apply_refresh(RefreshOutcome {
            issues: vec![
                with_dep("a-1", "open", "a-2"),
                issue("a-2", "open"),
                with_dep("a-3", "open", "a-4"),
                issue("a-4", "closed"),
                with_dep("a-5", "in_progress", "a-2"),
            ],
            counts: Counts::default(),
            events: Vec::new(),
            malformed: 0,
            at: Utc::now(),
        });

        let by_id = |id: &str| app.issues.iter().find(|i| i.id == id).unwrap();
        assert!(app.is_blocked(by_id("a-1")), "depends on open issue");
        assert!(
            !app.is_blocked(by_id("a-3")),
            "dependency is already closed"
        );
        assert!(
            !app.is_blocked(by_id("a-5")),
            "in_progress issues never show as blocked"
        );
    }
}
