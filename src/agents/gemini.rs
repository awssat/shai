use super::{AgentPlugin, CmdSetup, HookAdapter};
use serde_json::Value;
use std::path::Path;

pub struct GeminiAgent;
pub(crate) static PLUGIN: GeminiAgent = GeminiAgent;

impl AgentPlugin for GeminiAgent {
    fn name(&self) -> &'static str { "gemini" }
    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GeminiAdapter }

    /// Gemini reads its system MD from `GEMINI_SYSTEM_MD`; if that env var points to a file,
    /// use its content as the existing base to merge from.
    fn existing_content(&self, _args: &[String], _skill_file: &std::path::Path) -> Option<String> {
        let path = std::env::var("GEMINI_SYSTEM_MD").ok()?;
        std::fs::read_to_string(path).ok()
    }

    fn cmd_setup(
        &self,
        args: &[String],
        skill_file: &Path,
        _skill_content: &str,
        _skills_dir: &Path,
    ) -> CmdSetup {
        CmdSetup {
            filtered_args: args.to_vec(),
            extra_args: vec![],
            envs: vec![("GEMINI_SYSTEM_MD".to_string(), skill_file.to_string_lossy().to_string())],
        }
    }
}

// ---------------------------------------------------------------------------
// GeminiAdapter
// ---------------------------------------------------------------------------

struct GeminiAdapter;

impl HookAdapter for GeminiAdapter {
    fn tool_name(&self, payload: &Value) -> Option<String> {
        payload["tool_name"].as_str().map(ToOwned::to_owned)
    }

    fn file_path(&self, tool_name: &str, payload: &Value) -> Option<String> {
        match tool_name {
            "postToolUse" | "fs_write" | "write" => payload["tool_input"]["path"]
                .as_str()
                .or_else(|| payload["tool_input"]["file_path"].as_str())
                .map(ToOwned::to_owned),
            _ => None,
        }
    }

    fn command_text(&self, _tool_name: &str, payload: &Value) -> Option<String> {
        payload["tool_input"]["command"]
            .as_str()
            .or_else(|| payload["tool_input"]["cmd"].as_str())
            .or_else(|| payload["tool_input"]["command_line"].as_str())
            .map(ToOwned::to_owned)
            .or_else(|| {
                payload["tool_input"]["args"].as_array().and_then(|args| {
                    let parts: Vec<String> = args
                        .iter()
                        .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                        .collect();
                    if parts.is_empty() { None } else { Some(parts.join(" ")) }
                })
            })
    }
}

