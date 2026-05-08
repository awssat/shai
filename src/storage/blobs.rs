use super::content_store::ContentStore;
use super::helpers::apply_bytes_delta;
use super::{BlobLoadError, GcResult, Storage};
use std::collections::{HashMap, HashSet};

impl Storage {
    pub(super) fn storage_entry_key(
        &self,
        event_id: i64,
        storage_kind: &str,
        blob_hash: &str,
    ) -> String {
        match storage_kind {
            "delta" => format!("delta:{}", event_id),
            _ => blob_hash.to_string(),
        }
    }

    pub(super) fn load_event_bytes(&self, event_id: i64) -> Result<Vec<u8>, BlobLoadError> {
        self.load_event_bytes_inner(event_id, 0)
    }

    fn load_event_bytes_inner(&self, event_id: i64, depth: u32) -> Result<Vec<u8>, BlobLoadError> {
        const MAX_DELTA_DEPTH: u32 = 200;
        if depth > MAX_DELTA_DEPTH {
            return Err(BlobLoadError::DeltaCorrupted(format!(
                "delta chain depth exceeded {} — possible cycle or corrupt data",
                MAX_DELTA_DEPTH
            )));
        }

        let conn = self.conn();
        let (blob_hash, storage_kind, base_event_id): (String, String, Option<i64>) = conn
            .query_row(
                "SELECT blob_hash, storage_kind, base_event_id
                 FROM timeline_events
                 WHERE id=?1",
                [event_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| BlobLoadError::Missing)?;

        if blob_hash == "[gc-deleted]" {
            return Err(BlobLoadError::Missing);
        }

        let store = self.content_store();
        let key = self.storage_entry_key(event_id, &storage_kind, &blob_hash);
        let payload = store
            .get(&key)
            .or_else(|| store.get_archive(&key))
            .ok_or(BlobLoadError::Missing)?;

        let bytes = if storage_kind == "delta" {
            let base_id = base_event_id.ok_or(BlobLoadError::Missing)?;
            let base = self.load_event_bytes_inner(base_id, depth + 1)?;
            apply_bytes_delta(&base, &payload)
                .ok_or_else(|| BlobLoadError::DeltaCorrupted("failed to apply delta".to_string()))?
        } else {
            payload
        };

        Ok(bytes)
    }

    pub fn gc(&self, days: u32, delete: bool, dry_run: bool) -> GcResult {
        let conn = self.conn();
        let cutoff_interval = format!("-{} days", days);
        let rows: Vec<(i64, String, String, Option<i64>, bool)> = conn
            .prepare(
                "SELECT id, blob_hash, storage_kind, base_event_id,
                        timestamp < datetime('now', ?1) AS is_expired
                 FROM timeline_events
                 WHERE event_kind='file_snapshot' AND blob_hash IS NOT NULL AND blob_hash != '[gc-deleted]'",
            )
            .unwrap()
            .query_map([&cutoff_interval], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get::<_, i64>(4)? != 0,
                ))
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let mut by_id = HashMap::new();
        for (id, blob_hash, storage_kind, base_event_id, is_expired) in &rows {
            by_id.insert(
                *id,
                (
                    blob_hash.clone(),
                    storage_kind.clone(),
                    *base_event_id,
                    *is_expired,
                ),
            );
        }

        let mut protected_ids = HashSet::new();
        for (id, _, _, _, is_expired) in &rows {
            if !is_expired {
                let mut current = Some(*id);
                while let Some(event_id) = current {
                    if !protected_ids.insert(event_id) {
                        break;
                    }
                    current =
                        by_id.get(&event_id).and_then(
                            |(_, _, base_event_id, _)| match base_event_id {
                                Some(next) if *next != event_id => Some(*next),
                                _ => None,
                            },
                        );
                }
            }
        }

        let candidates: Vec<(i64, String, String)> = rows
            .iter()
            .filter(|(id, _, _, _, is_expired)| *is_expired && !protected_ids.contains(id))
            .map(|(id, blob_hash, storage_kind, _, _)| {
                (*id, blob_hash.clone(), storage_kind.clone())
            })
            .collect();

        if candidates.is_empty() || dry_run {
            return GcResult {
                blob_count: candidates.len(),
                bytes_freed: 0,
            };
        }

        let store = self.content_store();
        let mut moved = 0;
        let mut freed = 0;
        for (event_id, blob_hash, storage_kind) in &candidates {
            let key = self.storage_entry_key(*event_id, storage_kind, blob_hash);
            if let Some(size) = store.compressed_size(&key) {
                if delete {
                    store.remove(&key);
                } else {
                    store.archive(&key);
                }
                freed += size;
                moved += 1;
            }
        }

        if delete {
            for (id, _, _) in &candidates {
                let _ = conn.execute(
                    "UPDATE timeline_events SET blob_hash='[gc-deleted]' WHERE id=?1",
                    [id],
                );
            }
        }

        GcResult {
            blob_count: moved,
            bytes_freed: freed,
        }
    }

    pub fn get_blob(&self, hash: &str) -> Result<Vec<u8>, BlobLoadError> {
        if hash == "[gc-deleted]" {
            return Err(BlobLoadError::Missing);
        }
        let store = self.content_store();
        if let Some(bytes) = store.get(hash).or_else(|| store.get_archive(hash)) {
            return Ok(bytes);
        }

        let conn = self.conn();
        let event_id: i64 = conn
            .query_row(
                "SELECT id
                 FROM timeline_events
                 WHERE event_kind='file_snapshot' AND blob_hash=?1
                 ORDER BY timestamp DESC LIMIT 1",
                [hash],
                |row| row.get(0),
            )
            .map_err(|_| BlobLoadError::Missing)?;
        self.load_event_bytes(event_id)
    }
}
