use super::{ChangeRecord, FileChangeRecord, SessionRecord, Storage};
use rusqlite::{params_from_iter, types::Value, Connection};

impl Storage {
    pub fn get_file_at_step(
        &self,
        file_path: &str,
        steps: u32,
    ) -> Option<(String, String, String)> {
        let conn = self.conn();
        conn.query_row(
            "SELECT blob_hash, timestamp, ast_summary FROM changes
             WHERE file_path = ?1
             ORDER BY timestamp DESC, id DESC
             LIMIT 1 OFFSET ?2",
            rusqlite::params![file_path, steps.saturating_sub(1)],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok()
    }

    pub fn get_file_history(&self, file_path: &str, limit: u32) -> Vec<FileChangeRecord> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT c.timestamp, c.tool_name, c.ast_summary, c.blob_hash, s.prompt, s.llm
                 FROM changes c
                 JOIN sessions s ON s.id = c.session_id
                 WHERE c.file_path = ?1
                 ORDER BY c.timestamp DESC, c.id DESC
                 LIMIT ?2",
            )
            .unwrap();

        stmt.query_map(rusqlite::params![file_path, limit], |row| {
            Ok(FileChangeRecord {
                timestamp: row.get(0)?,
                tool_name: row.get(1)?,
                ast_summary: row.get(2)?,
                blob_hash: row.get(3)?,
                prompt: row.get(4)?,
                llm: row.get(5)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
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
        let mut sql = String::from("SELECT id, session_key, llm, prompt, started_at FROM sessions WHERE project_id=?");
        let mut params = vec![Value::Text(self.project_id())];

        if let Some(s) = since {
            sql.push_str(" AND started_at >= ?");
            params.push(Value::Text(s.to_string()));
        }

        if !file_filter.is_empty() {
            sql.push_str(" AND id IN (SELECT session_id FROM changes WHERE ");
            for (index, filter) in file_filter.iter().enumerate() {
                if index > 0 { sql.push_str(" OR "); }
                sql.push_str("file_path LIKE ?");
                params.push(Value::Text(format!("%{}%", filter)));
            }
            sql.push(')');
        }

        sql.push_str(" ORDER BY started_at DESC LIMIT ?");
        params.push(Value::Integer(limit as i64));

        let mut stmt = conn.prepare(&sql).unwrap();
        stmt.query_map(params_from_iter(params), |row| {
            let id: i64 = row.get(0)?;
            Ok(SessionRecord {
                id,
                session_key: row.get(1)?,
                llm: row.get(2)?,
                prompt: row.get(3)?,
                started_at: row.get(4)?,
                changes: self.load_changes_for_session(&conn, id, file_filter),
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn load_changes_for_session(
        &self,
        conn: &Connection,
        session_id: i64,
        file_filter: &[String],
    ) -> Vec<ChangeRecord> {
        let mut sql = String::from(
            "SELECT file_path, blob_hash, ast_summary, tool_name, timestamp, agent_family, agent_name
             FROM changes
             WHERE session_id=?",
        );
        let mut params = vec![Value::Integer(session_id)];

        if !file_filter.is_empty() {
            sql.push_str(" AND (");
            for (index, filter) in file_filter.iter().enumerate() {
                if index > 0 { sql.push_str(" OR "); }
                sql.push_str("file_path LIKE ?");
                params.push(Value::Text(format!("%{}%", filter)));
            }
            sql.push(')');
        }

        sql.push_str(" ORDER BY timestamp ASC");

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
        .filter_map(|r| r.ok())
        .collect()
    }
}
