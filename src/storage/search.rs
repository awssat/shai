use super::{ChangeRecord, SearchMode, SessionRecord, Storage};
use rusqlite::params_from_iter;
use rusqlite::types::Value;

impl Storage {
    pub fn search_with_mode(
        &self,
        query: &str,
        limit: u32,
        mode: SearchMode,
    ) -> Vec<SessionRecord> {
        let conn = self.conn();
        let project_id = self.project_id();
        let pattern = format!("%{}%", query);

        let sql = match mode {
            SearchMode::Prompt => {
                "SELECT DISTINCT s.id, s.session_key, s.llm, s.started_at
                 FROM sessions s
                 JOIN timeline_events e ON e.session_id = s.id
                 WHERE s.project_id=?1 AND e.event_kind='prompt_submitted' AND e.summary LIKE ?2
                 ORDER BY s.started_at DESC
                 LIMIT ?3"
            }
            SearchMode::Path => {
                "SELECT DISTINCT s.id, s.session_key, s.llm, s.started_at
                 FROM sessions s
                 JOIN timeline_events e ON e.session_id = s.id
                 WHERE s.project_id=?1 AND e.event_kind='file_snapshot' AND e.file_path LIKE ?2
                 ORDER BY s.started_at DESC
                 LIMIT ?3"
            }
            SearchMode::Summary => {
                "SELECT DISTINCT s.id, s.session_key, s.llm, s.started_at
                 FROM sessions s
                 JOIN timeline_events e ON e.session_id = s.id
                 WHERE s.project_id=?1 AND e.event_kind='file_snapshot' AND e.summary LIKE ?2
                 ORDER BY s.started_at DESC
                 LIMIT ?3"
            }
            SearchMode::All => {
                "SELECT DISTINCT s.id, s.session_key, s.llm, s.started_at
                 FROM sessions s
                 LEFT JOIN timeline_events prompt ON prompt.session_id = s.id AND prompt.event_kind='prompt_submitted'
                 LEFT JOIN timeline_events file_evt ON file_evt.session_id = s.id AND file_evt.event_kind='file_snapshot'
                 WHERE s.project_id=?1
                   AND (
                       prompt.summary LIKE ?2
                       OR file_evt.file_path LIKE ?2
                       OR file_evt.summary LIKE ?2
                   )
                 ORDER BY s.started_at DESC
                 LIMIT ?3"
            }
        };

        // Step 1: collect session rows without any per-row queries.
        let mut stmt = conn.prepare(sql).unwrap();
        let session_rows: Vec<(i64, String, String, String)> = stmt
            .query_map(rusqlite::params![project_id, pattern, limit], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        drop(stmt);

        if session_rows.is_empty() {
            return Vec::new();
        }

        let session_ids: Vec<i64> = session_rows.iter().map(|r| r.0).collect();
        let placeholder = session_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let id_params: Vec<Value> = session_ids
            .iter()
            .map(|id| Value::Integer(*id))
            .collect();

        // Step 2: batch-fetch first prompt per session (one query total).
        let prompt_sql = format!(
            "SELECT t.session_id, t.summary
             FROM timeline_events t
             WHERE t.session_id IN ({placeholder}) AND t.event_kind='prompt_submitted'
               AND t.seq_in_session = (
                 SELECT MIN(t2.seq_in_session) FROM timeline_events t2
                 WHERE t2.session_id = t.session_id AND t2.event_kind='prompt_submitted'
               )"
        );
        let mut prompts: std::collections::HashMap<i64, String> =
            std::collections::HashMap::new();
        {
            let mut stmt = conn.prepare(&prompt_sql).unwrap();
            stmt.query_map(params_from_iter(id_params.clone()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .filter_map(Result::ok)
            .for_each(|(sid, prompt)| {
                prompts.insert(sid, prompt);
            });
        }

        // Step 3: batch-fetch all file changes for these sessions (one query total).
        let changes_sql = format!(
            "SELECT session_id, file_path, blob_hash, summary, COALESCE(tool_name, 'Write'),
                    timestamp, actor_family, actor_name
             FROM timeline_events
             WHERE session_id IN ({placeholder}) AND event_kind='file_snapshot'
             ORDER BY timestamp ASC, id ASC"
        );
        let mut changes_by_session: std::collections::HashMap<i64, Vec<ChangeRecord>> =
            session_ids.iter().map(|id| (*id, Vec::new())).collect();
        {
            let mut stmt = conn.prepare(&changes_sql).unwrap();
            stmt.query_map(params_from_iter(id_params), |row| {
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
        }

        // Step 4: assemble in original order.
        session_rows
            .into_iter()
            .map(|(id, session_key, llm, started_at)| SessionRecord {
                id,
                session_key,
                llm,
                prompt: prompts
                    .remove(&id)
                    .unwrap_or_else(|| "(no prompt)".to_string()),
                started_at,
                changes: changes_by_session.remove(&id).unwrap_or_default(),
            })
            .collect()
    }
}
