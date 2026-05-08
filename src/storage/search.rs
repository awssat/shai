use super::{SearchMode, SessionRecord, Storage};

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

        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map(rusqlite::params![project_id, pattern, limit], |row| {
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
                changes: self.load_changes_for_session(&conn, id, &[]),
            })
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
    }
}
