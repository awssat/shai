use crate::context;
use crate::status_output;

use super::shared::{local_shai_or_die, truncate_path};

pub(crate) fn cmd_status() {
    let (_, db) = local_shai_or_die();
    print!("{}", status_output::render_status(&db));
}

pub(crate) fn cmd_analytics(file: Option<&str>, subsystem: Option<&str>, limit: u32) {
    let (_, db) = local_shai_or_die();
    let analytics = db.get_analytics(file, subsystem, limit);

    println!("shai project analytics\n");

    if let Some(f) = file {
        println!("  file filter         {}", f);
    }
    if let Some(s) = subsystem {
        println!("  subsystem filter    {}", s);
    }

    if !analytics.recent_touches.is_empty() {
        println!("  recent activity:");
        for touch in &analytics.recent_touches {
            let prompt_note = if touch.prompt_kind == "missing" {
                " (unrecorded prompt)"
            } else {
                ""
            };
            println!(
                "    [{}][{}:{}] {:>3}x  {} — {}{}",
                touch.timestamp,
                touch.agent_family,
                touch.llm,
                touch.touch_count,
                touch.tool_name_norm,
                truncate_path(&touch.file_path, 64),
                prompt_note,
            );
        }
    }

    if !analytics.top_tools.is_empty() {
        println!("\n  top tools:");
        for tool in &analytics.top_tools {
            println!("    {:>3}x  {}", tool.count, tool.tool_name_norm);
        }
    }

    if !analytics.missing_prompt_sessions.is_empty() {
        println!("\n  sessions with missing prompts:");
        for session in &analytics.missing_prompt_sessions {
            println!(
                "    [{}][{}] {} — {} change(s)",
                session.started_at, session.llm, session.session_key, session.change_count
            );
        }
    }
    println!();
}

pub(crate) fn cmd_summary() {
    let (_, db) = local_shai_or_die();
    let history = db.get_history(20);
    let report = context::project_summary_report(
        std::path::Path::new("."),
        &history,
        context::ContextProfile::Standard,
    );
    println!("{}\n", report);
}

pub(crate) fn cmd_why(path: &str) {
    let (_, db) = local_shai_or_die();
    let history = db.get_history_filtered(12, &[path.to_string()], None);
    let report = context::why_file_report(&history, path, context::ContextProfile::Standard);
    println!("{}\n", report);
}
