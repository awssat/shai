use super::content_store::ContentStore;
use super::{
    ExportEventRecord, ExportRecord, ExportStats, ImportStats, MemoryDecisionRecord,
    MemoryFactRecord, Storage, TimelineEventRecord,
};
use base64::Engine;
use std::collections::HashSet;
use std::io::{BufRead, Write};

impl Storage {
    pub fn export_to<W: Write>(&self, mut writer: W) -> Result<ExportStats, String> {
        let conn = self.conn();
        let project_id = self.project_id();
        let mut seen_sessions = HashSet::new();
        let mut events = 0usize;
        let memory_facts = self.list_memory_facts(1_000);
        let memory_decisions = self.list_memory_decisions(1_000);
        let memory_record_count = memory_facts.len() + memory_decisions.len();

        let mut stmt = conn
            .prepare(
                "SELECT s.session_key, s.llm, s.started_at, s.closed_at,
                        e.id, e.project_id, e.session_id, e.seq_in_session, e.event_kind, e.timestamp,
                        e.actor_family, e.actor_name, e.file_path, e.blob_hash, e.tool_name, e.summary,
                        e.payload_json, e.raw_bytes, e.stored_bytes
                 FROM timeline_events e
                 JOIN sessions s ON s.id = e.session_id
                 WHERE e.project_id=?1
                 ORDER BY e.timestamp ASC, e.id ASC",
            )
            .map_err(|err| err.to_string())?;

        let rows = stmt
            .query_map([project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    TimelineEventRecord {
                        id: row.get(4)?,
                        project_id: row.get(5)?,
                        session_id: row.get(6)?,
                        seq_in_session: row.get(7)?,
                        event_kind: row.get(8)?,
                        timestamp: row.get(9)?,
                        actor_family: row.get(10)?,
                        actor_name: row.get(11)?,
                        file_path: row.get(12)?,
                        blob_hash: row.get(13)?,
                        tool_name: row.get(14)?,
                        summary: row.get(15)?,
                        payload_json: row.get(16)?,
                        raw_bytes: row.get::<_, i64>(17)? as u64,
                        stored_bytes: row.get::<_, i64>(18)? as u64,
                    },
                ))
            })
            .map_err(|err| err.to_string())?;

        for row in rows {
            let (session_key, llm, started_at, closed_at, event) =
                row.map_err(|err| err.to_string())?;
            seen_sessions.insert((session_key.clone(), llm.clone(), started_at.clone()));

            let blob_content_base64 = if event.event_kind == "file_snapshot" {
                match event.blob_hash.as_deref() {
                    Some(hash) if !hash.is_empty() && hash != "[gc-deleted]" => self
                        .get_blob(hash)
                        .ok()
                        .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
                    _ => None,
                }
            } else {
                None
            };

            let export_record = ExportRecord::Event(Box::new(ExportEventRecord {
                session_key,
                llm,
                started_at,
                closed_at,
                event,
                blob_content_base64,
            }));
            serde_json::to_writer(&mut writer, &export_record).map_err(|err| err.to_string())?;
            writer.write_all(b"\n").map_err(|err| err.to_string())?;
            events += 1;
        }

        for fact in memory_facts {
            serde_json::to_writer(&mut writer, &ExportRecord::MemoryFact(fact))
                .map_err(|err| err.to_string())?;
            writer.write_all(b"\n").map_err(|err| err.to_string())?;
        }
        for decision in memory_decisions {
            serde_json::to_writer(&mut writer, &ExportRecord::MemoryDecision(decision))
                .map_err(|err| err.to_string())?;
            writer.write_all(b"\n").map_err(|err| err.to_string())?;
        }
        Ok(ExportStats {
            sessions: seen_sessions.len(),
            events,
            memory_records: memory_record_count,
        })
    }

    pub fn import_from<R: std::io::Read>(&self, reader: R) -> Result<ImportStats, String> {
        let mut conn = self.conn();
        let target_project_id = self.project_id();
        let buf = std::io::BufReader::new(reader);
        let mut sessions_inserted = 0usize;
        let mut events_inserted = 0usize;
        let mut memory_records_inserted = 0usize;

        // Collect all records first so we can wrap all DB writes in one transaction.
        // Blobs are written to the content store before the transaction to keep SQLite
        // and redb concerns separate; a partial blob write is harmless (the event row
        // that references it is only committed once everything succeeds).
        let mut records: Vec<ExportRecord> = Vec::new();
        for line in buf.lines() {
            let line = line.map_err(|err| err.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            let record: ExportRecord =
                serde_json::from_str(&line).map_err(|err| err.to_string())?;
            records.push(record);
        }

        // Write blobs to the content store outside the SQLite transaction.
        for record in &records {
            if let ExportRecord::Event(ev) = record {
                if let (Some(hash), Some(raw_base64)) = (
                    ev.event.blob_hash.as_deref(),
                    ev.blob_content_base64.as_deref(),
                ) {
                    if hash != "[gc-deleted]" {
                        let raw_bytes = base64::engine::general_purpose::STANDARD
                            .decode(raw_base64)
                            .map_err(|err| err.to_string())?;
                        let compressed = zstd::encode_all(raw_bytes.as_slice(), 3)
                            .map_err(|err| err.to_string())?;
                        let _ = self.content_store().put(hash, &compressed);
                    }
                }
            }
        }

        // Write all metadata in one atomic transaction.
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|err| err.to_string())?;

        for record in records {
            match record {
                ExportRecord::Event(record) => {
                    let session_inserted = tx
                        .execute(
                            "INSERT OR IGNORE INTO sessions (
                    session_key, project_id, llm, agent_family, agent_name, started_at, closed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                            rusqlite::params![
                                record.session_key,
                                &target_project_id,
                                record.llm,
                                record.event.actor_family,
                                record.event.actor_name,
                                record.started_at,
                                record.closed_at
                            ],
                        )
                        .map_err(|err| err.to_string())?;
                    sessions_inserted += session_inserted;

                    let session_id: i64 = tx
                        .query_row(
                    "SELECT id FROM sessions WHERE session_key=?1 AND llm=?2 AND started_at=?3 LIMIT 1",
                            rusqlite::params![record.session_key, record.llm, record.started_at],
                            |row| row.get(0),
                        )
                        .map_err(|err| err.to_string())?;

                    let inserted = tx.execute(
                "INSERT OR IGNORE INTO timeline_events (
                    project_id, session_id, seq_in_session, event_kind, timestamp, actor_family, actor_name,
                    file_path, blob_hash, tool_name, summary, payload_json, raw_bytes, stored_bytes
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                        rusqlite::params![
                            &target_project_id,
                            session_id,
                            record.event.seq_in_session,
                            record.event.event_kind,
                            record.event.timestamp,
                            record.event.actor_family,
                            record.event.actor_name,
                            record.event.file_path,
                            record.event.blob_hash,
                            record.event.tool_name,
                            record.event.summary,
                            record.event.payload_json,
                            record.event.raw_bytes as i64,
                            record.event.stored_bytes as i64
                        ],
                    )
                    .map_err(|err| err.to_string())?;

                    events_inserted += inserted;
                }
                ExportRecord::MemoryFact(fact) => {
                    memory_records_inserted += import_memory_fact(&tx, &target_project_id, fact)?;
                }
                ExportRecord::MemoryDecision(decision) => {
                    memory_records_inserted +=
                        import_memory_decision(&tx, &target_project_id, decision)?;
                }
            }
        }

        tx.commit().map_err(|err| err.to_string())?;

        Ok(ImportStats {
            sessions_inserted,
            events_inserted,
            memory_records_inserted,
        })
    }
}

fn import_memory_fact(
    conn: &rusqlite::Connection,
    project_id: &str,
    fact: MemoryFactRecord,
) -> Result<usize, String> {
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO memory_facts (project_id, fact_key, content, verified, source, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            project_id,
            fact.fact_key,
            fact.content,
            fact.verified as i64,
            fact.source,
            fact.created_at
        ],
    )
    .map_err(|err| err.to_string())?;

    let target_id: i64 = conn
        .query_row(
            "SELECT id FROM memory_facts WHERE project_id=?1 AND fact_key=?2 LIMIT 1",
            rusqlite::params![project_id, fact.fact_key],
            |row| row.get(0),
        )
        .map_err(|err| err.to_string())?;
    insert_branch_refs(conn, project_id, "fact", target_id, &fact.branch_refs)?;
    Ok(inserted + fact.branch_refs.len())
}

fn import_memory_decision(
    conn: &rusqlite::Connection,
    project_id: &str,
    decision: MemoryDecisionRecord,
) -> Result<usize, String> {
    let inserted = conn
        .execute(
            "INSERT OR IGNORE INTO memory_decisions (
            project_id, title, rationale, alternatives, status, verified, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                project_id,
                decision.title,
                decision.rationale,
                decision.alternatives,
                decision.status,
                decision.verified as i64,
                decision.created_at
            ],
        )
        .map_err(|err| err.to_string())?;

    let target_id: i64 = conn
        .query_row(
            "SELECT id FROM memory_decisions WHERE project_id=?1 AND title=?2 LIMIT 1",
            rusqlite::params![project_id, decision.title],
            |row| row.get(0),
        )
        .map_err(|err| err.to_string())?;
    insert_branch_refs(
        conn,
        project_id,
        "decision",
        target_id,
        &decision.branch_refs,
    )?;
    Ok(inserted + decision.branch_refs.len())
}

fn insert_branch_refs(
    conn: &rusqlite::Connection,
    project_id: &str,
    target_kind: &str,
    target_id: i64,
    branch_refs: &[String],
) -> Result<(), String> {
    for branch in branch_refs {
        conn.execute(
            "INSERT INTO memory_refs (project_id, ref_kind, ref_value, target_kind, target_id)
             VALUES (?1, 'branch', ?2, ?3, ?4)",
            rusqlite::params![project_id, branch, target_kind, target_id],
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}
