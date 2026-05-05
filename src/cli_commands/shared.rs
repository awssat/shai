use crate::storage;
use std::path::PathBuf;

pub fn find_shai_dir(cwd: &str) -> Option<PathBuf> {
    let mut path = PathBuf::from(cwd);
    loop {
        let candidate = path.join(".shai");
        if candidate.is_dir() { return Some(candidate); }
        if !path.pop() { return None; }
    }
}

pub fn load_query(shai_dir: &std::path::Path) -> String {
    let p = shai_dir.parent().unwrap_or(shai_dir).join("queries/rust.scm");
    std::fs::read_to_string(p).unwrap_or_else(|_| include_str!("../../queries/rust.scm").to_string())
}

pub(crate) fn local_shai_or_die() -> (PathBuf, storage::Storage) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).to_string_lossy().to_string();
    let dir = find_shai_dir(&cwd).unwrap_or_else(|| {
        let path = std::path::Path::new(&cwd).join(".shai");
        let _ = std::fs::create_dir_all(&path);
        path
    });
    let db = storage::Storage::open(&dir);
    db.init_schema();
    (dir, db)
}

pub(crate) fn truncate_path(path: &str, max: usize) -> String {
    if path.len() <= max { return path.to_string(); }
    let parts: Vec<&str> = path.split(std::path::MAIN_SEPARATOR).collect();
    if parts.len() <= 2 { return path[..max].to_string(); }
    format!("…{}{}{}{}", std::path::MAIN_SEPARATOR, parts[parts.len() - 2], std::path::MAIN_SEPARATOR, parts[parts.len() - 1])
}

pub(crate) fn parse_search_mode(mode: &str) -> storage::SearchMode {
    match mode.trim().to_ascii_lowercase().as_str() {
        "prompt" => storage::SearchMode::Prompt,
        "summary" => storage::SearchMode::Summary,
        "path" => storage::SearchMode::Path,
        _ => storage::SearchMode::All,
    }
}

pub fn get_current_git_branch(cwd: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            return Some(branch);
        }
    }
    None
}
