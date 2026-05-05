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
        
        let sql = match mode {
            SearchMode::Prompt => {
                "SELECT id, session_key, llm, prompt, started_at FROM sessions
                 WHERE project_id=?1 AND prompt LIKE ?2
                 ORDER BY started_at DESC LIMIT ?3"
            }
            SearchMode::Path => {
                "SELECT id, session_key, llm, prompt, started_at FROM sessions
                 WHERE project_id=?1 AND id IN (SELECT session_id FROM changes WHERE file_path LIKE ?2)
                 ORDER BY started_at DESC LIMIT ?3"
            }
            SearchMode::Summary => {
                "SELECT id, session_key, llm, prompt, started_at FROM sessions
                 WHERE project_id=?1 AND id IN (SELECT session_id FROM changes WHERE ast_summary LIKE ?2)
                 ORDER BY started_at DESC LIMIT ?3"
            }
            SearchMode::All => {
                "SELECT id, session_key, llm, prompt, started_at FROM sessions
                 WHERE project_id=?1 AND (prompt LIKE ?2 OR id IN (SELECT session_id FROM changes WHERE file_path LIKE ?2 OR ast_summary LIKE ?2))
                 ORDER BY started_at DESC LIMIT ?3"
            }
        };

        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map(rusqlite::params![&project_id, &pattern, limit], |row| {
            let id: i64 = row.get(0)?;
            Ok(SessionRecord {
                id,
                session_key: row.get(1)?,
                llm: row.get(2)?,
                prompt: row.get(3)?,
                started_at: row.get(4)?,
                changes: self.load_changes_for_session(&conn, id, &[]),
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }
}
