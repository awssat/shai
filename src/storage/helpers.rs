pub(super) fn agent_identity(llm: &str) -> (String, String) {
    match llm {
        "claude" => ("anthropic".to_string(), "claude-code".to_string()),
        "gemini" => ("google".to_string(), "gemini-cli".to_string()),
        "copilot" => ("github".to_string(), "copilot-cli".to_string()),
        _ => ("generic".to_string(), llm.to_string()),
    }
}

pub(super) fn normalize_tool_name(tool: &str) -> String {
    let t = tool.to_lowercase();
    if t.contains("write") || t.contains("edit") || t.contains("patch") {
        "Write".to_string()
    } else if t.contains("shell") || t.contains("bash") || t.contains("exec") || t.contains("run") {
        "Shell".to_string()
    } else {
        tool.to_string()
    }
}

pub(super) fn normalize_file_path(project_root: &std::path::Path, raw_file_path: &str) -> String {
    let path = std::path::Path::new(raw_file_path);
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    let mut normalized_path = absolute_path
        .strip_prefix(project_root)
        .unwrap_or(&absolute_path)
        .to_string_lossy()
        .to_string();
    if normalized_path.starts_with("./") {
        normalized_path = normalized_path[2..].to_string();
    }
    normalized_path
}

pub(super) fn should_track(path: &str) -> bool {
    !path.contains(".shai") && !path.contains(".git/") && !path.contains("node_modules/")
}

pub(super) fn apply_bytes_delta(_old: &[u8], delta: &[u8]) -> Option<Vec<u8>> {
    zstd::decode_all(delta).ok()
}
