use crate::storage::types::{
    AnalyticsInfo, AnalyticsTouch, HotspotInfo, MissingPromptSession, StatusInfo, Storage,
    ToolUsage,
};

impl Storage {
    pub fn get_status(&self) -> StatusInfo {
        let conn = self.conn();
        let project_id = self.project_id();

        let total_sessions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE project_id=?1",
                [&project_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let open_sessions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE project_id=?1 AND closed_at IS NULL",
                [&project_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let total_changes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM timeline_events WHERE project_id=?1 AND event_kind='file_snapshot'",
                [&project_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let unique_files: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT file_path) FROM timeline_events WHERE project_id=?1 AND event_kind='file_snapshot'",
                [&project_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let (raw_bytes, stored_bytes): (i64, i64) = conn
            .query_row(
                "SELECT COALESCE(SUM(raw_bytes), 0), COALESCE(SUM(stored_bytes), 0)
                 FROM timeline_events
                 WHERE project_id=?1 AND event_kind='file_snapshot'",
                [&project_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0));

        let last_prompt: Option<String> = conn
            .query_row(
                "SELECT summary FROM timeline_events
                 WHERE project_id=?1 AND event_kind='prompt_submitted'
                 ORDER BY timestamp DESC, id DESC LIMIT 1",
                [&project_id],
                |r| r.get(0),
            )
            .ok();
        let last_checkpoint: Option<String> = conn
            .query_row(
                "SELECT summary FROM timeline_events
                 WHERE project_id=?1 AND event_kind='checkpoint_created'
                 ORDER BY timestamp DESC, id DESC LIMIT 1",
                [&project_id],
                |r| r.get(0),
            )
            .ok();
        let last_checkpoint_at: Option<String> = conn
            .query_row(
                "SELECT timestamp FROM timeline_events
                 WHERE project_id=?1 AND event_kind='checkpoint_created'
                 ORDER BY timestamp DESC, id DESC LIMIT 1",
                [&project_id],
                |r| r.get(0),
            )
            .ok();
        let last_at: Option<String> = conn
            .query_row(
                "SELECT started_at FROM sessions WHERE project_id=?1 ORDER BY started_at DESC LIMIT 1",
                [&project_id],
                |r| r.get(0),
            )
            .ok();
        let last_change_at: Option<String> = conn
            .query_row(
                "SELECT timestamp FROM timeline_events
                 WHERE project_id=?1 AND event_kind='file_snapshot'
                 ORDER BY timestamp DESC LIMIT 1",
                [&project_id],
                |r| r.get(0),
            )
            .ok();
        let first_at: Option<String> = conn
            .query_row(
                "SELECT started_at FROM sessions WHERE project_id=?1 ORDER BY started_at ASC LIMIT 1",
                [&project_id],
                |r| r.get(0),
            )
            .ok();

        let top_agents = {
            let mut stmt = conn
                .prepare(
                    "SELECT llm, COUNT(*) FROM sessions
                     WHERE project_id=?1
                     GROUP BY llm
                     ORDER BY COUNT(*) DESC
                     LIMIT 5",
                )
                .unwrap();
            stmt.query_map([&project_id], |r| {
                Ok((r.get(0)?, r.get::<_, i64>(1)? as usize))
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect()
        };

        let top_files = {
            let mut stmt = conn
                .prepare(
                    "SELECT file_path, COUNT(*) FROM timeline_events
                     WHERE project_id=?1 AND event_kind='file_snapshot'
                     GROUP BY file_path
                     ORDER BY COUNT(*) DESC
                     LIMIT 5",
                )
                .unwrap();
            stmt.query_map([&project_id], |r| {
                Ok((r.get(0)?, r.get::<_, i64>(1)? as usize))
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect()
        };

        let storage_hotspots = {
            let mut stmt = conn
                .prepare(
                    "SELECT file_path, COUNT(*), COALESCE(SUM(raw_bytes), 0), COALESCE(SUM(stored_bytes), 0)
                     FROM timeline_events
                     WHERE project_id=?1 AND event_kind='file_snapshot'
                     GROUP BY file_path
                     ORDER BY SUM(stored_bytes) DESC
                     LIMIT 5",
                )
                .unwrap();
            stmt.query_map([&project_id], |r| {
                Ok(HotspotInfo {
                    file_path: r.get(0)?,
                    revisions: r.get::<_, i64>(1)? as usize,
                    raw_bytes: r.get::<_, i64>(2)? as u64,
                    stored_bytes: r.get::<_, i64>(3)? as u64,
                })
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect()
        };

        StatusInfo {
            total_sessions: total_sessions as usize,
            open_sessions: open_sessions as usize,
            total_changes: total_changes as usize,
            unique_files: unique_files as usize,
            project_id: project_id.clone(),
            raw_bytes: raw_bytes as u64,
            stored_bytes: stored_bytes as u64,
            compression_ratio: if stored_bytes > 0 {
                raw_bytes as f64 / stored_bytes as f64
            } else {
                1.0
            },
            last_prompt,
            last_checkpoint,
            last_checkpoint_at,
            last_at,
            last_change_at,
            first_at,
            top_agents,
            top_files,
            storage_hotspots,
        }
    }

    pub fn get_analytics(
        &self,
        file_filter: Option<&str>,
        _subsystem_filter: Option<&str>,
        limit: u32,
    ) -> AnalyticsInfo {
        let conn = self.conn();
        let project_id = self.project_id();

        let recent_touches = {
            let mut sql = String::from(
                "SELECT e.file_path, COUNT(*), MAX(e.timestamp), s.agent_family,
                        CASE WHEN EXISTS (
                            SELECT 1 FROM timeline_events prompt
                            WHERE prompt.session_id = s.id AND prompt.event_kind='prompt_submitted'
                        ) THEN 'user' ELSE 'missing' END,
                        s.llm,
                        COALESCE(e.tool_name, 'Write')
                 FROM timeline_events e
                 JOIN sessions s ON s.id = e.session_id
                 WHERE e.project_id=?1 AND e.event_kind='file_snapshot'",
            );
            if let Some(filter) = file_filter {
                sql.push_str(&format!(" AND e.file_path LIKE '%{}%'", filter));
            }
            sql.push_str(
                " GROUP BY e.file_path, e.tool_name, s.llm
                  ORDER BY MAX(e.timestamp) DESC
                  LIMIT ?2",
            );
            let mut stmt = conn.prepare(&sql).unwrap();
            stmt.query_map(rusqlite::params![project_id, limit], |r| {
                Ok(AnalyticsTouch {
                    file_path: r.get(0)?,
                    touch_count: r.get::<_, i64>(1)? as usize,
                    agent_family: r.get(3)?,
                    prompt_kind: r.get(4)?,
                    timestamp: r.get(2)?,
                    llm: r.get(5)?,
                    tool_name_norm: r.get(6)?,
                })
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect()
        };

        let top_tools = {
            let mut stmt = conn
                .prepare(
                    "SELECT COALESCE(tool_name, 'Write'), COUNT(*)
                     FROM timeline_events
                     WHERE project_id=?1 AND event_kind='tool_called'
                     GROUP BY tool_name
                     ORDER BY COUNT(*) DESC
                     LIMIT ?2",
                )
                .unwrap();
            stmt.query_map(rusqlite::params![project_id, limit], |r| {
                Ok(ToolUsage {
                    tool_name_norm: r.get(0)?,
                    count: r.get::<_, i64>(1)? as usize,
                })
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect()
        };

        let missing_prompt_sessions = {
            let mut stmt = conn
                .prepare(
                    "SELECT s.session_key,
                            (
                                SELECT COUNT(*) FROM timeline_events ch
                                WHERE ch.session_id=s.id AND ch.event_kind='file_snapshot'
                            ),
                            s.started_at,
                            s.llm
                     FROM sessions s
                     WHERE s.project_id=?1
                       AND NOT EXISTS (
                           SELECT 1 FROM timeline_events prompt
                           WHERE prompt.session_id=s.id AND prompt.event_kind='prompt_submitted'
                       )
                     ORDER BY s.started_at DESC
                     LIMIT ?2",
                )
                .unwrap();
            stmt.query_map(rusqlite::params![project_id, limit], |r| {
                Ok(MissingPromptSession {
                    session_key: r.get(0)?,
                    change_count: r.get::<_, i64>(1)? as usize,
                    started_at: r.get(2)?,
                    llm: r.get(3)?,
                })
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect()
        };

        AnalyticsInfo {
            recent_touches,
            top_tools,
            missing_prompt_sessions,
        }
    }
}
