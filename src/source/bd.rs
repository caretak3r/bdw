//! Shells out to the `bd` CLI. Never touch `.beads/embeddeddolt/` directly —
//! that is the whole point of going through `bd --readonly --json`.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;

use crate::model::{Counts, Issue};

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub(crate) enum BdError {
    #[error("failed to run bd: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("bd exited with status {status}: {stderr}")]
    NonZero { status: i32, stderr: String },
    #[error("failed to parse bd output: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Wrapper for the top-level object `bd status --json` returns; only the
/// `summary` field is interesting to us.
#[derive(Debug, Deserialize)]
struct StatusResponse {
    summary: Counts,
}

pub(crate) struct BdClient {
    root: PathBuf,
}

impl BdClient {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn interactions_path(&self) -> PathBuf {
        self.root.join(".beads").join("interactions.jsonl")
    }

    async fn run(&self, args: &[&str]) -> Result<String, BdError> {
        let mut command = tokio::process::Command::new("bd");
        command
            .args(args)
            .arg("--readonly")
            .arg("--json")
            .arg("-C")
            .arg(&self.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command.spawn()?;

        let output = match tokio::time::timeout(TIMEOUT, child.wait_with_output()).await {
            Ok(result) => result?,
            Err(_) => {
                return Err(BdError::NonZero {
                    status: -1,
                    stderr: format!("bd {} timed out after {TIMEOUT:?}", args.join(" ")),
                })
            }
        };

        if !output.status.success() {
            return Err(BdError::NonZero {
                status: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub(crate) async fn snapshot(&self) -> Result<Vec<Issue>, BdError> {
        let raw = self.run(&["list", "--all", "--flat"]).await?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub(crate) async fn counts(&self) -> Result<Counts, BdError> {
        let raw = self.run(&["status"]).await?;
        let response: StatusResponse = serde_json::from_str(&raw)?;
        Ok(response.summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/list_voltrol.json").unwrap();
        let issues: Vec<Issue> = serde_json::from_str(&raw).unwrap();
        assert!(!issues.is_empty());

        let epic = issues.iter().find(|i| i.id == "voltrol-qv8").unwrap();
        assert_eq!(epic.issue_type, "epic");
        assert!(epic.parent.is_none());

        let child = issues.iter().find(|i| i.id == "voltrol-qv8.7").unwrap();
        assert_eq!(child.parent.as_deref(), Some("voltrol-qv8"));
        assert_eq!(child.dependencies.len(), 2);
        let parent_link = child
            .dependencies
            .iter()
            .find(|d| d.dep_type == "parent-child")
            .unwrap();
        assert_eq!(parent_link.depends_on_id, "voltrol-qv8");
    }

    #[test]
    fn parses_status_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/status_voltrol.json").unwrap();
        let response: StatusResponse = serde_json::from_str(&raw).unwrap();
        assert_eq!(response.summary.total_issues, 84);
        assert_eq!(response.summary.blocked_issues, 15);
        assert_eq!(response.summary.ready_issues, 15);
    }

    #[test]
    fn unknown_status_string_does_not_error() {
        let issue: Issue =
            serde_json::from_str(r#"{"id":"x-1","status":"someday_maybe"}"#).unwrap();
        assert_eq!(
            issue.status,
            crate::model::Status::Other("someday_maybe".into())
        );
    }
}
