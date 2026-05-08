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
    // Canonicalize to resolve symlinks so strip_prefix works correctly.
    // Fall back to the unresolved path if the file doesn't exist yet.
    let canonical_path = absolute_path.canonicalize().unwrap_or_else(|_| absolute_path.clone());
    let canonical_root = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf());
    let mut normalized_path = canonical_path
        .strip_prefix(&canonical_root)
        .unwrap_or(&canonical_path)
        .to_string_lossy()
        .to_string();
    if normalized_path.starts_with("./") {
        normalized_path = normalized_path[2..].to_string();
    }
    normalized_path
}

pub(super) fn should_track(path: &str, gitignore: &ignore::gitignore::Gitignore) -> bool {
    // Always exclude shai internals and git internals regardless of .gitignore
    if path.contains(".shai") || path.contains("/.git/") || path.ends_with("/.git") {
        return false;
    }
    // Honour .gitignore rules — covers build dirs, vendor, secrets, etc. for any project type
    !gitignore
        .matched_path_or_any_parents(std::path::Path::new(path), false)
        .is_ignore()
}

pub(super) fn build_gitignore(project_root: &std::path::Path) -> ignore::gitignore::Gitignore {
    let mut builder = ignore::gitignore::GitignoreBuilder::new(project_root);
    builder.add(project_root.join(".gitignore"));
    builder.add(project_root.join(".git").join("info").join("exclude"));
    builder.build().unwrap_or_else(|_| ignore::gitignore::Gitignore::empty())
}

pub(super) fn apply_bytes_delta(_old: &[u8], delta: &[u8]) -> Option<Vec<u8>> {
    zstd::decode_all(delta).ok()
}
