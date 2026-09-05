//! End-to-end check against a real `bd` project. `bdw` is a binary crate
//! (no lib target per the Phase 1 spec), so this drives the compiled
//! `bdw --check` binary rather than the internal `BdClient`/`Tailer` types
//! directly — the exact same data-layer code path `--check` exercises.

use std::path::Path;
use std::process::Command;

fn bd_available() -> bool {
    Command::new("bd")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_bd(root: &Path, args: &[&str]) -> serde_json::Value {
    let output = Command::new("bd")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("failed to spawn bd");
    assert!(
        output.status.success(),
        "bd {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null)
}

#[test]
fn check_mode_sees_a_real_bd_project() {
    if !bd_available() {
        println!("bd not found on PATH; skipping integration test");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    // `bd init` only works against the current directory, not `-C`.
    let init_status = Command::new("bd")
        .arg("init")
        .arg("--json")
        .current_dir(root)
        .status()
        .expect("failed to run bd init");
    assert!(init_status.success(), "bd init failed");

    let created = run_bd(
        root,
        &[
            "create",
            "integration probe",
            "--json",
            "--actor",
            "test-actor",
        ],
    );
    let issue_id = created["id"]
        .as_str()
        .expect("create output has id")
        .to_string();

    run_bd(
        root,
        &[
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--actor",
            "test-actor",
            "--json",
        ],
    );
    run_bd(
        root,
        &["close", &issue_id, "--actor", "test-actor", "--json"],
    );

    let output = Command::new(env!("CARGO_BIN_EXE_bdw"))
        .arg(root)
        .arg("--check")
        .output()
        .expect("failed to run bdw --check");

    assert!(
        output.status.success(),
        "bdw --check exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("issues parsed: 1"), "stdout was:\n{stdout}");
    assert!(
        stdout.contains("malformed audit lines: 0"),
        "stdout was:\n{stdout}"
    );
    // status: open -> in_progress -> closed is two field_change events.
    assert!(
        stdout.contains("audit events tailed: 2"),
        "stdout was:\n{stdout}"
    );
}
