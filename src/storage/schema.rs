use super::Storage;

impl Storage {
    pub fn init_schema(&self) {
        let conn = self.conn();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_key TEXT NOT NULL,
                project_id  TEXT NOT NULL DEFAULT '',
                llm         TEXT NOT NULL DEFAULT 'claude',
                agent_family TEXT NOT NULL DEFAULT '',
                agent_name  TEXT NOT NULL DEFAULT '',
                prompt      TEXT NOT NULL,
                prompt_kind TEXT NOT NULL DEFAULT 'user',
                payload_json TEXT,
                started_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
                closed_at   DATETIME
            );
            CREATE TABLE IF NOT EXISTS changes (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  INTEGER REFERENCES sessions(id),
                project_id  TEXT NOT NULL DEFAULT '',
                agent_family TEXT NOT NULL DEFAULT '',
                agent_name  TEXT NOT NULL DEFAULT '',
                timestamp   DATETIME DEFAULT CURRENT_TIMESTAMP,
                file_path   TEXT NOT NULL,
                blob_hash   TEXT NOT NULL,
                ast_summary TEXT NOT NULL DEFAULT '',
                tool_name   TEXT NOT NULL DEFAULT 'Write',
                tool_name_raw TEXT NOT NULL DEFAULT '',
                tool_name_norm TEXT NOT NULL DEFAULT '',
                event_kind  TEXT NOT NULL DEFAULT 'file_change',
                storage_kind TEXT NOT NULL DEFAULT 'full',
                base_change_id INTEGER,
                compression TEXT NOT NULL DEFAULT 'zstd',
                file_revision INTEGER NOT NULL DEFAULT 0,
                raw_bytes   INTEGER NOT NULL DEFAULT 0,
                stored_bytes INTEGER NOT NULL DEFAULT 0,
                payload_json TEXT
            );
            CREATE TABLE IF NOT EXISTS internal_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_sess_project ON sessions(project_id);
            CREATE INDEX IF NOT EXISTS idx_sess_key    ON sessions(session_key);
            CREATE INDEX IF NOT EXISTS idx_chg_project ON changes(project_id);
            CREATE INDEX IF NOT EXISTS idx_chg_session ON changes(session_id);
            CREATE INDEX IF NOT EXISTS idx_chg_file    ON changes(file_path);
        ",
        )
        .unwrap_or_else(|err| {
            tracing::error!("shai: schema init failed: {err}");
        });

        conn.execute(
            "INSERT INTO internal_state(key, value) VALUES('schema_version', '10')
             ON CONFLICT(key) DO UPDATE SET value='10'",
            [],
        )
        .ok();
    }
}
