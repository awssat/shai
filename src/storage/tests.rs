#[cfg(test)]
mod tests {
    use crate::storage::Storage;
    use tempfile::tempdir;

    #[test]
    fn test_open_session_and_record_change() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        storage.init_schema();
        
        let session_key = "test-session";
        let prompt = "Initial prompt";
        let llm = "claude";
        
        storage.open_session(session_key, prompt, llm, None);
        
        let file_path = "src/main.rs";
        let content = b"fn main() {}";
        let tool_name = "Write";
        let query = "";
        
        storage.record_change(session_key, llm, file_path, content, tool_name, query, None);
        
        let history = storage.get_history(10);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].prompt, prompt);
        assert_eq!(history[0].changes.len(), 1);
        assert_eq!(history[0].changes[0].file_path, file_path);
    }

    #[test]
    fn test_project_id_stability() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        let id1 = storage.get_project_id();
        let id2 = storage.get_project_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_gc_logic() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        storage.init_schema();
        let result = storage.gc(30, true, false);
        assert_eq!(result.blob_count, 0);
    }
}
