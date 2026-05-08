use std::fmt::Write;

use crate::agents;
use crate::storage::Storage;

fn truncate_prompt(prompt: &str, max: usize) -> &str {
    &prompt[..prompt.len().min(max)]
}

pub(crate) fn render_status(storage: &Storage) -> String {
    let status = storage.get_status();
    let project_root = storage.project_root();
    let adapters = agents::list_adapters();

    let mut out = String::with_capacity(2048);
    let _ = writeln!(out, "shai project status\n");
    let _ = writeln!(out, "  project root {}", project_root.display());
    let _ = writeln!(out, "  project id   {}", status.project_id);
    let _ = writeln!(out, "  shai dir     {}", storage.shai_dir.display());
    let _ = writeln!(
        out,
        "  sessions     {} total, {} open",
        status.total_sessions, status.open_sessions
    );
    let _ = writeln!(out, "  changes      {}", status.total_changes);
    let _ = writeln!(out, "  files        {} unique", status.unique_files);
    let _ = writeln!(
        out,
        "  storage      {} raw bytes, {} stored bytes ({:.2}x)",
        status.raw_bytes, status.stored_bytes, status.compression_ratio
    );

    if let Some(first) = &status.first_at {
        let _ = writeln!(out, "  tracked from {}", first);
    }
    if let Some(last_session) = &status.last_at {
        let _ = writeln!(out, "  last session {}", last_session);
    }
    if let Some(last_change) = &status.last_change_at {
        let _ = writeln!(out, "  last change  {}", last_change);
    }
    if let Some(prompt) = &status.last_prompt {
        let _ = writeln!(out, "  last prompt  \"{}\"", truncate_prompt(prompt, 72));
    }
    if let Some(checkpoint) = &status.last_checkpoint {
        let when = status
            .last_checkpoint_at
            .as_deref()
            .unwrap_or("unknown time");
        let _ = writeln!(
            out,
            "  checkpoint   \"{}\" ({})",
            truncate_prompt(checkpoint, 72),
            when
        );
    }
    if !status.top_agents.is_empty() {
        let summary = status
            .top_agents
            .iter()
            .map(|(agent, count)| format!("{}({})", agent, count))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "  agents       {}", summary);
    }

    let _ = writeln!(out, "  adapters     {} visible", adapters.len());

    if !status.top_files.is_empty() {
        let _ = writeln!(out, "\n  most changed files:");
        for (path, count) in &status.top_files {
            let _ = writeln!(out, "    {:>3}x  {}", count, path);
        }
    }

    if !status.storage_hotspots.is_empty() {
        let _ = writeln!(out, "\n  checkpoint candidates:");
        for hotspot in &status.storage_hotspots {
            let _ = writeln!(
                out,
                "    {:>3} rev  {}  ({:.1} KB raw, {:.1} KB stored)",
                hotspot.revisions,
                hotspot.file_path,
                hotspot.raw_bytes as f64 / 1024.0,
                hotspot.stored_bytes as f64 / 1024.0,
            );
        }
    }

    out
}
