use super::types::ContextProfile;
use crate::storage::SessionRecord;
use std::path::Path;

pub fn project_summary_report(
    repo_root: &Path,
    history: &[SessionRecord],
    _profile: ContextProfile,
) -> String {
    let mut out = String::from("Project History Summary:\n");

    if let Some(branch) = crate::cli_commands::shared::get_current_git_branch(repo_root) {
        out.push_str(&format!("Current Branch: {}\n", branch));
    }
    out.push('\n');

    if history.is_empty() {
        out.push_str("No recent activity recorded.");
        return out;
    }
    for session in history.iter().take(5) {
        out.push_str(&format!(
            "- [{}] \"{}\"\n",
            session.started_at, session.prompt
        ));
        for change in session.changes.iter().take(3) {
            out.push_str(&format!(
                "  ↳ {} — {}\n",
                change.file_path, change.ast_summary
            ));
        }
    }
    out
}

pub fn why_file_report(
    history: &[SessionRecord],
    file_path: &str,
    _profile: ContextProfile,
) -> String {
    let mut out = format!("Context for '{}':\n\n", file_path);
    let mut found = false;
    for session in history {
        for change in &session.changes {
            if change.file_path.contains(file_path) {
                out.push_str(&format!(
                    "- [{}] \"{}\": {}\n",
                    session.started_at, session.prompt, change.ast_summary
                ));
                found = true;
                break;
            }
        }
    }
    if !found {
        out.push_str("No recent changes found for this file.");
    }
    out
}
