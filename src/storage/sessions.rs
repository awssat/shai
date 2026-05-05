use super::content_store::ContentStore;
use super::helpers::{agent_identity, normalize_tool_name, should_track};
use super::{Storage, UNRECORDED_PROMPT};
use crate::verbalizer::{diff_snapshots, verbalize};
use rusqlite::Error as SqlError;

impl Storage {
    pub fn open_session(&self, session_key: &str, prompt: &str, llm: &str, payload_json: Option<&str>) {
        let conn = self.conn();
        let project_id = self.project_id();
        let (agent_family, agent_name) = agent_identity(llm);
        
        let existing: Option<(i64, String)> = match conn.query_row(
            "SELECT id, prompt FROM sessions
             WHERE session_key=?1 AND llm=?2 AND closed_at IS NULL
             ORDER BY started_at DESC LIMIT 1",
            (session_key, llm),
            |r| Ok((r.get(0)?, r.get(1)?)),
        ) {
            Ok(row) => Some(row),
            Err(SqlError::QueryReturnedNoRows) => None,
            Err(_) => None,
        };

        if let Some((id, existing_prompt)) = existing {
            if existing_prompt == UNRECORDED_PROMPT && !prompt.is_empty() {
                let _ = conn.execute(
                    "UPDATE sessions SET prompt=?2, payload_json=?3 WHERE id=?1",
                    (id, prompt, payload_json),
                );
            }
            return;
        }

        let _ = conn.execute(
            "INSERT INTO sessions (session_key, project_id, llm, agent_family, agent_name, prompt, payload_json, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)",
            (session_key, &project_id, llm, &agent_family, &agent_name, prompt, payload_json),
        );
    }

    pub fn close_session(&self, session_key: &str, llm: &str) {
        let conn = self.conn();
        let _ = conn.execute(
            "UPDATE sessions SET closed_at=CURRENT_TIMESTAMP
             WHERE session_key=?1 AND llm=?2 AND closed_at IS NULL",
            (session_key, llm),
        );
    }

    pub fn record_change(&self, session_key: &str, llm: &str, raw_file_path: &str, content: &[u8], tool_name: &str, query_str: &str, payload_json: Option<&str>) {
        if !should_track(raw_file_path) { return; }
        
        let project_root = self.project_root();
        let path = std::path::Path::new(raw_file_path);
        let absolute_path = if path.is_absolute() { path.to_path_buf() } else { project_root.join(path) };
        let mut normalized_path = absolute_path.strip_prefix(&project_root).unwrap_or(&absolute_path).to_string_lossy().to_string();
        if normalized_path.starts_with("./") { normalized_path = normalized_path[2..].to_string(); }
        let file_path = normalized_path.as_str();

        let (agent_family, agent_name) = agent_identity(llm);
        let tool_name_norm = normalize_tool_name(tool_name);
        let hash = blake3::hash(content).to_hex().to_string();
        
        let conn = self.conn();
        let session_id: i64 = conn.query_row(
            "SELECT id FROM sessions WHERE session_key=?1 AND llm=?2 ORDER BY started_at DESC LIMIT 1",
            (session_key, llm),
            |r| r.get(0)
        ).unwrap_or(0);
        
        let source = String::from_utf8_lossy(content).to_string();
        let ast_summary = build_ast_summary(file_path, &source, None, query_str);

        let full_payload = match zstd::encode_all(content, 3) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("shai: zstd encode failed: {}", e);
                return;
            }
        };

        let _ = conn.execute(
            "INSERT INTO changes (
                session_id, project_id, agent_family, agent_name, file_path, blob_hash,
                ast_summary, tool_name, tool_name_raw, tool_name_norm, raw_bytes, stored_bytes,
                payload_json, timestamp
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, CURRENT_TIMESTAMP)",
            rusqlite::params![
                session_id, self.project_id(), agent_family, agent_name, file_path, hash,
                ast_summary, tool_name, tool_name, tool_name_norm, content.len() as i64, full_payload.len() as i64,
                payload_json
            ]
        );
        
        let _ = self.content_store().put(&hash, &full_payload);
    }
}

fn build_ast_summary(file_path: &str, source: &str, _previous_raw: Option<&str>, query_str: &str) -> String {
    if let Some(nodes) = crate::semantic::parse_semantic_ast(file_path, source, query_str) {
        verbalize(&diff_snapshots(vec![], nodes))
    } else {
        "File updated".to_string()
    }
}
