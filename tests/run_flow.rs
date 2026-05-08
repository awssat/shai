use rusqlite::Connection;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn shai_bin() -> &'static str {
    env!("CARGO_BIN_EXE_shai")
}

fn prepend_path_env() -> String {
    let exe_dir = Path::new(shai_bin())
        .parent()
        .expect("binary should have parent dir");
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut parts = vec![exe_dir.to_path_buf()];
    parts.extend(std::env::split_paths(&existing));
    std::env::join_paths(parts)
        .expect("valid PATH")
        .to_string_lossy()
        .to_string()
}

fn open_db(project_root: &Path) -> Connection {
    Connection::open(project_root.join(".shai").join("timeline.sqlite")).unwrap()
}
#[test]
fn run_persists_checkpoint_from_wrapped_agent() {
    let dir = tempdir().unwrap();
    let output = Command::new(shai_bin())
        .current_dir(dir.path())
        .env("PATH", prepend_path_env())
        .args([
            "run",
            "bash",
            "-c",
            "printf 'hello\\n' > note.txt; printf '%s\\n' '{\"tool_name\":\"write\",\"tool_input\":{\"path\":\"note.txt\"}}'; shai checkpoint \"checkpoint from test\"",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "run failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let conn = open_db(dir.path());
    let checkpoint_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM timeline_events WHERE event_kind='checkpoint_created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let file_snapshot_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM timeline_events WHERE event_kind='file_snapshot'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(checkpoint_count >= 1);
    assert!(file_snapshot_count >= 1);
}

#[test]
fn run_persists_guard_block_and_snapshot_from_wrapped_agent() {
    let dir = tempdir().unwrap();
    let output = Command::new(shai_bin())
        .current_dir(dir.path())
        .env("PATH", prepend_path_env())
        .args([
            "run",
            "bash",
            "-c",
            "printf 'keep\\n' > note.txt; printf '%s\\n' '{\"tool_name\":\"shell\",\"tool_input\":{\"command\":\"rm -rf note.txt\"}}'",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "run failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let conn = open_db(dir.path());
    let guard_block_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM timeline_events WHERE event_kind='guard_blocked'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let guard_snapshot_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM timeline_events WHERE tool_name='GuardSnapshot'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let guard_summary: String = conn
        .query_row(
            "SELECT summary FROM timeline_events WHERE event_kind='guard_blocked' ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(guard_block_count >= 1);
    assert!(guard_snapshot_count >= 1);
    assert!(guard_summary.contains("rm -rf note.txt"));
}

/// Verify that the PATH guard wrappers intercept a destructive shell command
/// that the agent runs directly (no JSON tool-call sniffing involved).
#[test]
fn run_shell_level_rm_is_intercepted_by_guard_wrapper() {
    let dir = tempdir().unwrap();
    let important = dir.path().join("important.txt");
    std::fs::write(&important, b"keep this\n").unwrap();

    // Agent runs `rm -rf important.txt` as a plain shell command (no JSON payload).
    // The guard wrapper should intercept it via PATH and block it.
    let output = Command::new(shai_bin())
        .current_dir(dir.path())
        .env("PATH", prepend_path_env())
        .args(["run", "bash", "-c", "rm -rf important.txt; echo done"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "run failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // The file must still exist — rm was intercepted before executing
    assert!(
        important.exists(),
        "important.txt should still exist because rm was blocked by the guard wrapper"
    );

    // guard_blocked event must be persisted in the timeline
    let conn = open_db(dir.path());
    let guard_block_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM timeline_events WHERE event_kind='guard_blocked'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let guard_summary: String = conn
        .query_row(
            "SELECT summary FROM timeline_events WHERE event_kind='guard_blocked' ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(guard_block_count >= 1);
    assert!(
        guard_summary.contains("rm"),
        "guard summary should mention rm; got: {}",
        guard_summary
    );
}
