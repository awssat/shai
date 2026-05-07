use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::Value;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use crate::adapters::adapter_for;
use crate::cli_commands::shared::{find_shai_dir, load_query};
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
            if escape_next { escape_next = false; i += 1; continue; }
            if in_string {
                match b {
                    b'\\' => escape_next = true,
                    b'"' => in_string = false,
                    _ => {}
                }
                i += 1; continue;
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
        Self { storage, session_id, agent_cmd, closed: false }
    }
    fn close(&mut self) {
        if !self.closed {
            self.storage.close_session(&self.session_id, &self.agent_cmd);
            self.closed = true;
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) { self.close(); }
}

// ---------------------------------------------------------------------------
// Prompt Line Buffer
// ---------------------------------------------------------------------------

struct PromptLineBuffer {
    buf: Vec<char>,
    in_escape: bool,
}

impl PromptLineBuffer {
    fn new() -> Self {
        Self { buf: Vec::new(), in_escape: false }
    }
    fn feed(&mut self, c: char) -> Option<String> {
        if self.in_escape {
            if c.is_ascii_alphabetic() || c == '~' { self.in_escape = false; }
            return None;
        }
        if c == '\x1b' { self.in_escape = true; return None; }
        if c == '\x7f' || c == '\x08' { self.buf.pop(); return None; }
        if c == '\n' || c == '\r' {
            let line: String = self.buf.iter().collect();
            self.buf.clear();
            if line.trim().is_empty() { return None; }
            return Some(line);
        }
        if c.is_control() { return None; }
        self.buf.push(c);
        None
    }
}

// ---------------------------------------------------------------------------
// Change Recording
// ---------------------------------------------------------------------------

struct ChangeEvent {
    file_path: String,
    tool_name: String,
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
                if prev == current_hash { stable_content = content; break; }
            }
            last_hash = Some(current_hash);
            stable_content = content;
        }
    }

    if stable_content.is_empty() || stable_content.len() > 5 * 1024 * 1024 { return; }

    storage.record_change(
        session_id, agent_cmd, &evt.file_path, &stable_content, &evt.tool_name, query_str, None,
    );
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

pub async fn cmd_run(agent_cmd: String, args: Vec<String>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("failed to get current dir: {}", e))?.to_string_lossy().to_string();
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

    let mut session_guard = SessionGuard::new(storage_arc.clone(), session_id.clone(), agent_cmd.clone());

    // Setup SHAI skills directory
    let skills_dir = shai_dir.join("skills");
    let _ = std::fs::create_dir_all(&skills_dir);

    // For copilot, use .github/copilot-instructions.md instead of skills directory
    let github_dir = std::path::Path::new(&cwd).join(".github");
    let _ = std::fs::create_dir_all(&github_dir);
    let copilot_instructions_file = github_dir.join("copilot-instructions.md");

    // Prepare SHAI context content
    let history_report = crate::context::project_summary_report(
        std::path::Path::new(&cwd), &storage_arc.get_history(20), crate::context::ContextProfile::Compact,
    );

    let shai_skill_content = format!(
        "# SHAI Project Memory\n\nYou have a project memory tool called `shai` available in your PATH. Use it to maintain continuity and perform high-fidelity actions:\n\n- `shai summary` - Get project digest\n- `shai history [--limit <n>] [--file <path>]` - View timeline (--file uses SQL LIKE %path%)\n- `shai search \"<query>\" [--limit <n>] [--mode <all|prompt|summary|path>]` - Search (uses SQL LIKE %query%, NO regex)\n- `shai why <file>` - Get context about a file\n- `shai log <file> [--limit <n>]` - Trace file evolution\n- `shai diff <file> [--steps <n>]` - Preview rollback\n- `shai rollback <file> [--steps <n>]` - Restore file to previous version\n- `shai status` - Show project statistics\n- `shai analytics [--file <path>] [--subsystem <path>] [--limit <n>]` - Show metrics and hotspots\n\n## Recent Project History\n\n{}\n\nAlways consider using `shai` tools to understand project context before making changes.",
        if history_report.trim().is_empty() { "No previous history recorded.".to_string() } else { history_report.clone() }
    );

    // Prepare skill file - handle merging with existing user content
    // We'll write to a temp file to avoid conflicts
    let skill_file = if agent_cmd == "copilot" {
        // For copilot, use .github/copilot-instructions.md
        copilot_instructions_file.clone()
    } else {
        // For other agents, use .shai/skills/shai-context.md
        skills_dir.join("shai-context.md")
    };

    let final_skill_content = if agent_cmd == "copilot" {
        // For copilot, check if .github/copilot-instructions.md exists
        if skill_file.exists() {
            if let Ok(existing_content) = std::fs::read_to_string(&skill_file) {
                format!("{}\n\n{}", existing_content, shai_skill_content)
            } else {
                shai_skill_content.clone()
            }
        } else {
            shai_skill_content.clone()
        }
    } else if agent_cmd == "gemini" {
        // For gemini, check if user has GEMINI_SYSTEM_MD set
        if let Ok(existing_path) = std::env::var("GEMINI_SYSTEM_MD") {
            // Merge existing content with SHAI context
            if let Ok(existing_content) = std::fs::read_to_string(&existing_path) {
                format!("{}\n\n{}", existing_content, shai_skill_content)
            } else {
                shai_skill_content
            }
        } else {
            shai_skill_content.clone()
        }
    } else if agent_cmd == "claude" {
        // For claude, check if user has --append-system-prompt-file in args
        let existing_file_idx = args.iter().position(|a| a == "--append-system-prompt-file");
        if let Some(idx) = existing_file_idx {
            if idx + 1 < args.len() {
                let existing_path = &args[idx + 1];
                if let Ok(existing_content) = std::fs::read_to_string(existing_path) {
                    // Merge existing content with SHAI context
                    format!("{}\n\n{}", existing_content, shai_skill_content)
                } else {
                    shai_skill_content.clone()
                }
            } else {
                shai_skill_content.clone()
            }
        } else {
            shai_skill_content.clone()
        }
    } else if agent_cmd == "opencode" {
        // For opencode, check if user has --system in args
        // Usage: opencode --system "instruction" "prompt"
        let system_idx = args.iter().position(|a| a == "--system");
        if let Some(idx) = system_idx {
            if idx + 1 < args.len() {
                let existing_instruction = &args[idx + 1];
                // Merge existing system instruction with SHAI context
                format!("{}\n\n{}", existing_instruction, shai_skill_content)
            } else {
                shai_skill_content.clone()
            }
        } else {
            shai_skill_content.clone()
        }
    } else if agent_cmd == "junie" {
        // For junie, just use SHAI content (we'll add --skill-location flag in command builder)
        shai_skill_content.clone()
    } else {
        // For other agents, just use SHAI content
        shai_skill_content.clone()
    };

    let _ = std::fs::write(&skill_file, &final_skill_content);

    let (cols, rows) = size().unwrap_or((80, 24));
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|e| format!("failed to open pty: {}", e))?;

    // Prepare command builder
    let mut cmd_builder = CommandBuilder::new(&agent_cmd);

    // For claude and opencode, filter out user's flags since we'll replace them with our merged content
    if agent_cmd == "claude" {
        let append_flag_idx = args.iter().position(|a| a == "--append-system-prompt-file");
        if let Some(idx) = append_flag_idx {
            // Filter out the flag and its value
            let filtered_args: Vec<&String> = args.iter()
                .enumerate()
                .filter(|(i, _)| *i != idx && *i != idx + 1)
                .map(|(_, arg)| arg)
                .collect();
            cmd_builder.args(filtered_args);
        } else {
            cmd_builder.args(&args);
        }
    } else if agent_cmd == "opencode" {
        let system_flag_idx = args.iter().position(|a| a == "--system");
        if let Some(idx) = system_flag_idx {
            // Filter out the flag and its value, we'll add it back with merged content
            let filtered_args: Vec<&String> = args.iter()
                .enumerate()
                .filter(|(i, _)| *i != idx && *i != idx + 1)
                .map(|(_, arg)| arg)
                .collect();
            cmd_builder.args(filtered_args);
            // Add --system flag with merged content
            cmd_builder.arg("--system");
            cmd_builder.arg(&final_skill_content);
        } else {
            cmd_builder.args(&args);
        }
    } else if agent_cmd == "junie" {
        // For junie, add --skill-location flag pointing to SHAI skills directory
        cmd_builder.args(&args);
        cmd_builder.arg("--skill-location");
        cmd_builder.arg(&skills_dir);
    } else {
        cmd_builder.args(&args);
    }

    // Set environment variables
    cmd_builder.env("SHAI_SESSION_ID", &session_id);
    cmd_builder.env("SHAI_PROJECT_ID", &project_id);
    cmd_builder.env("SHAI_AGENT", &agent_cmd);
    cmd_builder.env("CLICOLOR_FORCE", "1");

    if agent_cmd == "gemini" {
        // For gemini, use GEMINI_SYSTEM_MD environment variable
        // Content already merged above, just set the path
        let skill_path = skill_file.to_string_lossy().to_string();
        cmd_builder.env("GEMINI_SYSTEM_MD", &skill_path);
    }
    // Note: For copilot, we write to .github/copilot-instructions.md directly, no env var needed

    cmd_builder.cwd(&cwd);

    if agent_cmd == "claude" {
        // For claude, use --append-system-prompt-file to add SHAI skills
        // Content already merged above, just point to our file
        cmd_builder.arg("--append-system-prompt-file");
        cmd_builder.arg(&skill_file);
    }

    let child = pair.slave.spawn_command(cmd_builder).map_err(|e| format!("failed to spawn agent '{}': {}", agent_cmd, e))?;
    drop(pair.slave);

    let mut master_reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let master_writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // Wrap the writer in Arc<Mutex<>> so it can be shared between tasks
    let master_writer_shared = Arc::new(Mutex::new(master_writer));

    let _raw_guard = RawModeGuard::new();

    let session_id_for_stdin = session_id.clone();
    let agent_cmd_for_stdin = agent_cmd.clone();
    let storage_for_stdin = storage_arc.clone();

    // For other agents (not claude/copilot/gemini/opencode/junie), use stdin injection with \r\n terminator
    if agent_cmd != "copilot" && agent_cmd != "claude" && agent_cmd != "gemini" && agent_cmd != "opencode" && agent_cmd != "junie" {
        let injection_header = format!(
            "[SHAI SYSTEM INSTRUCTION] You have a project memory tool called `shai` available in your PATH. Use it to maintain continuity and perform high-fidelity actions: `shai summary` (project digest), `shai history [--limit <n>] [--file <path>]` (timeline, --file uses SQL LIKE %path%), `shai search \"<query>\" [--limit <n>] [--mode <all|prompt|summary|path>]` (uses SQL LIKE %query%, NO regex support), `shai why <file>` (context), `shai log <file> [--limit <n>]` (evolution), `shai diff <file> [--steps <n>]` (rollback preview), `shai rollback <file> [--steps <n>]` (restore file), `shai status` (stats), `shai analytics [--file <path>] [--subsystem <path>] [--limit <n>]` (metrics/hotspots). [RECENT PROJECT HISTORY] {} [END OF SHAI CONTEXT]\r\n",
            if history_report.trim().is_empty() { "No previous history recorded.".to_string() } else { history_report.replace('\n', " | ") }
        );
        let master_writer_for_injection = Arc::clone(&master_writer_shared);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if let Ok(mut writer) = master_writer_for_injection.lock() {
                let _ = writer.write_all(injection_header.as_bytes());
                let _ = writer.flush();
            }
        });
    }

    // Stdin Proxy Task
    let stdin_handle = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut stdin = tokio::io::stdin();
        let mut buffer = [0u8; 1024];
        let mut line_buffer = PromptLineBuffer::new();

        while let Ok(n) = stdin.read(&mut buffer).await {
            if n == 0 { break; }
            let chunk = &buffer[..n];
            if let Ok(s) = std::str::from_utf8(chunk) {
                for c in s.chars() {
                    if let Some(line) = line_buffer.feed(c) {
                        storage_for_stdin.open_session(&session_id_for_stdin, &line, &agent_cmd_for_stdin, None);
                    }
                }
            }
            if let Ok(mut writer) = master_writer_shared.lock() {
                if writer.write_all(chunk).is_err() { break; }
                let _ = writer.flush();
            } else {
                break;
            }
        }
    });

    let (change_tx, change_rx) = std::sync::mpsc::channel::<ChangeEvent>();

    let session_id_for_worker = session_id.clone();
    let agent_cmd_for_worker = agent_cmd.clone();
    let storage_for_worker = storage_arc.clone();
    let shai_dir_for_worker = shai_dir.clone();
    let worker_handle = tokio::task::spawn_blocking(move || {
        let query_str = load_query(&shai_dir_for_worker);
        while let Ok(evt) = change_rx.recv() {
            stabilize_and_record(&storage_for_worker, &session_id_for_worker, &agent_cmd_for_worker, &query_str, evt);
        }
    });

    let agent_cmd_for_stdout = agent_cmd.clone();
    let stdout_handle = tokio::task::spawn_blocking(move || {
        let mut buffer = [0u8; 4096];
        let mut sniff_buffer: Vec<u8> = Vec::new();
        let mut stdout = std::io::stdout();

        while let Ok(n) = master_reader.read(&mut buffer) {
            if n == 0 { break; }
            let bytes = &buffer[..n];
            let _ = stdout.write_all(bytes);
            let _ = stdout.flush();
            sniff_buffer.extend_from_slice(bytes);

            let mut max_consumed = 0;
            for (json_bytes, end_pos) in BalancedJsonIter::new(&sniff_buffer) {
                if let Ok(json) = serde_json::from_slice::<Value>(&json_bytes) {
                    let adapter = adapter_for(&agent_cmd_for_stdout);
                    if let Some(tool_name) = adapter.tool_name(&json) {
                        if let Some(file_path) = adapter.file_path(&tool_name, &json) {
                            let _ = change_tx.send(ChangeEvent { file_path, tool_name });
                        }
                    }
                }
                max_consumed = max_consumed.max(end_pos);
            }
            if max_consumed > 0 { sniff_buffer.drain(..max_consumed); }
            if sniff_buffer.len() > 1024 * 1024 {
                if let Some(last_start) = sniff_buffer.iter().rposition(|&b| b == b'{') { sniff_buffer.drain(..last_start); }
                else { sniff_buffer.clear(); }
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
                    let _ = master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                }
            }
        }
    });
    #[cfg(not(unix))]
    let resize_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if let Ok((cols, rows)) = size() {
                let _ = master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
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
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), worker_handle).await;
    session_guard.close();
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
}
