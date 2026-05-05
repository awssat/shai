#[cfg(test)]
mod tests {
    use crate::adapters::adapter_for;
    use serde_json::json;

    #[test]
    fn test_gemini_adapter() {
        let adapter = adapter_for("gemini");
        let payload = json!({
            "tool_name": "fs_write",
            "tool_input": { "path": "test.txt" }
        });
        assert_eq!(adapter.tool_name(&payload), Some("fs_write".to_string()));
        assert_eq!(adapter.file_path("fs_write", &payload), Some("test.txt".to_string()));
    }
}
