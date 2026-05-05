use super::content_store::ContentStore;
use super::helpers::apply_bytes_delta;
use super::{BlobLoadError, GcResult, Storage};
use std::collections::{HashMap, HashSet};

impl Storage {
    pub(super) fn storage_entry_key(
        &self,
        change_id: i64,
        storage_kind: &str,
        blob_hash: &str,
    ) -> String {
        match storage_kind {
            "delta" => format!("delta:{}", change_id),
            _ => blob_hash.to_string(),
        }
    }

    pub(super) fn load_change_bytes(&self, change_id: i64) -> Result<Vec<u8>, BlobLoadError> {
        let conn = self.conn();
        let (blob_hash, storage_kind, base_change_id): (String, String, Option<i64>) = conn
            .query_row(
                "SELECT blob_hash, storage_kind, base_change_id
                 FROM changes
                 WHERE id=?1",
                [change_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| BlobLoadError::Missing)?;

        if blob_hash == "[gc-deleted]" {
            return Err(BlobLoadError::Missing);
        }

        let store = self.content_store();
        let key = self.storage_entry_key(change_id, &storage_kind, &blob_hash);
        let payload = store
            .get(&key)
            .or_else(|| store.get_archive(&key))
            .ok_or(BlobLoadError::Missing)?;

        let bytes = if storage_kind == "delta" {
            let base_id = base_change_id.ok_or(BlobLoadError::Missing)?;
            let base = self.load_change_bytes(base_id)?;
            apply_bytes_delta(&base, &payload)
                .ok_or_else(|| BlobLoadError::DeltaCorrupted("failed to apply delta".to_string()))?
        } else {
            payload
        };

        Ok(bytes)
    }

    pub fn gc(&self, days: u32, delete: bool, dry_run: bool) -> GcResult {
        let conn = self.conn();
        let cutoff = format!("datetime('now', '-{} days')", days);
        let rows: Vec<(i64, String, String, Option<i64>, bool)> = conn.prepare(&format!(
            "SELECT id, blob_hash, storage_kind, base_change_id, timestamp < {} AS is_expired
                  FROM changes
                  WHERE blob_hash != '[gc-deleted]'",
            cutoff
        )).unwrap().query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get::<_, i64>(4)? != 0,
            ))
        }).unwrap().filter_map(|r| r.ok()).collect();

        let mut by_id = HashMap::new();
        for (id, blob_hash, storage_kind, base_change_id, is_expired) in &rows {
            by_id.insert(*id, (blob_hash.clone(), storage_kind.clone(), *base_change_id, *is_expired));
        }

        let mut protected_ids = HashSet::new();
        for (id, _, _, _, is_expired) in &rows {
            if !is_expired {
                let mut current = Some(*id);
                while let Some(change_id) = current {
                    if !protected_ids.insert(change_id) { break; }
                    current = by_id.get(&change_id).and_then(|(_, _, base_change_id, _)| {
                        match base_change_id { Some(next) if *next != change_id => Some(*next), _ => None }
                    });
                }
            }
        }

        let candidates: Vec<(i64, String, String)> = rows.iter()
            .filter(|(id, _, _, _, is_expired)| *is_expired && !protected_ids.contains(id))
            .map(|(id, blob_hash, storage_kind, _, _)| (*id, blob_hash.clone(), storage_kind.clone()))
            .collect();

        if candidates.is_empty() || dry_run { return GcResult { blob_count: candidates.len(), bytes_freed: 0 }; }

        let store = self.content_store();
        let mut moved = 0;
        let mut freed = 0;
        for (change_id, blob_hash, storage_kind) in &candidates {
            let key = self.storage_entry_key(*change_id, &storage_kind, &blob_hash);
            if let Some(size) = store.compressed_size(&key) {
                if delete { store.remove(&key); } else { store.archive(&key); }
                freed += size; moved += 1;
            }
        }

        if delete {
            for (id, _, _) in &candidates {
                let _ = conn.execute("UPDATE changes SET blob_hash='[gc-deleted]' WHERE id=?1", [id]);
            }
        }

        GcResult { blob_count: moved, bytes_freed: freed }
    }

    pub fn get_blob(&self, hash: &str) -> Result<Vec<u8>, BlobLoadError> {
        if hash == "[gc-deleted]" { return Err(BlobLoadError::Missing); }
        let store = self.content_store();
        if let Some(bytes) = store.get(hash).or_else(|| store.get_archive(hash)) { return Ok(bytes); }

        let conn = self.conn();
        let change_id: i64 = conn.query_row(
                "SELECT id FROM changes WHERE blob_hash=?1 ORDER BY timestamp DESC LIMIT 1",
                [hash], |row| row.get(0)).map_err(|_| BlobLoadError::Missing)?;
        self.load_change_bytes(change_id)
    }
}
