pub(super) fn agent_identity(llm: &str) -> (String, String) {
    match llm {
        "claude" => ("anthropic".to_string(), "claude-code".to_string()),
        "gemini" => ("google".to_string(), "gemini-cli".to_string()),
        "ghcs" | "copilot" => ("github".to_string(), "copilot-cli".to_string()),
        _ => ("generic".to_string(), llm.to_string()),
    }
}

pub(super) fn normalize_tool_name(tool: &str) -> String {
    let t = tool.to_lowercase();
    if t.contains("write") || t.contains("edit") || t.contains("patch") {
        "Write".to_string()
    } else {
        tool.to_string()
    }
}

pub(super) fn should_track(path: &str) -> bool {
    !path.contains(".shai") && !path.contains(".git/") && !path.contains("node_modules/")
}

pub(super) fn apply_bytes_delta(_old: &[u8], delta: &[u8]) -> Option<Vec<u8>> {
    zstd::decode_all(delta).ok()
}
