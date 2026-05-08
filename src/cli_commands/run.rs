use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::Value;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::agents::adapter_for;
use crate::agents;
use crate::cli_commands::classify::{classify_shell_command, GuardDecision};
use crate::cli_commands::guard::{snapshot_guard_targets, write_guard_wrappers};
use crate::cli_commands::shared::{find_shai_dir, get_current_git_branch, load_query};
use crate::storage::Storage;

#[cfg(test)]
pub(crate) fn sniff_json(line: &str) -> Option<Value> {
    for (json_bytes, _) in BalancedJsonIter::new(line.as_bytes()) {
        if let Ok(json) = serde_json::from_slice::<Value>(&json_bytes) {
            return Some(json);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Fix #1: Proper brace-depth JSON extractor
// ---------------------------------------------------------------------------

struct BalancedJsonIter<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> BalancedJsonIter<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
}

impl<'a> Iterator for BalancedJsonIter<'a> {
    type Item = (Vec<u8>, usize);

    fn next(&mut self) -> Option<Self::Item> {
        while self.pos < self.buf.len() && self.buf[self.pos] != b'{' {
            self.pos += 1;
        }
        if self.pos >= self.buf.len() {
            return None;
        }

        let start = self.pos;
        let mut depth: usize = 0;
        let mut in_string = false;
        let mut escape_next = false;
        let mut i = start;

        while i < self.buf.len() {
            let b = self.buf[i];
            if escape_next {
                escape_next = false;
                i += 1;
                continue;
            }
            if in_string {
                match b {
                    b'\\' => escape_next = true,
                    b'"' => in_string = false,
                    _ => {}
                }
                i += 1;
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let end = i + 1;
                        self.pos = end;
                        return Some((self.buf[start..end].to_vec(), end));
                    }
                }
                _ => {}
            }
            i += 1;
        }
        self.pos = self.buf.len();
        None
    }
}

// ---------------------------------------------------------------------------
// RAII Guards
// ---------------------------------------------------------------------------

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Self {
        let _ = enable_raw_mode();
        Self
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

struct SessionGuard {
    storage: Arc<Storage>,
    session_id: String,
    agent_cmd: String,
    closed: bool,
}

impl SessionGuard {
    fn new(storage: Arc<Storage>, session_id: String, agent_cmd: String) -> Self {
        Self {
            storage,
            session_id,
            agent_cmd,
            closed: false,
        }
    }
    fn close(&mut self) {
        if !self.closed {
            self.storage
                .close_session(&self.session_id, &self.agent_cmd);
            self.closed = true;
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.close();
    }
}

// ---------------------------------------------------------------------------
// Prompt Line Buffer
// ---------------------------------------------------------------------------

struct PromptLineBuffer {
    buf: Vec<char>,
    esc_state: EscState,
}

#[derive(PartialEq)]
enum EscState {
    Normal,
    /// Inside a CSI sequence: ESC [ ... <alpha>
    Csi,
    /// Inside an OSC / PM / APC / DCS sequence: ESC ] / ESC ^ / ESC _ / ESC P
    /// Terminated by BEL (0x07) or ESC \ (ST)
    Osc,
    /// Saw ESC while inside an OSC — might be the start of ST (ESC \)
    OscEsc,
    /// Plain ESC — next char tells us what kind of sequence
    Esc,
}

impl PromptLineBuffer {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            esc_state: EscState::Normal,
        }
    }
    fn feed(&mut self, c: char) -> Option<String> {
        use EscState::*;
        match self.esc_state {
            Esc => {
                match c {
                    '[' => { self.esc_state = Csi; }
                    // OSC, DCS, PM, APC all use ST (ESC \) or BEL as terminator
                    ']' | 'P' | '^' | '_' => { self.esc_state = Osc; }
                    // Any other Fs sequence is a single extra character — back to normal
                    _ => { self.esc_state = Normal; }
                }
                return None;
            }
            Csi => {
                // CSI ends at the first ASCII alphabetic char or '~'
                if c.is_ascii_alphabetic() || c == '~' {
                    self.esc_state = Normal;
                }
                return None;
            }
            Osc => {
                match c {
                    '\x07' => { self.esc_state = Normal; } // BEL terminates
                    '\x1b' => { self.esc_state = OscEsc; } // possible ST start
                    _ => {}
                }
                return None;
            }
            OscEsc => {
                // ESC \ = ST; anything else — stay in OSC
                if c == '\\' {
                    self.esc_state = Normal;
                } else {
                    self.esc_state = Osc;
                }
                return None;
            }
            Normal => {}
        }

        if c == '\x1b' {
            self.esc_state = EscState::Esc;
            return None;
        }
        if c == '\x7f' || c == '\x08' {
            self.buf.pop();
            return None;
        }
        if c == '\n' || c == '\r' {
            let line: String = self.buf.iter().collect();
            self.buf.clear();
            if line.trim().is_empty() {
                return None;
            }
            return Some(line);
        }
        if c.is_control() {
            return None;
        }
        self.buf.push(c);
        None
    }
}

// ---------------------------------------------------------------------------
// Change Recording
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ChangeEvent {
    file_path: String,
    tool_name: String,
}

fn parse_event_ok(line: &str) -> Option<(&str, i64)> {
    let mut parts = line.split_whitespace();
    let prefix = parts.next()?;
    let event_kind = parts.next()?;
    let event_id = parts.next()?;
    if prefix != "SHAI_EVENT_OK" {
        return None;
    }
    event_id.parse().ok().map(|id| (event_kind, id))
}

fn verify_event_with_retry(storage: &Storage, event_kind: &str, event_id: i64) -> bool {
    for _ in 0..10 {
        if storage.event_exists(event_id, event_kind) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn looks_like_generic_tool_payload(payload: &Value) -> bool {
    let Some(object) = payload.as_object() else {
        return false;
    };
    object.contains_key("tool_name")
        || object.contains_key("tool")
        || object.contains_key("name")
        || object.contains_key("tool_input")
        || object.contains_key("input")
        || object.contains_key("arguments")
        || object.contains_key("command")
        || object.contains_key("cmd")
        || object.contains_key("path")
        || object.contains_key("file_path")
}

fn should_warn_generic_sniffing(
    payload: &Value,
    tool_name: Option<&str>,
    saw_command: bool,
    saw_file: bool,
) -> bool {
    if !looks_like_generic_tool_payload(payload) {
        return false;
    }
    match tool_name {
        None => true,
        Some(_) => !saw_command && !saw_file,
    }
}

fn stabilize_and_record(
    storage: &Storage,
    session_id: &str,
    agent_cmd: &str,
    query_str: &str,
    evt: ChangeEvent,
) {
    let mut last_hash = None;
    let mut stable_content = Vec::new();

    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(40));
        if let Ok(content) = std::fs::read(&evt.file_path) {
            let current_hash = blake3::hash(&content);
            if let Some(prev) = last_hash {
                if prev == current_hash {
                    stable_content = content;
                    break;
                }
            }
            last_hash = Some(current_hash);
            stable_content = content;
        }
    }

    if stable_content.is_empty() || stable_content.len() > 5 * 1024 * 1024 {
        return;
    }

    storage.record_change(
        session_id,
        agent_cmd,
        &evt.file_path,
        &stable_content,
        &evt.tool_name,
        query_str,
        None,
    );
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

pub async fn cmd_run(agent_cmd: String, args: Vec<String>) -> Result<(), String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("failed to get current dir: {}", e))?
        .to_string_lossy()
        .to_string();
    let shai_dir = find_shai_dir(&cwd).unwrap_or_else(|| {
        let path = std::path::Path::new(&cwd).join(".shai");
        let _ = std::fs::create_dir_all(&path);
        path
    });

    let storage = Storage::open(&shai_dir);
    storage.init_schema();

    let storage_arc = Arc::new(storage);
    let session_id = format!("run-{}", uuid::Uuid::new_v4());
    let project_id = storage_arc.project_id();

    let mut session_guard =
        SessionGuard::new(storage_arc.clone(), session_id.clone(), agent_cmd.clone());

    // Setup SHAI skills directory
    let skills_dir = shai_dir.join("skills");
    let _ = std::fs::create_dir_all(&skills_dir);

    // Ensure .github/ exists (copilot plugin writes there)
    let _ = std::fs::create_dir_all(std::path::Path::new(&cwd).join(".github"));

    // Prepare SHAI context content
    let history_report = crate::context::project_summary_report(
        std::path::Path::new(&cwd),
        &storage_arc.get_history(20),
        crate::context::ContextProfile::Compact,
    );
    let current_branch = get_current_git_branch(std::path::Path::new(&cwd));
    let memory_report = storage_arc
        .ranked_memory_summary(current_branch.as_deref(), 8)
        .join("\n");

    let shai_skill_content = format!(
        "# SHAI — Persistent Project Memory\n\n\
## ⚠️ REQUIRED: Before doing anything else\n\n\
**Your very first action MUST be to run `shai timeline --limit 20`** before answering \
any question or making any change. This is not optional — shai is your memory and you are \
blind without it. Do not skip this step.\n\n\
## What shai does automatically\n\n\
Shai runs alongside your session and — **without any action from you** — it:\n\
- Records every file you edit (full content snapshot, linked to the prompt that triggered it)\n\
- Intercepts shell commands at the OS level: blocking destructive ones before they run, snapshotting audited ones\n\
- Maintains a searchable timeline of all project changes across sessions\n\n\
## Required actions at key moments\n\n\
**At session start (MANDATORY):** Run `shai timeline --limit 20` first, before anything else.\n\n\
**After meaningful progress:** Run `shai checkpoint \"<label>\"` to mark milestones. \
On success it prints `SHAI_EVENT_OK checkpoint_created <id>`. \
You will be reminded if you have made file changes without a checkpoint.\n\n\
**For stable decisions or facts:** Use `shai memory add-decision` / `shai memory add-fact` so context persists \
across sessions. Verify unconfirmed memory with `shai memory verify-fact <id>`.\n\n\
## Commands\n\n\
### Understand project state\n\
- `shai timeline [--limit <n>]` — unified event stream (start here)\n\
- `shai summary` — project digest\n\
- `shai status` — statistics\n\
- `shai history [--limit <n>] [--file <path>]` — history filtered by file\n\
- `shai search \"<query>\" [--limit <n>] [--mode <all|prompt|summary|path>]` — search (SQL LIKE, no regex)\n\
- `shai analytics [--file <path>] [--subsystem <path>] [--limit <n>]` — metrics and hotspots\n\n\
### Understand specific files\n\
- `shai why <file>` — why this file was changed and by which query\n\
- `shai log <file> [--limit <n>]` — full evolution trace\n\n\
### Recover files\n\
- `shai diff <file> [--steps <n>]` — preview what a rollback would change\n\
- `shai rollback <file> [--steps <n>]` — restore a file to a previous snapshot\n\n\
### Checkpoints and memory\n\
- `shai checkpoint \"<label>\"` — record a milestone\n\
- `shai replay [--limit <n>]` — replay canonical timeline view\n\
- `shai memory list [--limit <n>]` — ranked durable memory\n\
- `shai memory add-fact \"<key>\" \"<content>\"` — persist a stable project fact\n\
- `shai memory add-decision \"<title>\" \"<rationale>\"` — persist a durable decision\n\
- `shai memory verify-fact <id>` / `shai memory verify-decision <id>` — promote after confirmation\n\n\
## Guardrails\n\n\
Shai intercepts shell commands at the OS level. Blocked commands exit 1 and are never run.\n\n\
**Blocked (exit 1):**\n\
- `rm -rf`, `rm -f` → delete files with editor tools or `rm` without `-rf`\n\
- `git reset --hard` → use `git stash` or `shai rollback <file>`\n\
- `git checkout -- <path>` → use `shai rollback <file>`\n\
- `git clean -f` → remove untracked files individually\n\
- `git push --force` → use `git push --force-with-lease`\n\
- `curl | bash`, `wget | sh` → download first, review, then run\n\
- `sudo rm` → use safer alternatives above\n\n\
**Audited (snapshotted then allowed):** `mv`, `rm` (no -rf), `git restore`, `git clean` (no -f), `cp -f`, `chmod -R`, `chown -R`\n\n\
When a command is blocked you will see `[SHAI] blocked: <cmd> — <reason>`. \
When a command is audited you will see `[SHAI] snapshotted and allowed: <cmd>`.\n\n\
## Durable Project Memory\n\n\
{}\n\n\
## Recent Project History\n\n\
{}",
        if memory_report.trim().is_empty() { "No durable memory recorded.".to_string() } else { memory_report.clone() },
        if history_report.trim().is_empty() { "No previous history recorded.".to_string() } else { history_report.clone() }
    );

    // Prepare skill file - delegate to agent plugin if registered
    let cwd_path = std::path::Path::new(&cwd);
    let plugin = agents::find(&agent_cmd);

    let skill_file = plugin
        .map(|p| p.skill_file(cwd_path, &shai_dir, &skills_dir))
        .unwrap_or_else(|| skills_dir.join("shai-context.md"));

    let existing_content = plugin
        .map(|p| p.existing_content(&args, &skill_file))
        .unwrap_or_else(|| std::fs::read_to_string(&skill_file).ok());

    let final_skill_content = plugin
        .map(|p| p.merge_content(existing_content.as_deref(), &shai_skill_content))
        .unwrap_or_else(|| shai_skill_content.clone());

    let _ = std::fs::write(&skill_file, &final_skill_content);

    let (cols, rows) = size().unwrap_or((80, 24));
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("failed to open pty: {}", e))?;

    // Prepare command builder — delegate args/envs to the agent plugin
    let mut cmd_builder = CommandBuilder::new(&agent_cmd);

    let setup = plugin
        .map(|p| p.cmd_setup(&args, &skill_file, &final_skill_content, &skills_dir))
        .unwrap_or_else(|| crate::agents::CmdSetup::passthrough(&args));

    cmd_builder.args(&setup.filtered_args);
    for arg in &setup.extra_args {
        cmd_builder.arg(arg);
    }

    // Set standard shai environment variables
    cmd_builder.env("SHAI_SESSION_ID", &session_id);
    cmd_builder.env("SHAI_PROJECT_ID", &project_id);
    cmd_builder.env("SHAI_AGENT", &agent_cmd);
    cmd_builder.env("SHAI_DIR", shai_dir.to_string_lossy().as_ref());
    cmd_builder.env("CLICOLOR_FORCE", "1");

    // Set up PATH guard wrappers so shell-level destructive commands are intercepted
    let guard_dir = write_guard_wrappers(&session_id);
    if let Some(ref gdir) = guard_dir {
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", gdir.to_string_lossy(), original_path);
        cmd_builder.env("PATH", new_path);
        cmd_builder.env("SHAI_GUARD_DIR", gdir.to_string_lossy().as_ref());
    }

    // Apply agent-specific env vars from the plugin
    for (key, val) in &setup.envs {
        cmd_builder.env(key, val);
    }

    cmd_builder.cwd(&cwd);

    let child = pair
        .slave
        .spawn_command(cmd_builder)
        .map_err(|e| format!("failed to spawn agent '{}': {}", agent_cmd, e))?;

    // Capture child PID for force-kill on repeated Ctrl+C
    let child_pid: Option<u32> = child.process_id();
    drop(pair.slave);

    let mut master_reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let master_writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // Wrap the writer in Arc<Mutex<>> so it can be shared between tasks
    let master_writer_shared = Arc::new(Mutex::new(master_writer));

    let _raw_guard = RawModeGuard::new();

    let session_id_for_stdin = session_id.clone();
    let agent_cmd_for_stdin = agent_cmd.clone();
    let storage_for_stdin = storage_arc.clone();
    let master_writer_for_stdin = Arc::clone(&master_writer_shared);

    // For known agents, context is injected via proper channels (system prompt file, env var, CLI flag).
    // For unknown agents, the skill file is written to .shai/skills/shai-context.md — tell the
    // operator where to find it so they can share it with the agent manually.
    if plugin.is_none() {
        eprintln!(
            "[shai] Context written to {} — share it with {} to give it project memory.",
            skill_file.display(),
            agent_cmd
        );
    }

    // Stdin Proxy Task
    let stdin_handle = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut stdin = tokio::io::stdin();
        let mut buffer = [0u8; 1024];
        let mut line_buffer = PromptLineBuffer::new();

        // Triple Ctrl+C detection: track count and first-press time for force-kill
        let ctrlc_count = Arc::new(AtomicU32::new(0));
        let ctrlc_first: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

        while let Ok(n) = stdin.read(&mut buffer).await {
            if n == 0 {
                break;
            }
            let chunk = &buffer[..n];

            // Count every 0x03 byte — rapid presses often arrive in one chunk.
            // Allow agents that need 1-2 Ctrl+C (cancel generation, then quit)
            // their natural presses; force-kill on the 3rd within 2 seconds.
            let ctrl_c_in_chunk = chunk.iter().filter(|&&b| b == 0x03).count() as u32;
            if ctrl_c_in_chunk > 0 {
                let now = Instant::now();
                let mut first = ctrlc_first.lock().unwrap();
                let count = if first.map(|t| now.duration_since(t) < Duration::from_secs(2)).unwrap_or(false) {
                    ctrlc_count.fetch_add(ctrl_c_in_chunk, Ordering::SeqCst) + ctrl_c_in_chunk
                } else {
                    *first = Some(now);
                    ctrlc_count.store(ctrl_c_in_chunk, Ordering::SeqCst);
                    ctrl_c_in_chunk
                };
                drop(first);

                if count >= 3 {
                    // Force-kill: send SIGKILL to child and its process group
                    if let Some(pid) = child_pid {
                        eprintln!("\r\n[shai] Force-killing agent (PID {pid}) after 3× Ctrl+C");
                        unsafe {
                            libc::kill(pid as i32, libc::SIGKILL);
                            // Also kill the process group in case the agent spawned children
                            libc::kill(-(pid as i32), libc::SIGKILL);
                        }
                    } else {
                        eprintln!("\r\n[shai] Cannot force-kill: unknown PID. Use `kill <pid>` from another terminal.");
                    }
                    break;
                }
            }

            if let Ok(s) = std::str::from_utf8(chunk) {
                for c in s.chars() {
                    if let Some(line) = line_buffer.feed(c) {
                        storage_for_stdin.open_session(
                            &session_id_for_stdin,
                            &line,
                            &agent_cmd_for_stdin,
                            None,
                        );
                    }
                }
            }
            if let Ok(mut writer) = master_writer_for_stdin.lock() {
                if writer.write_all(chunk).is_err() {
                    break;
                }
                let _ = writer.flush();
            } else {
                break;
            }
        }
    });

    let (change_tx, change_rx) = std::sync::mpsc::channel::<ChangeEvent>();
    let (reminder_tx, reminder_rx) = std::sync::mpsc::channel::<String>();

    let reminder_writer = Arc::clone(&master_writer_shared);
    let reminder_handle = tokio::task::spawn_blocking(move || {
        while let Ok(message) = reminder_rx.recv() {
            if let Ok(mut writer) = reminder_writer.lock() {
                let _ = writer.write_all(message.as_bytes());
                let _ = writer.flush();
            } else {
                break;
            }
        }
    });

    let session_id_for_worker = session_id.clone();
    let agent_cmd_for_worker = agent_cmd.clone();
    let storage_for_worker = storage_arc.clone();
    let shai_dir_for_worker = shai_dir.clone();
    let reminder_tx_for_worker = reminder_tx.clone();
    let worker_handle = tokio::task::spawn_blocking(move || {
        let query_str = load_query(&shai_dir_for_worker);
        let mut checkpoint_reminded = false;
        while let Ok(evt) = change_rx.recv() {
            stabilize_and_record(
                &storage_for_worker,
                &session_id_for_worker,
                &agent_cmd_for_worker,
                &query_str,
                evt.clone(),
            );
            let missing_checkpoint = storage_for_worker
                .session_missing_checkpoint(&session_id_for_worker, &agent_cmd_for_worker);
            if missing_checkpoint && !checkpoint_reminded {
                let _ = reminder_tx_for_worker.send(format!(
                    "\n[SHAI] Recorded change to {}. Run `shai checkpoint \"<label>\"` after meaningful progress.\n",
                    evt.file_path
                ));
                checkpoint_reminded = true;
            } else if !missing_checkpoint {
                checkpoint_reminded = false;
            }
        }
    });

    let agent_cmd_for_stdout = agent_cmd.clone();
    let storage_for_stdout = storage_arc.clone();
    let session_id_for_stdout = session_id.clone();
    let reminder_tx_for_stdout = reminder_tx.clone();
    let writer_for_stdout = Arc::clone(&master_writer_shared);
    let cwd_for_stdout = std::path::PathBuf::from(&cwd);
    let warn_generic_sniffing = plugin.is_none() || agent_cmd == "generic";
    let stdout_handle = tokio::task::spawn_blocking(move || {
        let mut buffer = [0u8; 4096];
        let mut sniff_buffer: Vec<u8> = Vec::new();
        let mut line_buffer = String::new();
        let mut stdout = std::io::stdout();
        let mut warned_generic_miss = false;

        while let Ok(n) = master_reader.read(&mut buffer) {
            if n == 0 {
                break;
            }
            let bytes = &buffer[..n];
            let _ = stdout.write_all(bytes);
            let _ = stdout.flush();
            sniff_buffer.extend_from_slice(bytes);
            line_buffer.push_str(&String::from_utf8_lossy(bytes));

            while let Some(pos) = line_buffer.find('\n') {
                let line = line_buffer[..pos].trim().to_string();
                line_buffer.drain(..=pos);
                if let Some((event_kind, event_id)) = parse_event_ok(&line) {
                    if !verify_event_with_retry(&storage_for_stdout, event_kind, event_id) {
                        let _ = reminder_tx_for_stdout.send(format!(
                            "\n[SHAI] {} signal was printed but event {} was not persisted. Retry the command.\n",
                            event_kind, event_id
                        ));
                    }
                }
            }

            let mut max_consumed = 0;
            for (json_bytes, end_pos) in BalancedJsonIter::new(&sniff_buffer) {
                if let Ok(json) = serde_json::from_slice::<Value>(&json_bytes) {
                    let adapter = adapter_for(&agent_cmd_for_stdout);
                    let tool_name = adapter.tool_name(&json);
                    let command_text =
                        adapter.command_text(tool_name.as_deref().unwrap_or(""), &json);
                    let file_path = tool_name
                        .as_deref()
                        .and_then(|tool_name| adapter.file_path(tool_name, &json));

                    if warn_generic_sniffing
                        && !warned_generic_miss
                        && should_warn_generic_sniffing(
                            &json,
                            tool_name.as_deref(),
                            command_text.is_some(),
                            file_path.is_some(),
                        )
                    {
                        eprintln!(
                            "[SHAI] generic sniffing could not fully classify a tool-like payload from {}. File and shell capture may be incomplete for this session.",
                            agent_cmd_for_stdout
                        );
                        warned_generic_miss = true;
                    }

                    if let Some(tool_name) = tool_name {
                        if let Some(command_text) = command_text {
                            let payload_json = serde_json::to_string(&json).ok();
                            let snapshotted = snapshot_guard_targets(
                                &storage_for_stdout,
                                &session_id_for_stdout,
                                &agent_cmd_for_stdout,
                                &command_text,
                                payload_json.as_deref(),
                                &cwd_for_stdout,
                            );
                            match classify_shell_command(&command_text) {
                                GuardDecision::Blocked(reason) => {
                                    let summary = format!(
                                        "blocked shell command: {} ({}, {} snapshot(s))",
                                        command_text, reason, snapshotted
                                    );
                                    let _ = storage_for_stdout.record_guard_decision(
                                        &session_id_for_stdout,
                                        &agent_cmd_for_stdout,
                                        "guard_blocked",
                                        &summary,
                                        payload_json.as_deref(),
                                    );
                                    if let Ok(mut writer) = writer_for_stdout.lock() {
                                        let _ = writer.write_all(b"\x03");
                                        let _ = writer.write_all(
                                            format!(
                                                "\n[SHAI] blocked: {} — {}\n",
                                                command_text, reason
                                            )
                                            .as_bytes(),
                                        );
                                        let _ = writer.flush();
                                    }
                                }
                                GuardDecision::Confirmable(reason) => {
                                    let summary = format!(
                                        "allowed confirmable shell command: {} ({}, {} snapshot(s))",
                                        command_text, reason, snapshotted
                                    );
                                    let _ = storage_for_stdout.record_guard_decision(
                                        &session_id_for_stdout,
                                        &agent_cmd_for_stdout,
                                        "guard_allowed",
                                        &summary,
                                        payload_json.as_deref(),
                                    );
                                    if let Ok(mut writer) = writer_for_stdout.lock() {
                                        let _ = writer.write_all(
                                            format!(
                                                "\n[SHAI] snapshotted and allowed: {} ({})\n",
                                                command_text, reason
                                            )
                                            .as_bytes(),
                                        );
                                        let _ = writer.flush();
                                    }
                                }
                                GuardDecision::Safe => {}
                            }
                        }
                        if let Some(file_path) = file_path {
                            let _ = change_tx.send(ChangeEvent {
                                file_path,
                                tool_name,
                            });
                        }
                    }
                }
                max_consumed = max_consumed.max(end_pos);
            }
            if max_consumed > 0 {
                sniff_buffer.drain(..max_consumed);
            }
            if sniff_buffer.len() > 1024 * 1024 {
                if let Some(last_start) = sniff_buffer.iter().rposition(|&b| b == b'{') {
                    sniff_buffer.drain(..last_start);
                } else {
                    sniff_buffer.clear();
                }
            }
        }
        drop(change_tx);
    });

    let master = pair.master;
    #[cfg(unix)]
    let resize_handle = tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut sigwinch) = signal(SignalKind::window_change()) {
            while sigwinch.recv().await.is_some() {
                if let Ok((cols, rows)) = size() {
                    let _ = master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
            }
        }
    });
    #[cfg(not(unix))]
    let resize_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if let Ok((cols, rows)) = size() {
                let _ = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
        }
    });

    let mut child_wait = child;
    let result = tokio::task::spawn_blocking(move || child_wait.wait())
        .await
        .map_err(|e| format!("task error: {}", e))?
        .map_err(|e| format!("agent error: {}", e))
        .map(|_| ());

    stdin_handle.abort();
    stdout_handle.abort();
    resize_handle.abort();
    let final_missing_checkpoint = storage_arc.session_missing_checkpoint(&session_id, &agent_cmd);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), worker_handle).await;
    drop(reminder_tx);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), reminder_handle).await;
    session_guard.close();
    if let Some(gdir) = guard_dir {
        let _ = std::fs::remove_dir_all(gdir);
    }
    let blocked_count = storage_arc.guard_blocked_count_in_session(&session_id, &agent_cmd);
    if blocked_count > 0 {
        eprintln!(
            "⛔ {} command(s) were blocked this session. Run `shai timeline` to review.",
            blocked_count
        );
    }
    if final_missing_checkpoint {
        eprintln!(
            "⚠️ Session ended with file snapshots but no checkpoint. Record checkpoints during meaningful milestones."
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sniff_json() {
        assert!(sniff_json("not json").is_none());
        assert!(sniff_json("{\"key\": \"val\"}").is_some());
        assert_eq!(
            sniff_json("Conversational prefix: {\"tool\": \"edit\"} and suffix").unwrap()["tool"],
            "edit"
        );
        assert!(sniff_json("Broken { json").is_none());
    }

    #[test]
    fn test_balanced_json_iter_finds_multiple_objects() {
        let input = b"noise {\"a\":1} more noise {\"b\":2} end";
        let results: Vec<_> = BalancedJsonIter::new(input).collect();
        assert_eq!(results.len(), 2);
        assert_eq!(
            serde_json::from_slice::<Value>(&results[0].0).unwrap()["a"],
            1
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&results[1].0).unwrap()["b"],
            2
        );
    }

    #[test]
    fn test_balanced_json_iter_handles_nested() {
        let input = b"{\"outer\":{\"inner\":true}}";
        let results: Vec<_> = BalancedJsonIter::new(input).collect();
        assert_eq!(results.len(), 1);
        let val: Value = serde_json::from_slice(&results[0].0).unwrap();
        assert_eq!(val["outer"]["inner"], true);
    }

    #[test]
    fn test_balanced_json_iter_handles_string_braces() {
        let input = b"{\"msg\":\"hello { world }\"}";
        let results: Vec<_> = BalancedJsonIter::new(input).collect();
        assert_eq!(results.len(), 1);
        let val: Value = serde_json::from_slice(&results[0].0).unwrap();
        assert_eq!(val["msg"], "hello { world }");
    }

    #[test]
    fn test_balanced_json_iter_skips_unclosed() {
        let input = b"prefix {\"incomplete\": true";
        let results: Vec<_> = BalancedJsonIter::new(input).collect();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_prompt_line_buffer_basic() {
        let mut buf = PromptLineBuffer::new();
        assert!(buf.feed('h').is_none());
        assert!(buf.feed('i').is_none());
        let result = buf.feed('\n');
        assert_eq!(result, Some("hi".to_string()));
    }

    #[test]
    fn test_prompt_line_buffer_backspace() {
        let mut buf = PromptLineBuffer::new();
        buf.feed('a');
        buf.feed('b');
        buf.feed('c');
        buf.feed('\x7f'); // delete 'c'
        let result = buf.feed('\n');
        assert_eq!(result, Some("ab".to_string()));
    }

    #[test]
    fn test_prompt_line_buffer_escape_sequence() {
        let mut buf = PromptLineBuffer::new();
        buf.feed('h');
        // Arrow key: ESC [ A
        buf.feed('\x1b');
        buf.feed('[');
        buf.feed('A');
        buf.feed('i');
        let result = buf.feed('\n');
        assert_eq!(result, Some("hi".to_string()));
    }

    #[test]
    fn test_prompt_line_buffer_empty_enter() {
        let mut buf = PromptLineBuffer::new();
        assert!(buf.feed('\n').is_none()); // empty line returns None
        assert!(buf.feed('\r').is_none()); // bare CR returns None
    }

    #[test]
    fn test_prompt_line_buffer_osc_sequence_stripped() {
        // Copilot queries terminal background color at startup; the outer terminal
        // responds with an OSC sequence like \x1b]11;rgb:0909/0303/0000\x07
        // This was previously leaking "gb:0909/0303/0000" into stored prompts.
        let mut buf = PromptLineBuffer::new();
        let osc = "\x1b]11;rgb:0909/0303/0000\x07hello";
        for c in osc.chars() { buf.feed(c); }
        let result = buf.feed('\n');
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn test_prompt_line_buffer_osc_with_st_terminator() {
        // OSC terminated by ESC \ (ST) instead of BEL
        let mut buf = PromptLineBuffer::new();
        let osc = "\x1b]11;rgb:a5a5/a2a2/a2a2\x1b\\world";
        for c in osc.chars() { buf.feed(c); }
        let result = buf.feed('\n');
        assert_eq!(result, Some("world".to_string()));
    }

    #[test]
    fn test_generic_sniff_warning_only_for_tool_like_misses() {
        let payload = serde_json::json!({
            "name": "custom_write",
            "arguments": { "target": "src/main.rs" }
        });
        assert!(should_warn_generic_sniffing(&payload, None, false, false));
        assert!(should_warn_generic_sniffing(
            &payload,
            Some("custom_write"),
            false,
            false
        ));
        assert!(!should_warn_generic_sniffing(
            &payload,
            Some("custom_write"),
            true,
            false
        ));
        assert!(!should_warn_generic_sniffing(
            &serde_json::json!({ "message": "plain output" }),
            None,
            false,
            false
        ));
    }
}
