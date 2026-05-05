use crate::search_output;
use super::shared::{local_shai_or_die, parse_search_mode};

pub(crate) fn cmd_history(limit: u32, file: Option<String>) {
    let (_, db) = local_shai_or_die();
    let file_filter: Vec<String> = file.into_iter().collect();
    let history = if file_filter.is_empty() {
        db.get_history(limit)
    } else {
        db.get_history_filtered(limit, &file_filter, None)
    };

    if history.is_empty() {
        println!("No sessions recorded yet.");
        return;
    }
    for s in &history {
        println!("[{}][{}] \"{}\"", s.started_at, s.llm, s.prompt);
        for c in &s.changes {
            let short_hash = if c.blob_hash.len() > 8 { &c.blob_hash[..8] } else { &c.blob_hash };
            println!(
                "  ↳ [{}] [{}] {} — {}",
                c.timestamp, short_hash, c.file_path, c.ast_summary
            );
        }
        println!();
    }
}

pub(crate) fn cmd_log(file: &str, limit: u32) {
    let (_, db) = local_shai_or_die();
    let history = db.get_file_history(file, limit);
    if history.is_empty() {
        println!("No history found for '{}'.", file);
        return;
    }

    println!("Change log for '{}':\n", file);
    for c in history {
        let prompt = c.prompt.unwrap_or_else(|| "(no prompt)".to_string());
        let llm = c.llm.unwrap_or_else(|| "unknown".to_string());
        let short_hash = if c.blob_hash.len() > 8 { &c.blob_hash[..8] } else { &c.blob_hash };
        println!("[{}][{}] [{}] \"{}\"\n  ↳ {} ({})", c.timestamp, llm, short_hash, prompt, c.ast_summary, c.tool_name);
    }
}

pub(crate) fn cmd_rollback(file: &str, steps: u32) {
    let (shai_dir, db) = local_shai_or_die();
    let target_path = shai_dir.parent().unwrap_or(&shai_dir).join(file);

    let (hash, _time, summary) = match db.get_file_at_step(file, steps) {
        Some(found) => found,
        None => {
            eprintln!("❌ No history for '{}' at step {}", file, steps);
            return;
        }
    };

    let target_bytes = match db.get_blob(&hash) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("❌ Could not load step {}: {}", steps, e.message());
            return;
        }
    };

    if let Err(e) = std::fs::write(&target_path, target_bytes) {
        eprintln!("❌ Failed to write {}: {}", file, e);
        return;
    }

    println!("✅ Rolled back '{}' to step {} ({})", file, steps, summary);
}

pub(crate) fn cmd_diff(file: &str, steps: u32) {
    let (shai_dir, db) = local_shai_or_die();
    let target_path = shai_dir.parent().unwrap_or(&shai_dir).join(file);

    let (hash, time, summary) = match db.get_file_at_step(file, steps) {
        Some(found) => found,
        None => {
            eprintln!("❌ No history for '{}' at step {}", file, steps);
            return;
        }
    };

    let historical_bytes = match db.get_blob(&hash) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("❌ Could not load step {}: {}", steps, e.message());
            return;
        }
    };

    let current_bytes = std::fs::read(&target_path).unwrap_or_default();
    let diff = build_rollback_diff(file, &historical_bytes, &current_bytes, &time, &summary);
    println!("{}", diff);
}

pub(crate) fn cmd_search(query: &str, limit: u32, mode: &str) {
    let (_, db) = local_shai_or_die();
    let results = db.search_with_mode(query, limit, parse_search_mode(mode));
    println!("{}", search_output::format_search_results(query, mode, &results));
}

pub fn build_rollback_diff(
    file: &str,
    historical_bytes: &[u8],
    current_bytes: &[u8],
    timestamp: &str,
    summary: &str,
) -> String {
    let historical_text = String::from_utf8_lossy(historical_bytes);
    let current_text = String::from_utf8_lossy(current_bytes);

    if historical_text == current_text {
        return format!(
            "shai: '{}' already matches step 1 ({})\nNo changes to restore.",
            file, summary
        );
    }

    let diff = similar::TextDiff::from_lines(&current_text, &historical_text);
    let mut out = format!(
        "shai: rollback preview for '{}'\nTarget: Step 1 (saved {})\nSummary: {}\n\n",
        file, timestamp, summary
    );

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        out.push_str(&format!("{}{}", sign, change));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_rollback_diff_reports_identical_content() {
        let diff = build_rollback_diff(
            "test.rs",
            b"fn main() {}\n",
            b"fn main() {}\n",
            "2025-01-01 00:00:00",
            "no-op",
        );

        assert!(diff.contains("already matches step 1"));
        assert!(diff.contains("no-op"));
    }
}
