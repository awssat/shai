use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{AgentPlugin, HookAdapter};
use super::generic::looks_like_file_write_tool;

pub struct DroidAgent;
pub(crate) static PLUGIN: DroidAgent = DroidAgent;

impl AgentPlugin for DroidAgent {
    fn name(&self) -> &'static str { "droid" }

    fn hook_adapter(&self) -> &'static dyn HookAdapter { &DroidAdapter }

    /// Factory Droid auto-discovers `AGENTS.md` from the project root.
    fn skill_file(&self, cwd: &Path, _shai_dir: &Path, _skills_dir: &Path) -> PathBuf {
        cwd.join("AGENTS.md")
    }

    // existing_content: default (reads AGENTS.md if it exists)
    // merge_content: default (strip + append)
    // cmd_setup: default (droid auto-discovers AGENTS.md — no flag needed)
}

// ---------------------------------------------------------------------------
// DroidAdapter — handles Factory Droid's `--output-format stream-json` output
//
// Each line is a JSON object.  Tool calls look like:
//   {"type":"tool_call","toolName":"Execute","parameters":{"command":"..."}}
//   {"type":"tool_call","toolName":"Write","parameters":{"file_path":"..."}}
// ---------------------------------------------------------------------------

pub(crate) struct DroidAdapter;

impl HookAdapter for DroidAdapter {
    fn tool_name(&self, payload: &Value) -> Option<String> {
        payload.get("toolName")?.as_str().map(ToOwned::to_owned)
    }

    fn file_path(&self, tool_name: &str, payload: &Value) -> Option<String> {
        if looks_like_file_write_tool(tool_name) {
            let params = payload.get("parameters")?;
            params
                .get("file_path")
                .or_else(|| params.get("path"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        } else {
            None
        }
    }

    fn command_text(&self, _tool_name: &str, payload: &Value) -> Option<String> {
        payload
            .get("parameters")
            .and_then(|p| p.get("command"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    }
}
