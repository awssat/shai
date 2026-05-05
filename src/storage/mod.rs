use std::path::PathBuf;

mod analytics;
mod blobs;
mod content_store;
mod export_import;
mod helpers;
mod history;
mod schema;
mod search;
mod sessions;
#[cfg(test)]
mod tests;
mod types;

use self::content_store::RedbContentStore;
use self::types::open_wal_connection;
pub use self::types::*;

pub(super) const UNRECORDED_PROMPT: &str = "(unrecorded prompt)";

impl Storage {
    pub fn open(shai_dir: &std::path::Path) -> Self {
        std::fs::create_dir_all(shai_dir).ok();
        Self {
            shai_dir: shai_dir.to_path_buf(),
            project_id_cache: std::sync::Mutex::new(None),
            content_store: RedbContentStore::new(shai_dir),
        }
    }

    fn conn(&self) -> StorageConn<'_> {
        StorageConn::Owned(open_wal_connection(&self.shai_dir))
    }

    fn content_store(&self) -> &RedbContentStore {
        &self.content_store
    }

    pub fn project_root(&self) -> PathBuf {
        self.shai_dir.parent().unwrap_or(&self.shai_dir).to_path_buf()
    }

    pub fn get_project_id(&self) -> String {
        let mut cache = self.project_id_cache.lock().unwrap();
        if let Some(id) = cache.as_ref() {
            return id.clone();
        }

        let project_id_path = self.shai_dir.join("project_id");
        let id = if let Ok(existing) = std::fs::read_to_string(&project_id_path) {
            let trimmed = existing.trim();
            if trimmed.is_empty() {
                self.generate_and_save_project_id(&project_id_path)
            } else {
                trimmed.to_string()
            }
        } else {
            self.generate_and_save_project_id(&project_id_path)
        };

        *cache = Some(id.clone());
        id
    }

    pub fn project_id(&self) -> String {
        self.get_project_id()
    }

    fn generate_and_save_project_id(&self, path: &std::path::Path) -> String {
        let id = format!("project_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let _ = std::fs::write(path, &id);
        id
    }
}
