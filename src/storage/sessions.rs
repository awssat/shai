use super::content_store::ContentStore;
use super::helpers::{agent_identity, normalize_file_path, normalize_tool_name, should_track};
use super::Storage;
use crate::verbalizer::{diff_snapshots, verbalize};
use rusqlite::{params, Error as SqlError};

/// Metadata about why a file change is being recorded.
pub struct ChangeHints<'a> {
    pub tool_name: &'a str,
    pub query_str: &'a str,
    pub payload_json: Option<&'a str>,
}

/// Event-specific fields for appending a timeline event.
pub(super) struct EventData<'a> {
    pub event_kind: &'a str,
    pub timestamp: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub blob_hash: Option<&'a str>,
    pub tool_name: Option<&'a str>,
    pub summary: &'a str,
    pub payload_json: Option<&'a str>,
    pub raw_bytes: i64,
    pub stored_bytes: i64,
    pub storage_kind: &'a str,
    pub base_event_id: Option<i64>,
}

impl Storage {
    pub fn open_session(
        &self,
        session_key: &str,
        prompt: &str,
        llm: &str,
        payload_json: Option<&str>,
    ) {
        let (session_id, project_id, agent_family, agent_name) =
            self.ensure_session(session_key, llm);
        if prompt.trim().is_empty() {
            return;
        }
        let _ = self.append_event(
            session_id,
            &project_id,
            &agent_family,
            &agent_name,
            EventData {
                event_kind: "prompt_submitted",
                timestamp: None,
                file_path: None,
                blob_hash: None,
                tool_name: Some(prompt),
                summary: prompt,
                payload_json,
                raw_bytes: 0,
                stored_bytes: 0,
                storage_kind: "full",
                base_event_id: None,
            },
        );
    }

    pub fn close_session(&self, session_key: &str, llm: &str) {
        let conn = self.conn();
        let session: Option<(i64, String, String, String)> = match conn.query_row(
            "SELECT id, project_id, agent_family, agent_name
             FROM sessions
             WHERE session_key=?1 AND llm=?2 AND closed_at IS NULL
             ORDER BY started_at DESC LIMIT 1",
            (session_key, llm),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ) {
            Ok(row) => Some(row),
            Err(SqlError::QueryReturnedNoRows) => None,
            Err(_) => None,
        };

        let Some((session_id, project_id, agent_family, agent_name)) = session else {
            return;
        };

        let _ = conn.execute(
            "UPDATE sessions SET closed_at=CURRENT_TIMESTAMP WHERE id=?1",
            [session_id],
        );

        let _ = self.append_event(
            session_id,
            &project_id,
            &agent_family,
            &agent_name,
            EventData {
                event_kind: "session_closed",
                timestamp: None,
                file_path: None,
                blob_hash: None,
                tool_name: None,
                summary: "",
                payload_json: None,
                raw_bytes: 0,
                stored_bytes: 0,
                storage_kind: "full",
                base_event_id: None,
            },
        );
    }

    pub fn record_change(
        &self,
        session_key: &str,
        llm: &str,
        raw_file_path: &str,
        content: &[u8],
        hints: ChangeHints<'_>,
    ) {
        if !should_track(raw_file_path, self.gitignore()) {
            return;
        }

        let file_path = normalize_file_path(&self.project_root(), raw_file_path);
        let (session_id, project_id, agent_family, agent_name) =
            self.ensure_session(session_key, llm);
        let tool_name_norm = normalize_tool_name(hints.tool_name);
        let hash = blake3::hash(content).to_hex().to_string();

        let source = String::from_utf8_lossy(content).to_string();
        let ast_summary = build_ast_summary(&file_path, &source, hints.query_str);

        let full_payload = match zstd::encode_all(content, 3) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::error!("shai: zstd encode failed: {}", err);
                return;
            }
        };

        let _ = self.content_store().put(&hash, &full_payload);

        let _ = self.append_event(
            session_id,
            &project_id,
            &agent_family,
            &agent_name,
            EventData {
                event_kind: "file_snapshot",
                timestamp: None,
                file_path: Some(&file_path),
                blob_hash: Some(&hash),
                tool_name: Some(&tool_name_norm),
                summary: &ast_summary,
                payload_json: hints.payload_json,
                raw_bytes: content.len() as i64,
                stored_bytes: full_payload.len() as i64,
                storage_kind: "full",
                base_event_id: None,
            },
        );
    }

    pub fn record_checkpoint(
        &self,
        session_key: &str,
        llm: &str,
        label: &str,
    ) -> Result<i64, String> {
        let (session_id, project_id, agent_family, agent_name) =
            self.ensure_session(session_key, llm);
        self.append_event(
            session_id,
            &project_id,
            &agent_family,
            &agent_name,
            EventData {
                event_kind: "checkpoint_created",
                timestamp: None,
                file_path: None,
                blob_hash: None,
                tool_name: None,
                summary: label,
                payload_json: None,
                raw_bytes: 0,
                stored_bytes: 0,
                storage_kind: "full",
                base_event_id: None,
            },
        )
    }

    pub fn record_guard_decision(
        &self,
        session_key: &str,
        llm: &str,
        event_kind: &str,
        summary: &str,
        payload_json: Option<&str>,
    ) -> Result<i64, String> {
        let (session_id, project_id, agent_family, agent_name) =
            self.ensure_session(session_key, llm);
        self.append_event(
            session_id,
            &project_id,
            &agent_family,
            &agent_name,
            EventData {
                event_kind,
                timestamp: None,
                file_path: None,
                blob_hash: None,
                tool_name: Some("Shell"),
                summary,
                payload_json,
                raw_bytes: 0,
                stored_bytes: 0,
                storage_kind: "full",
                base_event_id: None,
            },
        )
    }

    pub fn record_guard_snapshot(
        &self,
        session_key: &str,
        llm: &str,
        raw_file_path: &str,
        content: &[u8],
        summary: &str,
        payload_json: Option<&str>,
    ) {
        if !should_track(raw_file_path, self.gitignore()) {
            return;
        }

        let file_path = normalize_file_path(&self.project_root(), raw_file_path);
        let (session_id, project_id, agent_family, agent_name) =
            self.ensure_session(session_key, llm);
        let hash = blake3::hash(content).to_hex().to_string();

        let full_payload = match zstd::encode_all(content, 3) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::error!("shai: zstd encode failed for guard snapshot: {}", err);
                return;
            }
        };

        let _ = self.content_store().put(&hash, &full_payload);
        let _ = self.append_event(
            session_id,
            &project_id,
            &agent_family,
            &agent_name,
            EventData {
                event_kind: "file_snapshot",
                timestamp: None,
                file_path: Some(&file_path),
                blob_hash: Some(&hash),
                tool_name: Some("GuardSnapshot"),
                summary,
                payload_json,
                raw_bytes: content.len() as i64,
                stored_bytes: full_payload.len() as i64,
                storage_kind: "full",
                base_event_id: None,
            },
        );
    }

    pub fn event_exists(&self, event_id: i64, event_kind: &str) -> bool {
        let conn = self.conn();
        conn.query_row(
            "SELECT 1 FROM timeline_events WHERE id=?1 AND event_kind=?2 LIMIT 1",
            params![event_id, event_kind],
            |_| Ok(()),
        )
        .is_ok()
    }

    pub fn session_missing_checkpoint(&self, session_key: &str, llm: &str) -> bool {
        let conn = self.conn();
        let result: Option<(i64, i64)> = conn
            .query_row(
                "SELECT
                   SUM(CASE WHEN event_kind='file_snapshot'
                               AND COALESCE(tool_name,'') != 'GuardSnapshot' THEN 1 ELSE 0 END),
                   SUM(CASE WHEN event_kind='checkpoint_created' THEN 1 ELSE 0 END)
                 FROM timeline_events
                 WHERE session_id = (
                   SELECT id FROM sessions
                   WHERE session_key=?1 AND llm=?2
                   ORDER BY started_at DESC LIMIT 1
                 )",
                params![session_key, llm],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        match result {
            Some((snapshots, checkpoints)) => snapshots > 0 && checkpoints == 0,
            None => false,
        }
    }

    pub fn guard_blocked_count_in_session(&self, session_key: &str, llm: &str) -> i64 {
        let conn = self.conn();
        conn.query_row(
            "SELECT COUNT(*) FROM timeline_events
             WHERE session_id = (
               SELECT id FROM sessions
               WHERE session_key=?1 AND llm=?2
               ORDER BY started_at DESC LIMIT 1
             ) AND event_kind='guard_blocked'",
            params![session_key, llm],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    pub(super) fn ensure_session(
        &self,
        session_key: &str,
        llm: &str,
    ) -> (i64, String, String, String) {
        let conn = self.conn();
        let project_id = self.project_id();
        let (agent_family, agent_name) = agent_identity(llm);

        let existing: Option<(i64, String, String, String)> = match conn.query_row(
            "SELECT id, project_id, agent_family, agent_name
             FROM sessions
             WHERE session_key=?1 AND llm=?2 AND closed_at IS NULL
             ORDER BY started_at DESC LIMIT 1",
            (session_key, llm),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ) {
            Ok(row) => Some(row),
            Err(SqlError::QueryReturnedNoRows) => None,
            Err(_) => None,
        };

        if let Some(session) = existing {
            return session;
        }

        let _ = conn.execute(
            "INSERT INTO sessions (session_key, project_id, llm, agent_family, agent_name, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)",
            params![session_key, project_id, llm, agent_family, agent_name],
        );

        let session_id = conn.last_insert_rowid();
        let _ = self.append_event(
            session_id,
            &project_id,
            &agent_family,
            &agent_name,
            EventData {
                event_kind: "session_started",
                timestamp: None,
                file_path: None,
                blob_hash: None,
                tool_name: None,
                summary: "",
                payload_json: None,
                raw_bytes: 0,
                stored_bytes: 0,
                storage_kind: "full",
                base_event_id: None,
            },
        );

        (session_id, project_id, agent_family, agent_name)
    }

    pub(super) fn append_event(
        &self,
        session_id: i64,
        project_id: &str,
        actor_family: &str,
        actor_name: &str,
        ev: EventData<'_>,
    ) -> Result<i64, String> {
        let mut conn = self.conn();
        // BEGIN IMMEDIATE serializes concurrent writers so that `next_seq` and the
        // INSERT are atomic — preventing duplicate seq_in_session values.
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let seq = next_seq(&tx, session_id);
        let result = if let Some(timestamp) = ev.timestamp {
            tx.execute(
                "INSERT INTO timeline_events (
                    project_id, session_id, seq_in_session, event_kind, timestamp, actor_family, actor_name,
                    file_path, blob_hash, tool_name, summary, payload_json, storage_kind, base_event_id,
                    raw_bytes, stored_bytes
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    project_id, session_id, seq, ev.event_kind, timestamp,
                    actor_family, actor_name, ev.file_path, ev.blob_hash, ev.tool_name,
                    ev.summary, ev.payload_json, ev.storage_kind, ev.base_event_id,
                    ev.raw_bytes, ev.stored_bytes
                ],
            )
        } else {
            tx.execute(
                "INSERT INTO timeline_events (
                    project_id, session_id, seq_in_session, event_kind, actor_family, actor_name,
                    file_path, blob_hash, tool_name, summary, payload_json, storage_kind, base_event_id,
                    raw_bytes, stored_bytes
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    project_id, session_id, seq, ev.event_kind,
                    actor_family, actor_name, ev.file_path, ev.blob_hash, ev.tool_name,
                    ev.summary, ev.payload_json, ev.storage_kind, ev.base_event_id,
                    ev.raw_bytes, ev.stored_bytes
                ],
            )
        };

        result.map_err(|err| err.to_string())?;
        let row_id = tx.last_insert_rowid();
        tx.commit().map_err(|e| e.to_string())?;
        Ok(row_id)
    }
}

fn next_seq(conn: &rusqlite::Connection, session_id: i64) -> i64 {
    conn.query_row(
        "SELECT COALESCE(MAX(seq_in_session), 0) + 1 FROM timeline_events WHERE session_id=?1",
        [session_id],
        |row| row.get(0),
    )
    .unwrap_or(1)
}

fn build_ast_summary(file_path: &str, source: &str, query_str: &str) -> String {
    if let Some(nodes) = crate::semantic::parse_semantic_ast(file_path, source, query_str) {
        verbalize(&diff_snapshots(vec![], nodes))
    } else {
        "File updated".to_string()
    }
}
