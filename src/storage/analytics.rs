use crate::storage::types::{
    AnalyticsInfo, StatusInfo, Storage, HotspotInfo, AnalyticsTouch, ToolUsage, MissingPromptSession
};

impl Storage {
    pub fn get_status(&self) -> StatusInfo {
        let conn = self.conn();
        let project_id = self.project_id();

        let total_sessions: i64 = conn.query_row("SELECT COUNT(*) FROM sessions WHERE project_id=?1", [&project_id], |r| r.get(0)).unwrap_or(0);
        let open_sessions: i64 = conn.query_row("SELECT COUNT(*) FROM sessions WHERE project_id=?1 AND closed_at IS NULL", [&project_id], |r| r.get(0)).unwrap_or(0);
        let total_changes: i64 = conn.query_row("SELECT COUNT(*) FROM changes WHERE project_id=?1", [&project_id], |r| r.get(0)).unwrap_or(0);
        let unique_files: i64 = conn.query_row("SELECT COUNT(DISTINCT file_path) FROM changes WHERE project_id=?1", [&project_id], |r| r.get(0)).unwrap_or(0);
        let (raw_bytes, stored_bytes): (i64, i64) = conn.query_row("SELECT SUM(raw_bytes), SUM(stored_bytes) FROM changes WHERE project_id=?1", [&project_id], |r| Ok((r.get(0).unwrap_or(0), r.get(1).unwrap_or(0)))).unwrap_or((0, 0));

        let last_prompt: Option<String> = conn.query_row("SELECT prompt FROM sessions WHERE project_id=?1 ORDER BY started_at DESC LIMIT 1", [&project_id], |r| r.get(0)).ok();
        let last_at: Option<String> = conn.query_row("SELECT started_at FROM sessions WHERE project_id=?1 ORDER BY started_at DESC LIMIT 1", [&project_id], |r| r.get(0)).ok();
        let last_change_at: Option<String> = conn.query_row("SELECT timestamp FROM changes WHERE project_id=?1 ORDER BY timestamp DESC LIMIT 1", [&project_id], |r| r.get(0)).ok();
        let first_at: Option<String> = conn.query_row("SELECT started_at FROM sessions WHERE project_id=?1 ORDER BY started_at ASC LIMIT 1", [&project_id], |r| r.get(0)).ok();

        let top_agents = {
            let mut stmt = conn.prepare("SELECT llm, COUNT(*) FROM sessions WHERE project_id=?1 GROUP BY llm ORDER BY COUNT(*) DESC LIMIT 5").unwrap();
            stmt.query_map([&project_id], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize))).unwrap().filter_map(|r| r.ok()).collect()
        };

        let top_files = {
            let mut stmt = conn.prepare("SELECT file_path, COUNT(*) FROM changes WHERE project_id=?1 GROUP BY file_path ORDER BY COUNT(*) DESC LIMIT 5").unwrap();
            stmt.query_map([&project_id], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize))).unwrap().filter_map(|r| r.ok()).collect()
        };

        let storage_hotspots = {
            let mut stmt = conn.prepare("SELECT file_path, COUNT(*), SUM(raw_bytes), SUM(stored_bytes) FROM changes WHERE project_id=?1 GROUP BY file_path ORDER BY SUM(stored_bytes) DESC LIMIT 5").unwrap();
            stmt.query_map([&project_id], |r| Ok(HotspotInfo {
                file_path: r.get(0)?,
                revisions: r.get::<_, i64>(1)? as usize,
                raw_bytes: r.get::<_, i64>(2)? as u64,
                stored_bytes: r.get::<_, i64>(3)? as u64,
            })).unwrap().filter_map(|r| r.ok()).collect()
        };

        StatusInfo {
            total_sessions: total_sessions as usize,
            open_sessions: open_sessions as usize,
            total_changes: total_changes as usize,
            unique_files: unique_files as usize,
            project_id: project_id.clone(),
            raw_bytes: raw_bytes as u64,
            stored_bytes: stored_bytes as u64,
            compression_ratio: if stored_bytes > 0 { raw_bytes as f64 / stored_bytes as f64 } else { 1.0 },
            last_prompt, last_at, last_change_at, first_at,
            top_agents, top_files, storage_hotspots,
        }
    }

    pub fn get_analytics(&self, file_filter: Option<&str>, _subsystem_filter: Option<&str>, limit: u32) -> AnalyticsInfo {
        let conn = self.conn();
        let project_id = self.project_id();

        let recent_touches = {
            let mut sql = String::from("SELECT c.file_path, COUNT(*), MAX(c.timestamp), s.agent_family, s.prompt_kind, s.llm, c.tool_name_norm FROM changes c JOIN sessions s ON s.id = c.session_id WHERE c.project_id=?1");
            if let Some(f) = file_filter { sql.push_str(&format!(" AND c.file_path LIKE '%{}%'", f)); }
            sql.push_str(" GROUP BY c.file_path, c.tool_name_norm, s.llm ORDER BY MAX(c.timestamp) DESC LIMIT ?2");
            let mut stmt = conn.prepare(&sql).unwrap();
            stmt.query_map(rusqlite::params![project_id, limit], |r| Ok(AnalyticsTouch {
                file_path: r.get(0)?,
                touch_count: r.get::<_, i64>(1)? as usize,
                agent_family: r.get(3)?,
                prompt_kind: r.get(4)?,
                timestamp: r.get(2)?,
                llm: r.get(5)?,
                tool_name_norm: r.get(6)?,
            })).unwrap().filter_map(|r| r.ok()).collect()
        };

        let top_tools = {
            let mut stmt = conn.prepare("SELECT tool_name_norm, COUNT(*) FROM changes WHERE project_id=?1 GROUP BY tool_name_norm ORDER BY COUNT(*) DESC LIMIT ?2").unwrap();
            stmt.query_map(rusqlite::params![project_id, limit], |r| Ok(ToolUsage {
                tool_name_norm: r.get(0)?,
                count: r.get::<_, i64>(1)? as usize,
            })).unwrap().filter_map(|r| r.ok()).collect()
        };

        let missing_prompt_sessions = {
            let mut stmt = conn.prepare("SELECT session_key, (SELECT COUNT(*) FROM changes WHERE session_id=s.id), started_at, llm FROM sessions s WHERE project_id=?1 AND (prompt IS NULL OR prompt = '' OR prompt = '(unrecorded prompt)') ORDER BY started_at DESC LIMIT ?2").unwrap();
            stmt.query_map(rusqlite::params![project_id, limit], |r| Ok(MissingPromptSession {
                session_key: r.get(0)?,
                change_count: r.get::<_, i64>(1)? as usize,
                started_at: r.get(2)?,
                llm: r.get(3)?,
            })).unwrap().filter_map(|r| r.ok()).collect()
        };

        AnalyticsInfo { recent_touches, top_tools, missing_prompt_sessions }
    }
}
