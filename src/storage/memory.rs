use super::{MemoryDecisionRecord, MemoryFactRecord, MemoryVerifyOutcome, Storage};
use rusqlite::params;

impl Storage {
    pub fn add_memory_fact(
        &self,
        fact_key: &str,
        content: &str,
        verified: bool,
        source: &str,
        branch: Option<&str>,
    ) -> Result<i64, String> {
        let conn = self.conn();
        let project_id = self.project_id();
        conn.execute(
            "INSERT INTO memory_facts (project_id, fact_key, content, verified, source)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id, fact_key, content, verified as i64, source],
        )
        .map_err(|err| err.to_string())?;
        let id = conn.last_insert_rowid();
        if let Some(branch) = branch {
            self.add_memory_ref("branch", branch, "fact", id)?;
        }
        Ok(id)
    }

    pub fn add_memory_decision(
        &self,
        title: &str,
        rationale: &str,
        alternatives: &str,
        status: &str,
        verified: bool,
        branch: Option<&str>,
    ) -> Result<i64, String> {
        let conn = self.conn();
        let project_id = self.project_id();
        conn.execute(
            "INSERT INTO memory_decisions (project_id, title, rationale, alternatives, status, verified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                project_id,
                title,
                rationale,
                alternatives,
                status,
                verified as i64
            ],
        )
        .map_err(|err| err.to_string())?;
        let id = conn.last_insert_rowid();
        if let Some(branch) = branch {
            self.add_memory_ref("branch", branch, "decision", id)?;
        }
        Ok(id)
    }

    pub fn add_memory_ref(
        &self,
        ref_kind: &str,
        ref_value: &str,
        target_kind: &str,
        target_id: i64,
    ) -> Result<i64, String> {
        let conn = self.conn();
        let project_id = self.project_id();
        conn.execute(
            "INSERT INTO memory_refs (project_id, ref_kind, ref_value, target_kind, target_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id, ref_kind, ref_value, target_kind, target_id],
        )
        .map_err(|err| err.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn verify_memory_fact(&self, id: i64) -> Result<MemoryVerifyOutcome, String> {
        let conn = self.conn();
        let project_id = self.project_id();
        let mut stmt = conn
            .prepare("SELECT verified FROM memory_facts WHERE project_id=?1 AND id=?2")
            .map_err(|err| err.to_string())?;
        let status = stmt
            .query_row(params![project_id, id], |row| row.get::<_, i64>(0))
            .map(|value| value != 0);
        match status {
            Ok(true) => Ok(MemoryVerifyOutcome::AlreadyVerified),
            Ok(false) => {
                conn.execute(
                    "UPDATE memory_facts SET verified=1 WHERE project_id=?1 AND id=?2",
                    params![project_id, id],
                )
                .map_err(|err| err.to_string())?;
                Ok(MemoryVerifyOutcome::Verified)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(MemoryVerifyOutcome::NotFound),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn verify_memory_decision(&self, id: i64) -> Result<MemoryVerifyOutcome, String> {
        let conn = self.conn();
        let project_id = self.project_id();
        let mut stmt = conn
            .prepare("SELECT verified FROM memory_decisions WHERE project_id=?1 AND id=?2")
            .map_err(|err| err.to_string())?;
        let status = stmt
            .query_row(params![project_id, id], |row| row.get::<_, i64>(0))
            .map(|value| value != 0);
        match status {
            Ok(true) => Ok(MemoryVerifyOutcome::AlreadyVerified),
            Ok(false) => {
                conn.execute(
                    "UPDATE memory_decisions SET verified=1 WHERE project_id=?1 AND id=?2",
                    params![project_id, id],
                )
                .map_err(|err| err.to_string())?;
                Ok(MemoryVerifyOutcome::Verified)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(MemoryVerifyOutcome::NotFound),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn list_memory_facts(&self, limit: u32) -> Vec<MemoryFactRecord> {
        let conn = self.conn();
        let project_id = self.project_id();
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, fact_key, content, verified, source, created_at
                 FROM memory_facts
                 WHERE project_id=?1
                 ORDER BY verified DESC, created_at DESC, id DESC
                 LIMIT ?2",
            )
            .unwrap();
        stmt.query_map(params![project_id, limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)? != 0,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .unwrap()
        .filter_map(Result::ok)
        .map(|row| MemoryFactRecord {
            id: row.0,
            project_id: row.1,
            fact_key: row.2,
            content: row.3,
            verified: row.4,
            source: row.5,
            created_at: row.6,
            branch_refs: self.branch_refs_for("fact", row.0),
        })
        .collect()
    }

    pub fn list_memory_decisions(&self, limit: u32) -> Vec<MemoryDecisionRecord> {
        let conn = self.conn();
        let project_id = self.project_id();
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, title, rationale, alternatives, status, verified, created_at
                 FROM memory_decisions
                 WHERE project_id=?1
                 ORDER BY verified DESC, created_at DESC, id DESC
                 LIMIT ?2",
            )
            .unwrap();
        stmt.query_map(params![project_id, limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)? != 0,
                row.get::<_, String>(7)?,
            ))
        })
        .unwrap()
        .filter_map(Result::ok)
        .map(|row| MemoryDecisionRecord {
            id: row.0,
            project_id: row.1,
            title: row.2,
            rationale: row.3,
            alternatives: row.4,
            status: row.5,
            verified: row.6,
            created_at: row.7,
            branch_refs: self.branch_refs_for("decision", row.0),
        })
        .collect()
    }

    pub fn ranked_memory_summary(&self, branch: Option<&str>, limit: u32) -> Vec<String> {
        let conn = self.conn();
        let project_id = self.project_id();
        let branch = branch.unwrap_or_default().to_string();
        let mut out = Vec::new();

        let fact_sql = "
            SELECT f.fact_key, f.content, f.verified,
                   EXISTS(
                       SELECT 1 FROM memory_refs r
                       WHERE r.project_id=f.project_id
                         AND r.target_kind='fact'
                         AND r.target_id=f.id
                         AND r.ref_kind='branch'
                         AND r.ref_value=?2
                   ) AS branch_match
            FROM memory_facts f
            WHERE f.project_id=?1
            ORDER BY branch_match DESC, f.verified DESC, f.created_at DESC, f.id DESC
            LIMIT ?3";
        let mut fact_stmt = conn.prepare(fact_sql).unwrap();
        for row in fact_stmt
            .query_map(params![project_id, branch, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                    row.get::<_, i64>(3)? != 0,
                ))
            })
            .unwrap()
            .filter_map(Result::ok)
        {
            let prefix = if row.2 { "verified fact" } else { "fact" };
            let scoped = if row.3 { " [branch]" } else { "" };
            out.push(format!("{}{}: {} = {}", prefix, scoped, row.0, row.1));
        }

        let remaining = limit.saturating_sub(out.len() as u32);
        if remaining == 0 {
            return out;
        }

        let decision_sql = "
            SELECT d.title, d.rationale, d.status, d.verified,
                   EXISTS(
                       SELECT 1 FROM memory_refs r
                       WHERE r.project_id=d.project_id
                         AND r.target_kind='decision'
                         AND r.target_id=d.id
                         AND r.ref_kind='branch'
                         AND r.ref_value=?2
                   ) AS branch_match
            FROM memory_decisions d
            WHERE d.project_id=?1
            ORDER BY branch_match DESC, d.verified DESC, d.created_at DESC, d.id DESC
            LIMIT ?3";
        let mut decision_stmt = conn.prepare(decision_sql).unwrap();
        for row in decision_stmt
            .query_map(params![project_id, branch, remaining], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                    row.get::<_, i64>(4)? != 0,
                ))
            })
            .unwrap()
            .filter_map(Result::ok)
        {
            let prefix = if row.3 {
                "verified decision"
            } else {
                "decision"
            };
            let scoped = if row.4 { " [branch]" } else { "" };
            let rationale = if row.1.trim().is_empty() {
                "".to_string()
            } else {
                format!(" — {}", row.1)
            };
            out.push(format!(
                "{}{}: {} ({}){}",
                prefix, scoped, row.0, row.2, rationale
            ));
        }

        out
    }

    fn branch_refs_for(&self, target_kind: &str, target_id: i64) -> Vec<String> {
        let conn = self.conn();
        let project_id = self.project_id();
        let mut stmt = conn
            .prepare(
                "SELECT ref_value
                 FROM memory_refs
                 WHERE project_id=?1 AND target_kind=?2 AND target_id=?3 AND ref_kind='branch'
                 ORDER BY id ASC",
            )
            .unwrap();
        stmt.query_map(params![project_id, target_kind, target_id], |row| {
            row.get(0)
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
    }
}
