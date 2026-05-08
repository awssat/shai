use super::classify::{classify_shell_command, shell_command_snapshot_targets, GuardDecision};
use super::shared::find_shai_dir;
use crate::storage::Storage;

/// Snapshot files that would be destroyed by `command_text` before the action runs.
/// Returns the number of files snapshotted.
pub(crate) fn snapshot_guard_targets(
    storage: &Storage,
    session_id: &str,
    agent_cmd: &str,
    command_text: &str,
    payload_json: Option<&str>,
    cwd: &std::path::Path,
) -> usize {
    let mut snapshotted = 0usize;
    for target in shell_command_snapshot_targets(command_text) {
        let full_path = if std::path::Path::new(&target).is_absolute() {
            std::path::PathBuf::from(&target)
        } else {
            cwd.join(&target)
        };
        if !full_path.is_file() {
            continue;
        }
        if let Ok(content) = std::fs::read(&full_path) {
            let summary = format!(
                "guard snapshot before destructive shell command: {}",
                command_text
            );
            storage.record_guard_snapshot(
                session_id,
                agent_cmd,
                full_path.to_string_lossy().as_ref(),
                &content,
                &summary,
                payload_json,
            );
            snapshotted += 1;
        }
    }
    snapshotted
}

/// Write thin shell wrapper scripts for destructive commands into a temp directory.
/// Returns the path to the guard directory, or `None` if setup failed.
///
/// Each wrapper intercepts the named command before it runs and routes it through
/// `shai guard-exec <name> "$@"`, which classifies, records, and either exec-replaces
/// into the real binary or exits 1 for blocked commands.
pub(crate) fn write_guard_wrappers(session_id: &str) -> Option<std::path::PathBuf> {
    let guard_dir = std::env::temp_dir().join(format!("shai-guards-{}", session_id));
    std::fs::create_dir_all(&guard_dir).ok()?;

    for cmd_name in &["rm", "unlink", "sudo", "git", "mv", "cp", "chmod", "chown"] {
        let script = format!("#!/bin/sh\nexec shai guard-exec {} \"$@\"\n", cmd_name);
        let wrapper_path = guard_dir.join(cmd_name);
        std::fs::write(&wrapper_path, script).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&wrapper_path, std::fs::Permissions::from_mode(0o755)).ok()?;
        }
    }
    Some(guard_dir)
}

/// Internal subcommand called by guard wrapper scripts.
///
/// Reads SHAI_SESSION_ID, SHAI_AGENT, and SHAI_GUARD_DIR from the environment,
/// classifies the intercepted command, records the guard event, and either
/// exec-replaces into the real binary (allowed) or exits 1 (blocked).
pub(crate) fn cmd_guard_exec(name: String, args: Vec<String>) -> ! {
    let session_id = std::env::var("SHAI_SESSION_ID").unwrap_or_default();
    let agent = std::env::var("SHAI_AGENT").unwrap_or_default();
    let guard_dir = std::env::var("SHAI_GUARD_DIR").unwrap_or_default();

    // Reconstruct the full command string for classification and snapshot extraction.
    // Single-quote any argument that contains whitespace so tokenize_shell_words
    // correctly round-trips the original argument boundaries.
    let mut parts = vec![name.clone()];
    parts.extend(args.iter().cloned());
    let command_text = shell_quote_join(&parts);

    let decision = classify_shell_command(&command_text);

    // Open storage — prefer SHAI_DIR env var (set by shai run) so this works even if
    // the agent has cd'd outside the project tree.
    let cwd = std::env::current_dir().unwrap_or_default();
    let storage = if !session_id.is_empty() {
        let shai_dir = std::env::var("SHAI_DIR")
            .map(std::path::PathBuf::from)
            .ok()
            .filter(|p| p.is_dir())
            .or_else(|| find_shai_dir(&cwd.to_string_lossy()));
        shai_dir.map(|dir| {
            let s = Storage::open(&dir);
            s.init_schema();
            s
        })
    } else {
        None
    };

    match &decision {
        GuardDecision::Blocked(reason) => {
            if let Some(ref db) = storage {
                let snapshotted =
                    snapshot_guard_targets(db, &session_id, &agent, &command_text, None, &cwd);
                let summary = format!(
                    "blocked shell command: {} ({}, {} snapshot(s))",
                    command_text, reason, snapshotted
                );
                let _ =
                    db.record_guard_decision(&session_id, &agent, "guard_blocked", &summary, None);
            }
            eprintln!("[SHAI] blocked: {} — {}", command_text, reason);
            std::process::exit(1);
        }
        GuardDecision::Confirmable(reason) => {
            let snapshotted = if let Some(ref db) = storage {
                snapshot_guard_targets(db, &session_id, &agent, &command_text, None, &cwd)
            } else {
                0
            };
            eprintln!(
                "[SHAI] snapshotted and allowed: {} ({})",
                command_text, reason
            );
            match spawn_and_capture(&name, &args, &guard_dir) {
                Ok(result) => {
                    if let Some(ref db) = storage {
                        let payload = build_exec_payload(result.exit_code, &result.stdout, &result.stderr);
                        let summary = format!(
                            "allowed confirmable shell command: {} ({}, {} snapshot(s), exit={})",
                            command_text, reason, snapshotted, result.exit_code
                        );
                        let _ = db.record_guard_decision(
                            &session_id,
                            &agent,
                            "guard_allowed",
                            &summary,
                            Some(&payload),
                        );
                    }
                    std::process::exit(result.exit_code);
                }
                Err(err) => {
                    eprintln!("[SHAI] guard: spawn failed: {}", err);
                    std::process::exit(1);
                }
            }
        }
        GuardDecision::Safe => {
            exec_real(&name, &args, &guard_dir);
        }
    }
}

/// Rebuild a command string from already-split tokens, quoting any token that
/// contains whitespace so `tokenize_shell_words` round-trips correctly.
fn shell_quote_join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|p| {
            if p.chars().any(|c| c.is_whitespace()) {
                // Single-quote, escaping any embedded single quotes
                format!("'{}'", p.replace('\'', "'\\''"))
            } else {
                p.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Search PATH for the real binary, skipping the guard wrapper directory.
fn find_real_binary(name: &str, guard_dir: &str) -> Option<std::path::PathBuf> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        if !guard_dir.is_empty() && dir.to_string_lossy() == guard_dir {
            continue;
        }
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn exec_real(name: &str, args: &[String], guard_dir: &str) -> ! {
    match find_real_binary(name, guard_dir) {
        Some(bin) => exec_binary(bin, args),
        None => {
            eprintln!("[SHAI] guard: cannot find real '{}' binary in PATH", name);
            std::process::exit(127);
        }
    }
}

#[cfg(unix)]
fn exec_binary(bin: std::path::PathBuf, args: &[String]) -> ! {
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(&bin).args(args).exec();
    eprintln!("[SHAI] guard: exec failed: {}", err);
    std::process::exit(1);
}

#[cfg(not(unix))]
fn exec_binary(bin: std::path::PathBuf, args: &[String]) -> ! {
    let code = std::process::Command::new(&bin)
        .args(args)
        .status()
        .map(|s| s.code().unwrap_or(0))
        .unwrap_or(1);
    std::process::exit(code);
}

struct CaptureResult {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Spawn the real binary, tee its stdout/stderr to our own, wait for it, and
/// return the exit code plus the captured output (for recording in the event
/// payload). Unlike `exec_real`, this preserves the process boundary so we
/// can observe what the command produced.
fn spawn_and_capture(name: &str, args: &[String], guard_dir: &str) -> Result<CaptureResult, String> {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::thread;

    let bin = find_real_binary(name, guard_dir)
        .ok_or_else(|| format!("cannot find real '{}' binary in PATH", name))?;

    let mut child = std::process::Command::new(&bin)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn failed: {}", e))?;

    let mut child_stdout = child.stdout.take().expect("piped stdout");
    let mut child_stderr = child.stderr.take().expect("piped stderr");

    let stdout_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
        loop {
            match child_stdout.read(&mut tmp) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = std::io::stdout().write_all(&tmp[..n]);
                    buf.extend_from_slice(&tmp[..n]);
                }
            }
        }
        buf
    });

    let stderr_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
        loop {
            match child_stderr.read(&mut tmp) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = std::io::stderr().write_all(&tmp[..n]);
                    buf.extend_from_slice(&tmp[..n]);
                }
            }
        }
        buf
    });

    let status = child.wait().map_err(|e| format!("wait failed: {}", e))?;
    let exit_code = status.code().unwrap_or(-1);
    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();

    Ok(CaptureResult { exit_code, stdout, stderr })
}

/// Serialise exit code + truncated output into a JSON string suitable for
/// the `payload_json` column of `timeline_events`.
fn build_exec_payload(exit_code: i32, stdout: &[u8], stderr: &[u8]) -> String {
    const MAX: usize = 2048;
    let stdout_s = String::from_utf8_lossy(stdout);
    let stderr_s = String::from_utf8_lossy(stderr);
    let stdout_val = if stdout.len() > MAX { &stdout_s[..MAX] } else { &stdout_s };
    let stderr_val = if stderr.len() > MAX { &stderr_s[..MAX] } else { &stderr_s };
    serde_json::json!({
        "exit_code": exit_code,
        "stdout": stdout_val,
        "stderr": stderr_val,
    })
    .to_string()
}
