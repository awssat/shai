use super::{
    ChangeRecord, FileChangeRecord, ProjectTimelineRecord, SessionRecord, Storage,
    TimelineEventRecord,
};
use rusqlite::{params, params_from_iter, types::Value, Connection};

impl Storage {
    pub fn get_file_at_step(
        &self,
        file_path: &str,
        steps: u32,
    ) -> Option<(String, String, String)> {
        let conn = self.conn();
        conn.query_row(
            "SELECT blob_hash, timestamp, summary
             FROM timeline_events
             WHERE event_kind='file_snapshot' AND file_path = ?1
             ORDER BY timestamp DESC, id DESC
             LIMIT 1 OFFSET ?2",
            params![file_path, steps.saturating_sub(1)],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok()
    }

    pub fn get_file_history(&self, file_path: &str, limit: u32) -> Vec<FileChangeRecord> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT e.timestamp, COALESCE(e.tool_name, 'Write'), e.summary, e.blob_hash, s.llm,
                        (
                            SELECT prompt.summary
                            FROM timeline_events prompt
                            WHERE prompt.session_id = e.session_id AND prompt.event_kind='prompt_submitted'
                            ORDER BY prompt.seq_in_session ASC
                            LIMIT 1
                        )
                 FROM timeline_events e
                 JOIN sessions s ON s.id = e.session_id
                 WHERE e.event_kind='file_snapshot' AND e.file_path = ?1
                 ORDER BY e.timestamp DESC, e.id DESC
                 LIMIT ?2",
            )
            .unwrap();

        stmt.query_map(params![file_path, limit], |row| {
            Ok(FileChangeRecord {
                timestamp: row.get(0)?,
                tool_name: row.get(1)?,
                ast_summary: row.get(2)?,
                blob_hash: row.get(3)?,
                prompt: row.get(5)?,
                llm: row.get(4)?,
            })
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
    }

    pub fn get_history(&self, limit: u32) -> Vec<SessionRecord> {
        self.get_history_filtered(limit, &[], None)
    }

    pub fn get_history_filtered(
        &self,
        limit: u32,
        file_filter: &[String],
        since: Option<&str>,
    ) -> Vec<SessionRecord> {
        let conn = self.conn();
        let mut sql = String::from(
            "SELECT id, session_key, llm, started_at
             FROM sessions
             WHERE project_id=?1",
        );
        let mut params = vec![Value::Text(self.project_id())];

        if let Some(since) = since {
            sql.push_str(" AND started_at >= ?");
            params.push(Value::Text(since.to_string()));
        }

        if !file_filter.is_empty() {
            sql.push_str(" AND id IN (SELECT session_id FROM timeline_events WHERE event_kind='file_snapshot' AND (");
            for (index, filter) in file_filter.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("file_path LIKE ?");
                params.push(Value::Text(format!("%{}%", filter)));
            }
            sql.push_str("))");
        }

        sql.push_str(" ORDER BY started_at DESC LIMIT ?");
        params.push(Value::Integer(limit as i64));

        let mut stmt = conn.prepare(&sql).unwrap();
        stmt.query_map(params_from_iter(params), |row| {
            let id: i64 = row.get(0)?;
            let prompt: Option<String> = conn
                .query_row(
                    "SELECT summary
                     FROM timeline_events
                     WHERE session_id=?1 AND event_kind='prompt_submitted'
                     ORDER BY seq_in_session ASC
                     LIMIT 1",
                    [id],
                    |prompt_row| prompt_row.get(0),
                )
                .ok();

            Ok(SessionRecord {
                id,
                session_key: row.get(1)?,
                llm: row.get(2)?,
                prompt: prompt.unwrap_or_else(|| "(no prompt)".to_string()),
                started_at: row.get(3)?,
                changes: self.load_changes_for_session(&conn, id, file_filter),
            })
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
    }

    pub fn load_changes_for_session(
        &self,
        conn: &Connection,
        session_id: i64,
        file_filter: &[String],
    ) -> Vec<ChangeRecord> {
        let mut sql = String::from(
            "SELECT file_path, blob_hash, summary, COALESCE(tool_name, 'Write'), timestamp, actor_family, actor_name
             FROM timeline_events
             WHERE session_id=?1 AND event_kind='file_snapshot'",
        );
        let mut params = vec![Value::Integer(session_id)];

        if !file_filter.is_empty() {
            sql.push_str(" AND (");
            for (index, filter) in file_filter.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("file_path LIKE ?");
                params.push(Value::Text(format!("%{}%", filter)));
            }
            sql.push(')');
        }

        sql.push_str(" ORDER BY timestamp ASC, id ASC");

        let mut stmt = conn.prepare(&sql).unwrap();
        stmt.query_map(params_from_iter(params), |row| {
            Ok(ChangeRecord {
                file_path: row.get(0)?,
                blob_hash: row.get(1)?,
                ast_summary: row.get(2)?,
                tool_name: row.get(3)?,
                timestamp: row.get(4)?,
                agent_family: row.get(5)?,
                agent_name: row.get(6)?,
            })
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
    }

    pub fn get_session_timeline(&self, session_id: i64) -> Vec<TimelineEventRecord> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, session_id, seq_in_session, event_kind, timestamp, actor_family,
                        actor_name, file_path, blob_hash, tool_name, summary, payload_json, raw_bytes, stored_bytes
                 FROM timeline_events
                 WHERE session_id=?1
                 ORDER BY seq_in_session ASC, id ASC",
            )
            .unwrap();

        stmt.query_map([session_id], |row| {
            Ok(TimelineEventRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                session_id: row.get(2)?,
                seq_in_session: row.get(3)?,
                event_kind: row.get(4)?,
                timestamp: row.get(5)?,
                actor_family: row.get(6)?,
                actor_name: row.get(7)?,
                file_path: row.get(8)?,
                blob_hash: row.get(9)?,
                tool_name: row.get(10)?,
                summary: row.get(11)?,
                payload_json: row.get(12)?,
                raw_bytes: row.get::<_, i64>(13)? as u64,
                stored_bytes: row.get::<_, i64>(14)? as u64,
            })
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
    }

    pub fn get_project_timeline(&self, limit: u32) -> Vec<ProjectTimelineRecord> {
        let conn = self.conn();
        let project_id = self.project_id();
        let mut stmt = conn
            .prepare(
                "SELECT s.session_key, s.llm,
                        e.id, e.project_id, e.session_id, e.seq_in_session, e.event_kind, e.timestamp,
                        e.actor_family, e.actor_name, e.file_path, e.blob_hash, e.tool_name, e.summary,
                        e.payload_json, e.raw_bytes, e.stored_bytes
                 FROM timeline_events e
                 JOIN sessions s ON s.id = e.session_id
                 WHERE e.project_id=?1
                 ORDER BY e.timestamp DESC, e.id DESC
                 LIMIT ?2",
            )
            .unwrap();

        stmt.query_map(params![project_id, limit], |row| {
            Ok(ProjectTimelineRecord {
                session_key: row.get(0)?,
                llm: row.get(1)?,
                event: TimelineEventRecord {
                    id: row.get(2)?,
                    project_id: row.get(3)?,
                    session_id: row.get(4)?,
                    seq_in_session: row.get(5)?,
                    event_kind: row.get(6)?,
                    timestamp: row.get(7)?,
                    actor_family: row.get(8)?,
                    actor_name: row.get(9)?,
                    file_path: row.get(10)?,
                    blob_hash: row.get(11)?,
                    tool_name: row.get(12)?,
                    summary: row.get(13)?,
                    payload_json: row.get(14)?,
                    raw_bytes: row.get::<_, i64>(15)? as u64,
                    stored_bytes: row.get::<_, i64>(16)? as u64,
                },
            })
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
    }
}
