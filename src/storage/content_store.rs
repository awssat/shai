use redb::{Database, ReadableDatabase, TableDefinition};
use std::path::PathBuf;
use std::sync::OnceLock;

const BLOBS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("code_blobs");

pub(super) trait ContentStore {
    fn put(&self, hash: &str, compressed: &[u8]) -> Result<(), String>;
    fn get(&self, hash: &str) -> Option<Vec<u8>>;
    fn get_archive(&self, hash: &str) -> Option<Vec<u8>>;
    fn remove(&self, hash: &str);
    fn compressed_size(&self, hash: &str) -> Option<u64>;
    fn archive(&self, hash: &str) -> bool;
}

pub(super) struct RedbContentStore {
    primary_path: PathBuf,
    archive_path: PathBuf,
    primary_db: OnceLock<Option<Database>>,
}

impl RedbContentStore {
    pub(super) fn new(shai_dir: &std::path::Path) -> Self {
        Self {
            primary_path: shai_dir.join("blobs.redb"),
            archive_path: shai_dir.join("blobs_archive.redb"),
            primary_db: OnceLock::new(),
        }
    }

    fn primary_db(&self) -> Option<&Database> {
        self.primary_db.get_or_init(|| {
            match Database::create(&self.primary_path) {
                Ok(db) => Some(db),
                Err(err) => {
                    tracing::error!(
                        "shai: failed to open redb '{}': {}",
                        self.primary_path.display(),
                        err
                    );
                    None
                }
            }
        }).as_ref()
    }

    fn open_db(path: &std::path::Path) -> Option<Database> {
        Database::open(path).ok()
    }

    fn load_from(path: &std::path::Path, hash: &str) -> Option<Vec<u8>> {
        let db = Self::open_db(path)?;
        let txn = db.begin_read().ok()?;
        let table = txn.open_table(BLOBS_TABLE).ok()?;
        let blob = table.get(hash).ok()??;
        zstd::decode_all(blob.value()).ok()
    }
}

impl ContentStore for RedbContentStore {
    fn put(&self, hash: &str, compressed: &[u8]) -> Result<(), String> {
        let db = self
            .primary_db()
            .ok_or_else(|| format!("failed to open redb '{}'", self.primary_path.display()))?;
        let wtxn = db.begin_write().map_err(|e| {
            format!(
                "redb begin_write failed for '{}': {}",
                self.primary_path.display(),
                e
            )
        })?;
        {
            let mut table = wtxn.open_table(BLOBS_TABLE).map_err(|e| {
                format!(
                    "redb open_table failed for '{}': {}",
                    self.primary_path.display(),
                    e
                )
            })?;
            table
                .insert(hash, compressed)
                .map_err(|e| format!("redb insert failed for key '{}': {}", hash, e))?;
        }
        wtxn.commit().map_err(|e| {
            format!(
                "redb commit failed for '{}': {}",
                self.primary_path.display(),
                e
            )
        })?;
        Ok(())
    }

    fn get(&self, hash: &str) -> Option<Vec<u8>> {
        let db = self.primary_db()?;
        let txn = db.begin_read().ok()?;
        let table = txn.open_table(BLOBS_TABLE).ok()?;
        let blob = table.get(hash).ok()??;
        zstd::decode_all(blob.value()).ok()
    }

    fn get_archive(&self, hash: &str) -> Option<Vec<u8>> {
        Self::load_from(&self.archive_path, hash)
    }

    fn remove(&self, hash: &str) {
        let Some(db) = self.primary_db() else {
            return;
        };
        let Ok(wtxn) = db.begin_write() else {
            tracing::error!(
                "shai: redb begin_write failed for '{}'",
                self.primary_path.display()
            );
            return;
        };
        {
            let Ok(mut table) = wtxn.open_table(BLOBS_TABLE) else {
                tracing::error!(
                    "shai: redb open_table failed for '{}'",
                    self.primary_path.display()
                );
                return;
            };
            table.remove(hash).ok();
        }
        wtxn.commit().ok();
    }

    fn compressed_size(&self, hash: &str) -> Option<u64> {
        let db = self.primary_db()?;
        let txn = db.begin_read().ok()?;
        let table = txn.open_table(BLOBS_TABLE).ok()?;
        let blob = table.get(hash).ok()??;
        Some(blob.value().len() as u64)
    }

    fn archive(&self, hash: &str) -> bool {
        let Some(primary) = self.primary_db() else {
            return false;
        };
        let Some(archive) = Database::create(&self.archive_path).ok() else {
            return false;
        };

        let Ok(rtxn) = primary.begin_read() else {
            tracing::error!(
                "shai: redb begin_read failed for '{}'",
                self.primary_path.display()
            );
            return false;
        };
        let bytes = {
            let table = rtxn.open_table(BLOBS_TABLE).ok();
            table.and_then(|table| {
                table
                    .get(hash)
                    .ok()
                    .flatten()
                    .map(|blob| blob.value().to_vec())
            })
        };
        drop(rtxn);
        let Some(bytes) = bytes else { return false };

        let Ok(wtxn) = archive.begin_write() else {
            tracing::error!(
                "shai: redb begin_write failed for '{}'",
                self.archive_path.display()
            );
            return false;
        };
        {
            let Ok(mut table) = wtxn.open_table(BLOBS_TABLE) else {
                tracing::error!(
                    "shai: redb open_table failed for '{}'",
                    self.archive_path.display()
                );
                return false;
            };
            table.insert(hash, bytes.as_slice()).ok();
        }
        wtxn.commit().ok();
        self.remove(hash);
        true
    }
}
