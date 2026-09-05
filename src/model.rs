//! Data types shared by the bd sources and (in later phases) the app/UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer};

/// Stored `status` values plus an `Other` catch-all for anything bd adds later.
/// `blocked` is not a stored status — it is derived from unmet dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Status {
    Open,
    InProgress,
    Closed,
    Deferred,
    Other(String),
}

impl Status {
    pub(crate) fn parse(raw: &str) -> Status {
        match raw {
            "open" => Status::Open,
            "in_progress" => Status::InProgress,
            "closed" => Status::Closed,
            "deferred" => Status::Deferred,
            other => Status::Other(other.to_string()),
        }
    }
}

impl Default for Status {
    fn default() -> Self {
        Status::Other(String::new())
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Status::parse(&raw))
    }
}

/// One `dependencies[]` element from `bd list --all --flat --json`. Only
/// `depends_on_id`/`dep_type` drive `is_blocked`; the rest is parsed for
/// schema completeness and stays unread until a dependency-detail view needs
/// "who added this link and when."
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Dependency {
    #[allow(dead_code)]
    pub(crate) issue_id: String,
    pub(crate) depends_on_id: String,
    #[serde(rename = "type")]
    pub(crate) dep_type: String,
    #[allow(dead_code)]
    pub(crate) created_at: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    pub(crate) created_by: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) metadata: String,
}

/// One issue from `bd list --all --flat --json`. Only `id` is guaranteed present.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Issue {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) status: Status,
    #[serde(default)]
    pub(crate) priority: Option<i32>,
    #[serde(default)]
    pub(crate) issue_type: String,
    #[serde(default)]
    pub(crate) parent: Option<String>,
    #[serde(default)]
    pub(crate) owner: Option<String>,
    #[serde(default)]
    pub(crate) created_by: Option<String>,
    #[serde(default)]
    pub(crate) created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub(crate) updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub(crate) dependencies: Vec<Dependency>,
    #[serde(default)]
    pub(crate) dependency_count: u32,
    #[serde(default)]
    pub(crate) dependent_count: u32,
    // No comment-count badge anywhere yet; kept parsed for when one lands.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) comment_count: u32,
    #[serde(default)]
    pub(crate) notes: Option<String>,
    #[serde(default)]
    pub(crate) design: Option<String>,
    #[serde(default)]
    pub(crate) acceptance_criteria: Option<String>,
}

/// `summary` object inside `bd status --json`. Header counts already know
/// blocked/ready, so we never derive them ourselves.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Counts {
    #[serde(default)]
    pub(crate) open_issues: u32,
    #[serde(default)]
    pub(crate) in_progress_issues: u32,
    #[serde(default)]
    pub(crate) blocked_issues: u32,
    #[serde(default)]
    pub(crate) closed_issues: u32,
    #[serde(default)]
    pub(crate) deferred_issues: u32,
    #[serde(default)]
    pub(crate) ready_issues: u32,
    #[serde(default)]
    pub(crate) total_issues: u32,
    // No header widget surfaces these three yet; kept parsed since they're
    // free on the wire and a future stats panel will want them.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) pinned_issues: u32,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) epics_eligible_for_closure: u32,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) average_lead_time_hours: f64,
}

/// One line of `.beads/interactions.jsonl`. Census across 1,129 real events:
/// `kind` is always `field_change`; creates/notes/comments never appear here.
/// Parse `kind` as a plain string so an unknown value never fails parsing.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AuditEvent {
    // Only used for schema completeness / potential future dedup; the feed
    // keys off `(issue_id, timestamp)`, not this id.
    #[allow(dead_code)]
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) created_at: DateTime<Utc>,
    #[serde(default)]
    pub(crate) actor: Option<String>,
    #[serde(default)]
    pub(crate) issue_id: Option<String>,
    #[serde(default)]
    pub(crate) extra: AuditExtra,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct AuditExtra {
    #[serde(default)]
    pub(crate) field: Option<String>,
    #[serde(default)]
    pub(crate) new_value: Option<String>,
    #[serde(default)]
    pub(crate) old_value: Option<String>,
    #[serde(default)]
    pub(crate) reason: Option<String>,
}

/// Who caused a feed event. A thin wrapper (rather than a bare `String`) so
/// the UI's actor->color hash has a single, stable type to key off.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Actor(pub(crate) String);

/// What changed. Audit events map onto `StatusChanged`/`PriorityChanged` when
/// their `field` matches, `FieldChanged` otherwise; the snapshot differ only
/// ever produces `Created`, `StatusChanged`, `PriorityChanged`, or `Touched`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Change {
    Created,
    StatusChanged {
        old: Status,
        new: Status,
    },
    PriorityChanged {
        old: Option<i32>,
        new: Option<i32>,
    },
    Touched,
    FieldChanged {
        field: String,
        old: Option<String>,
        new: Option<String>,
    },
}

/// One row in the merged, newest-first event feed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FeedEvent {
    pub(crate) issue_id: String,
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) actor: Option<Actor>,
    pub(crate) change: Change,
    pub(crate) reason: Option<String>,
    /// `true` when synthesized by the snapshot differ rather than read from
    /// the audit log; the UI renders derived events as dim italic `derived`.
    pub(crate) derived: bool,
}
