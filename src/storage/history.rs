use super::{
    ChangeRecord, FileChangeRecord, ProjectTimelineRecord, SessionRecord, Storage,
    TimelineEventRecord,
};
use rusqlite::{params, params_from_iter, types::Value};

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

        // --- Step 1: fetch sessions with prompt as a correlated subquery (no extra round-trips) ---
        let mut sql = String::from(
            "SELECT s.id, s.session_key, s.llm, s.started_at,
                    (
                        SELECT p.summary FROM timeline_events p
                        WHERE p.session_id = s.id AND p.event_kind='prompt_submitted'
                        ORDER BY p.seq_in_session ASC LIMIT 1
                    ) AS prompt
             FROM sessions s
             WHERE s.project_id=?1",
        );
        let mut params = vec![Value::Text(self.project_id())];

        if let Some(since) = since {
            sql.push_str(" AND s.started_at >= ?");
            params.push(Value::Text(since.to_string()));
        }

        if !file_filter.is_empty() {
            sql.push_str(
                " AND s.id IN (SELECT session_id FROM timeline_events \
                  WHERE event_kind='file_snapshot' AND (",
            );
            for (index, filter) in file_filter.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("file_path LIKE ?");
                params.push(Value::Text(format!("%{}%", filter)));
            }
            sql.push_str("))");
        }

        sql.push_str(" ORDER BY s.started_at DESC LIMIT ?");
        params.push(Value::Integer(limit as i64));

        let mut stmt = conn.prepare(&sql).unwrap();
        // (session_id, session_key, llm, started_at, prompt)
        type SessionRow = (i64, String, Option<String>, String, Option<String>);
        let rows: Vec<SessionRow> = stmt
            .query_map(params_from_iter(params), |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        if rows.is_empty() {
            return Vec::new();
        }

        // --- Step 2: load all changes for these sessions in one batch query ---
        let session_ids: Vec<i64> = rows.iter().map(|(id, ..)| *id).collect();
        let mut changes_by_session: std::collections::HashMap<i64, Vec<ChangeRecord>> =
            session_ids.iter().map(|id| (*id, Vec::new())).collect();

        let placeholder = session_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let mut changes_sql = format!(
            "SELECT session_id, file_path, blob_hash, summary, COALESCE(tool_name, 'Write'), timestamp, actor_family, actor_name
             FROM timeline_events
             WHERE session_id IN ({}) AND event_kind='file_snapshot'",
            placeholder
        );
        let mut changes_params: Vec<Value> = session_ids
            .iter()
            .map(|id| Value::Integer(*id))
            .collect();

        if !file_filter.is_empty() {
            changes_sql.push_str(" AND (");
            for (index, filter) in file_filter.iter().enumerate() {
                if index > 0 {
                    changes_sql.push_str(" OR ");
                }
                changes_sql.push_str("file_path LIKE ?");
                changes_params.push(Value::Text(format!("%{}%", filter)));
            }
            changes_sql.push(')');
        }
        changes_sql.push_str(" ORDER BY timestamp ASC, id ASC");

        let mut ch_stmt = conn.prepare(&changes_sql).unwrap();
        ch_stmt
            .query_map(params_from_iter(changes_params), |row| {
                let sid: i64 = row.get(0)?;
                Ok((
                    sid,
                    ChangeRecord {
                        file_path: row.get(1)?,
                        blob_hash: row.get(2)?,
                        ast_summary: row.get(3)?,
                        tool_name: row.get(4)?,
                        timestamp: row.get(5)?,
                        agent_family: row.get(6)?,
                        agent_name: row.get(7)?,
                    },
                ))
            })
            .unwrap()
            .filter_map(Result::ok)
            .for_each(|(sid, rec)| {
                changes_by_session.entry(sid).or_default().push(rec);
            });

        // --- Step 3: assemble results in original order ---
        rows.into_iter()
            .map(|(id, session_key, llm, started_at, prompt)| SessionRecord {
                id,
                session_key,
                llm: llm.unwrap_or_default(),
                prompt: prompt.unwrap_or_else(|| "(no prompt)".to_string()),
                started_at,
                changes: changes_by_session.remove(&id).unwrap_or_default(),
            })
            .collect()
    }

    /// Batch-fetch display events (file_snapshot, checkpoint_created, guard_blocked,
    /// guard_allowed) for multiple sessions in one query. Returns a map from session_id
    /// to the ordered list of events, avoiding one query per session in `cmd_history`.
    pub fn get_display_events_batch(
        &self,
        session_ids: &[i64],
    ) -> std::collections::HashMap<i64, Vec<TimelineEventRecord>> {
        if session_ids.is_empty() {
            return std::collections::HashMap::new();
        }
        let conn = self.conn();
        let placeholder = session_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, project_id, session_id, seq_in_session, event_kind, timestamp,
                    actor_family, actor_name, file_path, blob_hash, tool_name, summary,
                    payload_json, raw_bytes, stored_bytes
             FROM timeline_events
             WHERE session_id IN ({placeholder})
               AND event_kind IN ('file_snapshot', 'checkpoint_created', 'guard_blocked', 'guard_allowed')
             ORDER BY session_id, seq_in_session ASC, id ASC"
        );
        let id_params: Vec<Value> = session_ids
            .iter()
            .map(|id| Value::Integer(*id))
            .collect();
        let mut by_session: std::collections::HashMap<i64, Vec<TimelineEventRecord>> =
            session_ids.iter().map(|id| (*id, Vec::new())).collect();
        let mut stmt = conn.prepare(&sql).unwrap();
        stmt.query_map(params_from_iter(id_params), |row| {
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
        .for_each(|ev| {
            by_session.entry(ev.session_id).or_default().push(ev);
        });
        by_session
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
