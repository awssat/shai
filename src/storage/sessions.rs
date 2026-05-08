use super::content_store::ContentStore;
use super::helpers::{agent_identity, normalize_file_path, normalize_tool_name, should_track};
use super::Storage;
use crate::verbalizer::{diff_snapshots, verbalize};
use rusqlite::{params, Error as SqlError};

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
            "prompt_submitted",
            None,
            None,
            None,
            Some(prompt),
            prompt,
            payload_json,
            0,
            0,
            "full",
            None,
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
            "session_closed",
            None,
            None,
            None,
            None,
            "",
            None,
            0,
            0,
            "full",
            None,
        );
    }

    pub fn record_change(
        &self,
        session_key: &str,
        llm: &str,
        raw_file_path: &str,
        content: &[u8],
        tool_name: &str,
        query_str: &str,
        payload_json: Option<&str>,
    ) {
        if !should_track(raw_file_path) {
            return;
        }

        let file_path = normalize_file_path(&self.project_root(), raw_file_path);
        let (session_id, project_id, agent_family, agent_name) =
            self.ensure_session(session_key, llm);
        let tool_name_norm = normalize_tool_name(tool_name);
        let hash = blake3::hash(content).to_hex().to_string();

        let source = String::from_utf8_lossy(content).to_string();
        let ast_summary = build_ast_summary(&file_path, &source, query_str);

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
            "file_snapshot",
            None,
            Some(&file_path),
            Some(&hash),
            Some(&tool_name_norm),
            &ast_summary,
            payload_json,
            content.len() as i64,
            full_payload.len() as i64,
            "full",
            None,
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
            "checkpoint_created",
            None,
            None,
            None,
            None,
            label,
            None,
            0,
            0,
            "full",
            None,
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
            event_kind,
            None,
            None,
            None,
            Some("Shell"),
            summary,
            payload_json,
            0,
            0,
            "full",
            None,
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
        if !should_track(raw_file_path) {
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
            "file_snapshot",
            None,
            Some(&file_path),
            Some(&hash),
            Some("GuardSnapshot"),
            summary,
            payload_json,
            content.len() as i64,
            full_payload.len() as i64,
            "full",
            None,
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
        let session_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM sessions
                 WHERE session_key=?1 AND llm=?2
                 ORDER BY started_at DESC
                 LIMIT 1",
                params![session_key, llm],
                |row| row.get(0),
            )
            .ok();
        let Some(session_id) = session_id else {
            return false;
        };

        let snapshot_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM timeline_events
                 WHERE session_id=?1 AND event_kind='file_snapshot'
                   AND COALESCE(tool_name, '') != 'GuardSnapshot'",
                [session_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if snapshot_count == 0 {
            return false;
        }

        let checkpoint_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM timeline_events
                 WHERE session_id=?1 AND event_kind='checkpoint_created'",
                [session_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        checkpoint_count == 0
    }

    pub fn guard_blocked_count_in_session(&self, session_key: &str, llm: &str) -> i64 {
        let conn = self.conn();
        let session_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM sessions
                 WHERE session_key=?1 AND llm=?2
                 ORDER BY started_at DESC
                 LIMIT 1",
                params![session_key, llm],
                |row| row.get(0),
            )
            .ok();
        let Some(session_id) = session_id else {
            return 0;
        };
        conn.query_row(
            "SELECT COUNT(*) FROM timeline_events
             WHERE session_id=?1 AND event_kind='guard_blocked'",
            [session_id],
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
            "session_started",
            None,
            None,
            None,
            None,
            "",
            None,
            0,
            0,
            "full",
            None,
        );

        (session_id, project_id, agent_family, agent_name)
    }

    pub(super) fn append_event(
        &self,
        session_id: i64,
        project_id: &str,
        actor_family: &str,
        actor_name: &str,
        event_kind: &str,
        timestamp: Option<&str>,
        file_path: Option<&str>,
        blob_hash: Option<&str>,
        tool_name: Option<&str>,
        summary: &str,
        payload_json: Option<&str>,
        raw_bytes: i64,
        stored_bytes: i64,
        storage_kind: &str,
        base_event_id: Option<i64>,
    ) -> Result<i64, String> {
        let mut conn = self.conn();
        // BEGIN IMMEDIATE serializes concurrent writers so that `next_seq` and the
        // INSERT are atomic — preventing duplicate seq_in_session values.
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let seq = next_seq(&tx, session_id);
        let sql = if timestamp.is_some() {
            "INSERT INTO timeline_events (
                project_id, session_id, seq_in_session, event_kind, timestamp, actor_family, actor_name,
                file_path, blob_hash, tool_name, summary, payload_json, storage_kind, base_event_id,
                raw_bytes, stored_bytes
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)"
        } else {
            "INSERT INTO timeline_events (
                project_id, session_id, seq_in_session, event_kind, actor_family, actor_name,
                file_path, blob_hash, tool_name, summary, payload_json, storage_kind, base_event_id,
                raw_bytes, stored_bytes
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"
        };

        let result = if let Some(timestamp) = timestamp {
            tx.execute(
                sql,
                params![
                    project_id,
                    session_id,
                    seq,
                    event_kind,
                    timestamp,
                    actor_family,
                    actor_name,
                    file_path,
                    blob_hash,
                    tool_name,
                    summary,
                    payload_json,
                    storage_kind,
                    base_event_id,
                    raw_bytes,
                    stored_bytes
                ],
            )
        } else {
            tx.execute(
                sql,
                params![
                    project_id,
                    session_id,
                    seq,
                    event_kind,
                    actor_family,
                    actor_name,
                    file_path,
                    blob_hash,
                    tool_name,
                    summary,
                    payload_json,
                    storage_kind,
                    base_event_id,
                    raw_bytes,
                    stored_bytes
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
