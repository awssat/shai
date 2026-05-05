use crate::storage::SessionRecord;
use std::collections::HashSet;

pub const SEARCH_RESPONSE_BUDGET: usize = 2_500;
const SESSION_PROMPT_CLIP: usize = 96;
const CHANGE_SUMMARY_CLIP: usize = 120;
const CHANGE_PATH_CLIP: usize = 72;
const MAX_CHANGES_PER_SESSION: usize = 3;

pub fn format_search_results(query: &str, mode: &str, results: &[SessionRecord]) -> String {
    if results.is_empty() {
        return format!("No results for \"{}\".", query);
    }

    let mut out = String::new();
    push_with_budget(
        &mut out,
        &format!(
            "{} session(s) matching \"{}\" [{}]:\n\n",
            results.len(),
            query,
            mode
        ),
        SEARCH_RESPONSE_BUDGET,
    );

    let mut rendered_sessions = 0usize;
    for session in results.iter().take(10) {
        let start_len = out.len();
        if !push_with_budget(
            &mut out,
            &format!(
                "[{}][{}] \"{}\"\n",
                session.started_at,
                session.llm,
                clip(&session.prompt, SESSION_PROMPT_CLIP),
            ),
            SEARCH_RESPONSE_BUDGET,
        ) {
            break;
        }

        let mut seen = HashSet::new();
        let mut rendered_changes = 0usize;
        for change in &session.changes {
            if !seen.insert(change.file_path.as_str()) {
                continue;
            }
            if rendered_changes >= MAX_CHANGES_PER_SESSION {
                break;
            }
            if !push_with_budget(
                &mut out,
                &format!(
                    "  ↳ [{}] {} — {}\n",
                    &change.timestamp[..change.timestamp.len().min(16)],
                    clip(&change.file_path, CHANGE_PATH_CLIP),
                    clip(&change.ast_summary, CHANGE_SUMMARY_CLIP),
                ),
                SEARCH_RESPONSE_BUDGET,
            ) {
                break;
            }
            rendered_changes += 1;
        }

        if !push_with_budget(&mut out, "\n", SEARCH_RESPONSE_BUDGET) {
            break;
        }

        if out.len() == start_len {
            break;
        }
        rendered_sessions += 1;

        if out.len() >= SEARCH_RESPONSE_BUDGET {
            break;
        }
    }

    let total_matched = results.len().min(10);
    let remaining_sessions = total_matched.saturating_sub(rendered_sessions);
    if remaining_sessions > 0 || results.len() > 10 {
        let _ = push_with_budget(
            &mut out,
            &format!(
                "… {} more matching session(s) omitted.\n",
                results.len() - rendered_sessions
            ),
            SEARCH_RESPONSE_BUDGET,
        );
    }

    out.trim_end().to_string()
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}…", &text[..max])
    }
}

fn push_with_budget(out: &mut String, text: &str, budget: usize) -> bool {
    if out.len() + text.len() > budget {
        return false;
    }
    out.push_str(text);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ChangeRecord;

    fn session(prompt: &str, started_at: &str, files: &[(&str, &str)]) -> SessionRecord {
        SessionRecord {
            id: 0,
            session_key: "s".into(),
            llm: "claude".into(),
            prompt: prompt.into(),
            started_at: started_at.into(),
            changes: files
                .iter()
                .map(|(path, summary)| ChangeRecord {
                    file_path: (*path).into(),
                    blob_hash: "hash".into(),
                    ast_summary: (*summary).into(),
                    tool_name: "Write".into(),
                    timestamp: "2024-01-01 10:00:00".into(),
                    agent_family: "anthropic".into(),
                    agent_name: "claude".into(),
                })
                .collect(),
        }
    }

    #[test]
    fn search_output_stays_within_budget() {
        let results: Vec<_> = (0..12)
            .map(|idx| {
                session(
                    &format!("prompt {}", idx),
                    "2024-01-01",
                    &[("src/main.rs", "summary")],
                )
            })
            .collect();

        let rendered = format_search_results("test", "all", &results);
        assert!(rendered.len() <= SEARCH_RESPONSE_BUDGET);
        assert!(rendered.contains("matching \"test\""));
        assert!(rendered.contains("omitted"));
    }

    #[test]
    fn search_output_renders_pre_filtered_regex_matches() {
        let results = vec![session(
            "fix the bug",
            "2024-01-01",
            &[("src/main.rs", "summary")],
        )];

        let rendered = format_search_results("bug", "all", &results);
        assert!(rendered.contains("fix the bug"));
    }

    #[test]
    fn prompt_only_results_still_render_metadata() {
        let results = vec![SessionRecord {
            id: 1,
            session_key: "s".into(),
            llm: "gemini".into(),
            prompt: "no file changes here".into(),
            started_at: "2024-01-01 12:00:00".into(),
            changes: vec![],
        }];

        let rendered = format_search_results("no file", "prompt", &results);
        assert!(rendered.contains("[2024-01-01 12:00:00][gemini]"));
        assert!(rendered.contains("no file changes here"));
    }
}
