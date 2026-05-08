use rusqlite::{params, Connection};
use std::collections::HashMap;

use super::Storage;

const SCHEMA_VERSION: i64 = 11;

impl Storage {
    pub fn init_schema(&self) {
        let conn = self.conn();
        let version = current_schema_version(&conn);
        let has_timeline = table_exists(&conn, "timeline_events");

        if version == Some(SCHEMA_VERSION) && has_timeline {
            return;
        }

        if table_exists(&conn, "sessions") || table_exists(&conn, "changes") {
            if let Err(err) = migrate_v10_to_v11(&conn) {
                panic!("shai: fatal schema migration failure: {}", err);
            }
        } else {
            create_v11_schema(&conn);
            set_schema_version(&conn, SCHEMA_VERSION);
        }
    }
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

fn current_schema_version(conn: &Connection) -> Option<i64> {
    if !table_exists(conn, "internal_state") {
        return None;
    }
    conn.query_row(
        "SELECT CAST(value AS INTEGER) FROM internal_state WHERE key='schema_version'",
        [],
        |row| row.get(0),
    )
    .ok()
}

fn set_schema_version(conn: &Connection, version: i64) {
    let _ = conn.execute(
        "INSERT INTO internal_state(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [version.to_string()],
    );
}

fn create_v11_schema(conn: &Connection) {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            session_key  TEXT NOT NULL,
            project_id   TEXT NOT NULL,
            llm          TEXT NOT NULL,
            agent_family TEXT NOT NULL DEFAULT '',
            agent_name   TEXT NOT NULL DEFAULT '',
            started_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            closed_at    DATETIME
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_identity
            ON sessions(session_key, llm, started_at);
        CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project_id, started_at DESC);

        CREATE TABLE IF NOT EXISTS timeline_events (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id     TEXT NOT NULL,
            session_id     INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            seq_in_session INTEGER NOT NULL,
            event_kind     TEXT NOT NULL,
            timestamp      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            actor_family   TEXT NOT NULL DEFAULT '',
            actor_name     TEXT NOT NULL DEFAULT '',
            file_path      TEXT,
            blob_hash      TEXT,
            tool_name      TEXT,
            summary        TEXT NOT NULL DEFAULT '',
            payload_json   TEXT,
            storage_kind   TEXT NOT NULL DEFAULT 'full',
            base_event_id  INTEGER,
            raw_bytes      INTEGER NOT NULL DEFAULT 0,
            stored_bytes   INTEGER NOT NULL DEFAULT 0
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_events_session_seq
            ON timeline_events(session_id, seq_in_session);
        CREATE INDEX IF NOT EXISTS idx_events_project_time
            ON timeline_events(project_id, timestamp DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_events_file
            ON timeline_events(file_path, timestamp DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_events_kind
            ON timeline_events(event_kind, timestamp DESC, id DESC);

        CREATE TABLE IF NOT EXISTS memory_facts (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id  TEXT NOT NULL,
            fact_key    TEXT NOT NULL,
            content     TEXT NOT NULL,
            verified    INTEGER NOT NULL DEFAULT 0,
            source      TEXT NOT NULL DEFAULT '',
            created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS memory_decisions (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id    TEXT NOT NULL,
            title         TEXT NOT NULL,
            rationale     TEXT NOT NULL DEFAULT '',
            alternatives  TEXT NOT NULL DEFAULT '',
            status        TEXT NOT NULL DEFAULT 'active',
            verified      INTEGER NOT NULL DEFAULT 0,
            created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS memory_refs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id  TEXT NOT NULL,
            ref_kind    TEXT NOT NULL,
            ref_value   TEXT NOT NULL,
            target_kind TEXT NOT NULL,
            target_id   INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS internal_state (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
    ",
    )
    .unwrap_or_else(|err| {
        tracing::error!("shai: schema init failed: {err}");
    });
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("PRAGMA table_info(\"{}\")", table);
    conn.prepare(&sql)
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(1))
                .ok()
                .map(|rows| rows.filter_map(Result::ok).any(|col| col == column))
        })
        .unwrap_or(false)
}

fn migrate_v10_to_v11(conn: &Connection) -> Result<(), rusqlite::Error> {
    // Run the entire migration inside a single IMMEDIATE transaction.
    conn.execute_batch("BEGIN IMMEDIATE")?;

    let result = (|| -> Result<(), rusqlite::Error> {
        if table_exists(conn, "timeline_events") {
            conn.execute_batch("DROP TABLE timeline_events")?;
        }
        if table_exists(conn, "memory_facts") {
            conn.execute_batch("DROP TABLE memory_facts")?;
        }
        if table_exists(conn, "memory_decisions") {
            conn.execute_batch("DROP TABLE memory_decisions")?;
        }
        if table_exists(conn, "memory_refs") {
            conn.execute_batch("DROP TABLE memory_refs")?;
        }
        if table_exists(conn, "sessions_v10") {
            conn.execute_batch("DROP TABLE sessions_v10")?;
        }
        if table_exists(conn, "changes_v10") {
            conn.execute_batch("DROP TABLE changes_v10")?;
        }

        if table_exists(conn, "sessions") {
            conn.execute_batch("ALTER TABLE sessions RENAME TO sessions_v10")?;
        }
        if table_exists(conn, "changes") {
            conn.execute_batch("ALTER TABLE changes RENAME TO changes_v10")?;
        }

        create_v11_schema(conn);

        if table_exists(conn, "sessions_v10") {
            let has_prompt = column_exists(conn, "sessions_v10", "prompt");
            let has_agent_family = column_exists(conn, "sessions_v10", "agent_family");
            let has_agent_name = column_exists(conn, "sessions_v10", "agent_name");

            let prompt_col = if has_prompt { "prompt" } else { "'' AS prompt" };
            let family_col = if has_agent_family { "agent_family" } else { "'' AS agent_family" };
            let name_col = if has_agent_name { "agent_name" } else { "'' AS agent_name" };

            let query = format!(
                "SELECT id, session_key, project_id, llm, {family_col}, {name_col}, {prompt_col}, started_at, closed_at
                 FROM sessions_v10
                 ORDER BY id ASC"
            );

            let old_sessions = {
                let mut stmt = conn.prepare(&query)?;
                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                })
                .unwrap()
                .filter_map(Result::ok)
                .collect::<Vec<_>>();
                rows
            };

            let mut seq_by_session: HashMap<i64, i64> = HashMap::new();
            let mut closed_by_session: HashMap<i64, Option<String>> = HashMap::new();

            for (
                id,
                session_key,
                project_id,
                llm,
                agent_family,
                agent_name,
                prompt,
                started_at,
                closed_at,
            ) in old_sessions
            {
                conn.execute(
                    "INSERT INTO sessions (id, session_key, project_id, llm, agent_family, agent_name, started_at, closed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        id,
                        session_key,
                        project_id,
                        llm,
                        agent_family,
                        agent_name,
                        started_at,
                        closed_at
                    ],
                )?;

                let mut seq = 1i64;
                conn.execute(
                    "INSERT INTO timeline_events (
                        project_id, session_id, seq_in_session, event_kind, timestamp, actor_family, actor_name, summary
                     ) VALUES (?1, ?2, ?3, 'session_started', ?4, ?5, ?6, '')",
                    params![project_id, id, seq, started_at, agent_family, agent_name],
                )?;

                if !prompt.trim().is_empty() && prompt != "(unrecorded prompt)" {
                    seq += 1;
                    conn.execute(
                        "INSERT INTO timeline_events (
                            project_id, session_id, seq_in_session, event_kind, timestamp, actor_family, actor_name, summary
                         ) VALUES (?1, ?2, ?3, 'prompt_submitted', ?4, ?5, ?6, ?7)",
                        params![project_id, id, seq, started_at, agent_family, agent_name, prompt],
                    )?;
                }

                seq_by_session.insert(id, seq);
                closed_by_session.insert(id, closed_at);
            }

            if table_exists(conn, "changes_v10") {
                let ch_has_agent_family = column_exists(conn, "changes_v10", "agent_family");
                let ch_has_agent_name = column_exists(conn, "changes_v10", "agent_name");
                let ch_has_base = column_exists(conn, "changes_v10", "base_change_id");
                let ch_has_raw = column_exists(conn, "changes_v10", "raw_bytes");
                let ch_has_stored = column_exists(conn, "changes_v10", "stored_bytes");

                let family_col = if ch_has_agent_family { "agent_family" } else { "'' AS agent_family" };
                let name_col = if ch_has_agent_name { "agent_name" } else { "'' AS agent_name" };
                let base_col = if ch_has_base { "base_change_id" } else { "NULL AS base_change_id" };
                let raw_col = if ch_has_raw { "raw_bytes" } else { "0 AS raw_bytes" };
                let stored_col = if ch_has_stored { "stored_bytes" } else { "0 AS stored_bytes" };

                let changes_query = format!(
                    "SELECT session_id, project_id, {family_col}, {name_col}, timestamp, file_path, blob_hash,
                            ast_summary, tool_name_raw, payload_json, storage_kind, {base_col}, {raw_col}, {stored_col}
                     FROM changes_v10
                     ORDER BY session_id ASC, timestamp ASC, id ASC"
                );

                let old_changes = {
                    let mut stmt = conn.prepare(&changes_query)?;
                    let rows = stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, Option<String>>(9)?,
                            row.get::<_, String>(10)?,
                            row.get::<_, Option<i64>>(11)?,
                            row.get::<_, i64>(12)?,
                            row.get::<_, i64>(13)?,
                        ))
                    })
                    .unwrap()
                    .filter_map(Result::ok)
                    .collect::<Vec<_>>();
                    rows
                };

                for (
                    session_id,
                    project_id,
                    actor_family,
                    actor_name,
                    timestamp,
                    file_path,
                    blob_hash,
                    ast_summary,
                    tool_name,
                    payload_json,
                    storage_kind,
                    base_event_id,
                    raw_bytes,
                    stored_bytes,
                ) in old_changes
                {
                    let seq = seq_by_session.entry(session_id).or_insert(0);
                    *seq += 1;
                    conn.execute(
                        "INSERT INTO timeline_events (
                            project_id, session_id, seq_in_session, event_kind, timestamp, actor_family, actor_name,
                            file_path, blob_hash, tool_name, summary, payload_json, storage_kind, base_event_id,
                            raw_bytes, stored_bytes
                         ) VALUES (?1, ?2, ?3, 'file_snapshot', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                        params![
                            project_id,
                            session_id,
                            *seq,
                            timestamp,
                            actor_family,
                            actor_name,
                            file_path,
                            blob_hash,
                            tool_name,
                            ast_summary,
                            payload_json,
                            storage_kind,
                            base_event_id,
                            raw_bytes,
                            stored_bytes
                        ],
                    )?;
                }
            }

            for (session_id, closed_at) in closed_by_session {
                if let Some(closed_at) = closed_at {
                    let seq = seq_by_session.entry(session_id).or_insert(0);
                    *seq += 1;
                    conn.execute(
                        "INSERT INTO timeline_events (
                            project_id, session_id, seq_in_session, event_kind, timestamp, actor_family, actor_name, summary
                         )
                         SELECT project_id, id, ?2, 'session_closed', ?3, agent_family, agent_name, ''
                         FROM sessions
                         WHERE id=?1",
                        params![session_id, *seq, closed_at],
                    )?;
                }
            }
        }

        conn.execute_batch("DROP TABLE IF EXISTS changes_v10; DROP TABLE IF EXISTS sessions_v10;")?;
        set_schema_version(conn, SCHEMA_VERSION);
        Ok(())
    })();

    if result.is_err() {
        let _ = conn.execute_batch("ROLLBACK");
        return result;
    }

    conn.execute_batch("COMMIT")?;
    Ok(())
}
