//! Two derived-event sources: the audit-log tailer (rich: actor + reason)
//! and the snapshot differ (cheap fallback for anything the audit log
//! doesn't cover, e.g. creates).

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result};

use crate::model::{Actor, AuditEvent, Change, FeedEvent, Issue, Status};

pub(crate) struct TailResult {
    pub(crate) events: Vec<AuditEvent>,
    pub(crate) malformed: usize,
}

/// Byte-offset tail of `interactions.jsonl`. Holds no file handle between
/// calls — `.beads/` is rewritten by other processes, so we always reopen.
pub(crate) struct Tailer {
    offset: u64,
}

impl Tailer {
    pub(crate) fn new() -> Self {
        Self { offset: 0 }
    }

    /// Reads whatever complete lines have been appended since the last call.
    /// A missing file is not an error (report zero events); a file shorter
    /// than our offset means it was rotated or truncated underneath us, so
    /// we reset to the start rather than seeking past the end.
    pub(crate) fn read_new(&mut self, path: &Path) -> Result<TailResult> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                self.offset = 0;
                return Ok(TailResult {
                    events: Vec::new(),
                    malformed: 0,
                });
            }
            Err(err) => return Err(err).context("opening interactions.jsonl"),
        };

        let len = file
            .metadata()
            .context("reading interactions.jsonl metadata")?
            .len();
        if len < self.offset {
            self.offset = 0;
        }

        file.seek(SeekFrom::Start(self.offset))
            .context("seeking interactions.jsonl")?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .context("reading interactions.jsonl")?;

        // Only consume up to the last newline — a trailing partial line
        // (bd mid-write) is left for the next tail.
        let consumed = match buf.iter().rposition(|&b| b == b'\n') {
            Some(idx) => idx + 1,
            None => 0,
        };

        let mut events = Vec::new();
        let mut malformed = 0;
        for line in buf[..consumed].split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }
            match serde_json::from_slice::<AuditEvent>(line) {
                Ok(event) => events.push(event),
                Err(_) => malformed += 1,
            }
        }

        self.offset += consumed as u64;
        Ok(TailResult { events, malformed })
    }
}

/// Maps one audit-log line onto the unified feed type. `field_change` events
/// carry the field name in `extra.field`; `status`/`priority` get their own
/// `Change` variants so the UI can render them distinctly, everything else
/// falls back to `FieldChanged`.
pub(crate) fn audit_to_feed_event(event: &AuditEvent) -> FeedEvent {
    let change = match event.extra.field.as_deref() {
        Some("status") => Change::StatusChanged {
            old: event
                .extra
                .old_value
                .as_deref()
                .map(Status::parse)
                .unwrap_or_default(),
            new: event
                .extra
                .new_value
                .as_deref()
                .map(Status::parse)
                .unwrap_or_default(),
        },
        Some("priority") => Change::PriorityChanged {
            old: event
                .extra
                .old_value
                .as_deref()
                .and_then(|v| v.parse().ok()),
            new: event
                .extra
                .new_value
                .as_deref()
                .and_then(|v| v.parse().ok()),
        },
        Some(field) => Change::FieldChanged {
            field: field.to_string(),
            old: event.extra.old_value.clone(),
            new: event.extra.new_value.clone(),
        },
        None => Change::FieldChanged {
            field: event.kind.clone(),
            old: None,
            new: None,
        },
    };

    FeedEvent {
        issue_id: event.issue_id.clone().unwrap_or_default(),
        timestamp: event.created_at,
        actor: event.actor.clone().map(Actor),
        change,
        reason: event.extra.reason.clone(),
        derived: false,
    }
}

/// Synthesizes feed events by comparing two full snapshots. Never trusts
/// `updated_at` alone for status/priority — those get their own checks so a
/// simultaneous status+priority change produces two events, not one vague
/// `Touched`. `audit_events` is this refresh's tail read: any issue+field it
/// already reported is suppressed here, since the audit log has the actor
/// and reason and the differ does not.
pub(crate) fn diff_snapshots(
    prev: &HashMap<String, Issue>,
    next: &HashMap<String, Issue>,
    audit_events: &[AuditEvent],
) -> Vec<FeedEvent> {
    let mut audited_fields: HashSet<(&str, &str)> = HashSet::new();
    let mut audited_issues: HashSet<&str> = HashSet::new();
    for event in audit_events {
        if let Some(issue_id) = event.issue_id.as_deref() {
            audited_issues.insert(issue_id);
            if let Some(field) = event.extra.field.as_deref() {
                audited_fields.insert((issue_id, field));
            }
        }
    }

    let mut events = Vec::new();
    for (id, issue) in next {
        match prev.get(id) {
            None => events.push(FeedEvent {
                issue_id: id.clone(),
                timestamp: issue
                    .created_at
                    .or(issue.updated_at)
                    .unwrap_or_else(chrono::Utc::now),
                actor: issue
                    .owner
                    .clone()
                    .or_else(|| issue.created_by.clone())
                    .map(Actor),
                change: Change::Created,
                reason: None,
                derived: true,
            }),
            Some(old) => {
                let mut has_specific_change = false;

                if old.status != issue.status {
                    has_specific_change = true;
                    if !audited_fields.contains(&(id.as_str(), "status")) {
                        events.push(FeedEvent {
                            issue_id: id.clone(),
                            timestamp: issue.updated_at.unwrap_or_else(chrono::Utc::now),
                            actor: None,
                            change: Change::StatusChanged {
                                old: old.status.clone(),
                                new: issue.status.clone(),
                            },
                            reason: None,
                            derived: true,
                        });
                    }
                }

                if old.priority != issue.priority {
                    has_specific_change = true;
                    if !audited_fields.contains(&(id.as_str(), "priority")) {
                        events.push(FeedEvent {
                            issue_id: id.clone(),
                            timestamp: issue.updated_at.unwrap_or_else(chrono::Utc::now),
                            actor: None,
                            change: Change::PriorityChanged {
                                old: old.priority,
                                new: issue.priority,
                            },
                            reason: None,
                            derived: true,
                        });
                    }
                }

                if !has_specific_change
                    && old.updated_at != issue.updated_at
                    && !audited_issues.contains(id.as_str())
                {
                    events.push(FeedEvent {
                        issue_id: id.clone(),
                        timestamp: issue.updated_at.unwrap_or_else(chrono::Utc::now),
                        actor: None,
                        change: Change::Touched,
                        reason: None,
                        derived: true,
                    });
                }
            }
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn issue(id: &str, status: &str, priority: Option<i32>, updated_at: &str) -> Issue {
        let raw = serde_json::json!({
            "id": id,
            "status": status,
            "priority": priority,
            "updated_at": updated_at,
        });
        serde_json::from_value(raw).unwrap()
    }

    #[test]
    fn parses_interactions_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/interactions_voltrol.jsonl").unwrap();
        let mut parsed = 0;
        let mut malformed = 0;
        for line in raw.lines() {
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<AuditEvent>(line) {
                Ok(_) => parsed += 1,
                Err(_) => malformed += 1,
            }
        }
        assert_eq!(malformed, 0);
        assert_eq!(parsed, 27);
    }

    #[test]
    fn audit_status_change_maps_to_status_changed() {
        let raw = r#"{"id":"int-1","kind":"field_change","created_at":"2026-07-01T01:11:27Z","actor":"Rohit Gudi","issue_id":"voltrol-zlw","extra":{"field":"status","new_value":"closed","old_value":"open","reason":"done"}}"#;
        let event: AuditEvent = serde_json::from_str(raw).unwrap();
        let feed = audit_to_feed_event(&event);
        assert_eq!(feed.reason.as_deref(), Some("done"));
        assert!(!feed.derived);
        assert_eq!(
            feed.change,
            Change::StatusChanged {
                old: Status::Open,
                new: Status::Closed
            }
        );
    }

    #[test]
    fn differ_detects_created() {
        let prev = HashMap::new();
        let mut next = HashMap::new();
        next.insert(
            "a-1".to_string(),
            issue("a-1", "open", Some(1), "2026-01-01T00:00:00Z"),
        );

        let events = diff_snapshots(&prev, &next, &[]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change, Change::Created);
        assert!(events[0].derived);
    }

    #[test]
    fn differ_detects_status_and_priority_independently() {
        let mut prev = HashMap::new();
        prev.insert(
            "a-1".to_string(),
            issue("a-1", "open", Some(1), "2026-01-01T00:00:00Z"),
        );
        let mut next = HashMap::new();
        next.insert(
            "a-1".to_string(),
            issue("a-1", "closed", Some(0), "2026-01-02T00:00:00Z"),
        );

        let mut events = diff_snapshots(&prev, &next, &[]);
        events.sort_by_key(|e| format!("{:?}", e.change));
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].change,
            Change::PriorityChanged {
                old: Some(1),
                new: Some(0)
            }
        );
        assert_eq!(
            events[1].change,
            Change::StatusChanged {
                old: Status::Open,
                new: Status::Closed
            }
        );
    }

    #[test]
    fn differ_falls_back_to_touched() {
        let mut prev = HashMap::new();
        prev.insert(
            "a-1".to_string(),
            issue("a-1", "open", Some(1), "2026-01-01T00:00:00Z"),
        );
        let mut next = HashMap::new();
        next.insert(
            "a-1".to_string(),
            issue("a-1", "open", Some(1), "2026-01-02T00:00:00Z"),
        );

        let events = diff_snapshots(&prev, &next, &[]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change, Change::Touched);
    }

    #[test]
    fn differ_suppresses_when_audit_covers_same_field() {
        let mut prev = HashMap::new();
        prev.insert(
            "a-1".to_string(),
            issue("a-1", "open", Some(1), "2026-01-01T00:00:00Z"),
        );
        let mut next = HashMap::new();
        next.insert(
            "a-1".to_string(),
            issue("a-1", "closed", Some(1), "2026-01-02T00:00:00Z"),
        );

        let audit_raw = r#"{"id":"int-1","kind":"field_change","created_at":"2026-01-02T00:00:00Z","actor":"a","issue_id":"a-1","extra":{"field":"status","new_value":"closed","old_value":"open"}}"#;
        let audit_event: AuditEvent = serde_json::from_str(audit_raw).unwrap();

        let events = diff_snapshots(&prev, &next, std::slice::from_ref(&audit_event));
        assert!(
            events.is_empty(),
            "audit-covered status change must not also be derived"
        );
    }

    #[test]
    fn tailer_reads_appended_lines_and_advances_offset() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "not json").unwrap();
        writeln!(
            file,
            r#"{{"id":"int-1","kind":"field_change","created_at":"2026-01-01T00:00:00Z","issue_id":"a-1","extra":{{"field":"status"}}}}"#
        )
        .unwrap();
        file.flush().unwrap();

        let mut tailer = Tailer::new();
        let result = tailer.read_new(file.path()).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.malformed, 1);

        // Nothing new appended: second read is empty.
        let result = tailer.read_new(file.path()).unwrap();
        assert_eq!(result.events.len(), 0);
        assert_eq!(result.malformed, 0);

        writeln!(
            file,
            r#"{{"id":"int-2","kind":"field_change","created_at":"2026-01-01T00:00:01Z","issue_id":"a-1","extra":{{"field":"priority"}}}}"#
        )
        .unwrap();
        file.flush().unwrap();
        let result = tailer.read_new(file.path()).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].id, "int-2");
    }

    #[test]
    fn tailer_resets_offset_when_file_shrinks() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"{{"id":"int-1","kind":"field_change","created_at":"2026-01-01T00:00:00Z","issue_id":"a-1","extra":{{}}}}"#
        )
        .unwrap();
        file.flush().unwrap();

        let mut tailer = Tailer::new();
        tailer.read_new(file.path()).unwrap();
        assert!(tailer.offset > 0);

        // Simulate rotation: truncate to empty, then write a line short enough
        // that the new length is provably less than our prior offset — the
        // rotation guard only fires on `len < offset`, not on content identity.
        file.as_file().set_len(0).unwrap();
        file.rewind().unwrap();
        writeln!(
            file,
            r#"{{"id":"int-2","kind":"field_change","created_at":"2026-01-01T00:00:01Z"}}"#
        )
        .unwrap();
        file.flush().unwrap();

        let result = tailer.read_new(file.path()).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].id, "int-2");
    }

    #[test]
    fn tailer_missing_file_is_not_an_error() {
        let mut tailer = Tailer::new();
        let result = tailer
            .read_new(Path::new("/nonexistent/interactions.jsonl"))
            .unwrap();
        assert_eq!(result.events.len(), 0);
        assert_eq!(result.malformed, 0);
    }
}
