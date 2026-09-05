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
    OpenDetail,
    GroupEpic,
    GroupStatus,
    EnterSearch,
    CycleActor,
    /// `esc` in normal mode: closes an overlay-less clear, in priority order
    /// actor filter then leftover search text. Overlay/search-mode `esc` is
    /// handled directly by `App::handle_key`, not through this action.
    ClearFilters,
}

/// Pure key→action mapping for normal mode, kept separate from `App` so it
/// needs no terminal or state to unit test. Search/detail modes intercept
/// keys before this is consulted — see `App::handle_key`.
pub(crate) fn key_to_action(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::Down),
        KeyCode::Char('g') => Some(Action::Top),
        KeyCode::Char('G') => Some(Action::Bottom),
        KeyCode::Enter => Some(Action::OpenDetail),
        KeyCode::Esc => Some(Action::ClearFilters),
        KeyCode::Char('e') => Some(Action::GroupEpic),
        KeyCode::Char('s') => Some(Action::GroupStatus),
        KeyCode::Char('/') => Some(Action::EnterSearch),
        KeyCode::Char('a') => Some(Action::CycleActor),
        _ => None,
    }
}

/// How the tasks pane groups issues. Toggled by `e`/`s`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupBy {
    Status,
    Epic,
}

/// Input focus. While `Search`, printable keys edit the query instead of
/// dispatching through `key_to_action`; while `Detail`, only `esc` does
/// anything (closes the overlay) so the overlay can't be quit or navigated
/// out from under the user by an accidental keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Normal,
    Search,
    Detail,
}

pub(crate) struct App {
    /// The detail overlay (Phase 3) renders entirely from the loaded
    /// snapshot + feed ring — every field the mock's `bd show`/`bd history`
    /// panel needs is already in `Issue`/`FeedEvent`, so no shell-out ever
    /// needed `root`. Kept for a future feature that does need it.
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
    pub(crate) mode: Mode,
    pub(crate) group_by: GroupBy,
    pub(crate) search: String,
    pub(crate) actor_filter: Option<Actor>,
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
            mode: Mode::Normal,
            group_by: GroupBy::Status,
            search: String::new(),
            actor_filter: None,
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
            Action::OpenDetail => {
                if self.visible_row_count() > 0 {
                    self.mode = Mode::Detail;
                }
            }
            Action::GroupEpic => {
                self.group_by = GroupBy::Epic;
                self.clamp_selection();
            }
            Action::GroupStatus => {
                self.group_by = GroupBy::Status;
                self.clamp_selection();
            }
            Action::EnterSearch => self.mode = Mode::Search,
            Action::CycleActor => self.cycle_actor_filter(),
            Action::ClearFilters => {
                if self.actor_filter.take().is_none() && !self.search.is_empty() {
                    self.search.clear();
                    self.clamp_selection();
                }
            }
        }
    }

    /// Routes a raw key through the mode currently focused. Normal mode
    /// dispatches through `key_to_action`; search mode edits `self.search`
    /// directly (printable keys never reach `key_to_action` while typing);
    /// detail mode only honors `esc`.
    pub(crate) fn handle_key(&mut self, code: KeyCode) {
        match self.mode {
            Mode::Detail => {
                if code == KeyCode::Esc {
                    self.mode = Mode::Normal;
                }
            }
            Mode::Search => self.handle_search_key(code),
            Mode::Normal => {
                if let Some(action) = key_to_action(code) {
                    self.apply_action(action);
                }
            }
        }
    }

    fn handle_search_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.search.clear();
                self.mode = Mode::Normal;
                self.clamp_selection();
            }
            KeyCode::Enter => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.search.pop();
                self.clamp_selection();
            }
            KeyCode::Char(c) => {
                self.search.push(c);
                self.clamp_selection();
            }
            _ => {}
        }
    }

    /// `None` (all actors) -> each known actor, alphabetically -> back to
    /// `None`. Actor list is recomputed each cycle rather than cached, since
    /// `self.actors` only ever changes on refresh.
    fn cycle_actor_filter(&mut self) {
        let mut names: Vec<&Actor> = self.actors.keys().collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        self.actor_filter = match &self.actor_filter {
            None => names.first().map(|a| (*a).clone()),
            Some(current) => {
                let next = names
                    .iter()
                    .position(|a| *a == current)
                    .and_then(|idx| names.get(idx + 1));
                next.map(|a| (*a).clone())
            }
        };
    }

    pub(crate) fn visible_row_count(&self) -> usize {
        crate::ui::board::row_count(self)
    }

    /// The issue under the cursor in whatever grouping/search the board is
    /// currently showing — the single source the detail overlay reads from.
    pub(crate) fn selected_issue(&self) -> Option<&Issue> {
        crate::ui::board::selected_issue(self)
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
        assert_eq!(key_to_action(KeyCode::Esc), Some(Action::ClearFilters));
        assert_eq!(key_to_action(KeyCode::Enter), Some(Action::OpenDetail));
        assert_eq!(key_to_action(KeyCode::Char('e')), Some(Action::GroupEpic));
        assert_eq!(key_to_action(KeyCode::Char('s')), Some(Action::GroupStatus));
        assert_eq!(key_to_action(KeyCode::Char('/')), Some(Action::EnterSearch));
        assert_eq!(key_to_action(KeyCode::Char('a')), Some(Action::CycleActor));
    }

    #[test]
    fn search_mode_edits_query_instead_of_dispatching_actions() {
        let mut app = app_with(vec![issue("a-1", "open"), issue("b-2", "open")]);
        app.handle_key(KeyCode::Char('/'));
        assert_eq!(app.mode, Mode::Search);
        app.handle_key(KeyCode::Char('a'));
        app.handle_key(KeyCode::Char('-'));
        app.handle_key(KeyCode::Char('1'));
        assert_eq!(app.search, "a-1");
        // `j` must not move selection while typing.
        assert_eq!(app.selected, 0);
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.search, "a-1");
        app.handle_key(KeyCode::Esc);
        assert!(app.search.is_empty());
    }

    #[test]
    fn detail_mode_only_responds_to_esc() {
        let mut app = app_with(vec![issue("a-1", "open")]);
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.mode, Mode::Detail);
        app.handle_key(KeyCode::Char('q'));
        assert_eq!(
            app.mode,
            Mode::Detail,
            "quit must not reach the app while overlay is open"
        );
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn open_detail_is_a_no_op_with_no_selectable_rows() {
        let mut app = app_with(Vec::new());
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn actor_filter_cycles_alphabetically_then_wraps_to_all() {
        let mut app = app_with(vec![issue("a-1", "open")]);
        let t0 = Utc::now();
        app.actors.insert(Actor("bob".into()), t0);
        app.actors.insert(Actor("alice".into()), t0);

        app.apply_action(Action::CycleActor);
        assert_eq!(app.actor_filter, Some(Actor("alice".into())));
        app.apply_action(Action::CycleActor);
        assert_eq!(app.actor_filter, Some(Actor("bob".into())));
        app.apply_action(Action::CycleActor);
        assert_eq!(app.actor_filter, None);
    }

    #[test]
    fn clear_filters_clears_actor_before_search() {
        let mut app = app_with(vec![issue("a-1", "open")]);
        app.actor_filter = Some(Actor("bob".into()));
        app.search = "abc".into();
        app.apply_action(Action::ClearFilters);
        assert_eq!(app.actor_filter, None);
        assert_eq!(app.search, "abc", "first esc only clears the actor filter");
        app.apply_action(Action::ClearFilters);
        assert!(app.search.is_empty());
    }

    fn app_with(issues: Vec<Issue>) -> App {
        let mut app = App::new(PathBuf::from("/tmp"), "proj".into());
        app.apply_refresh(RefreshOutcome {
            issues,
            counts: Counts::default(),
            events: Vec::new(),
            malformed: 0,
            at: Utc::now(),
        });
        app
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
