use super::Storage;
use std::io::{BufRead, Write};
use serde_json::Value;

pub struct ExportStats {
    pub sessions: usize,
    pub changes: usize,
}

pub struct ImportStats {
    pub sessions_inserted: usize,
}

impl Storage {
    pub fn export_to<W: Write>(&self, mut writer: W) -> Result<ExportStats, String> {
        let history = self.get_history(1000);
        let mut sessions_count = 0;
        let mut changes_count = 0;

        for session in history {
            let json = serde_json::to_string(&session).unwrap();
            writer.write_all(json.as_bytes()).unwrap();
            writer.write_all(b"\n").unwrap();
            sessions_count += 1;
            changes_count += session.changes.len();
        }

        Ok(ExportStats {
            sessions: sessions_count,
            changes: changes_count,
        })
    }

    pub fn import_from<R: std::io::Read>(&self, reader: R) -> Result<ImportStats, String> {
        let buf = std::io::BufReader::new(reader);
        let mut sessions_inserted = 0;

        for line in buf.lines() {
            let line = line.map_err(|e| e.to_string())?;
            if let Ok(_session) = serde_json::from_str::<Value>(&line) {
                sessions_inserted += 1;
            }
        }

        Ok(ImportStats {
            sessions_inserted,
        })
    }
}
