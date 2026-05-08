use super::{AgentPlugin, HookAdapter};
use serde_json::Value;

pub struct GenericAgent;
pub(crate) static PLUGIN: GenericAgent = GenericAgent;

impl AgentPlugin for GenericAgent {
    fn name(&self) -> &'static str { "generic" }
    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GenericAdapter }
    // All defaults: passthrough args, default skill file, no extra envs.
}

// ---------------------------------------------------------------------------
// GenericAdapter — broad JSON shape coverage for unknown agents
// ---------------------------------------------------------------------------

pub(crate) struct GenericAdapter;

impl HookAdapter for GenericAdapter {
    fn tool_name(&self, payload: &Value) -> Option<String> {
        first_string(
            payload,
            &[
                &["tool_name"],
                &["tool"],
                &["name"],
                &["tool_call", "name"],
                &["call", "name"],
                &["function", "name"],
            ],
        )
        .map(ToOwned::to_owned)
    }

    fn file_path(&self, tool_name: &str, payload: &Value) -> Option<String> {
        if looks_like_file_write_tool(tool_name) {
            first_string(
                payload,
                &[
                    &["tool_input", "path"],
                    &["tool_input", "file_path"],
                    &["input", "path"],
                    &["input", "file_path"],
                    &["arguments", "path"],
                    &["arguments", "file_path"],
                    &["path"],
                    &["file_path"],
                ],
            )
            .map(ToOwned::to_owned)
        } else {
            None
        }
    }

    fn command_text(&self, _tool_name: &str, payload: &Value) -> Option<String> {
        shell_command_from_payload(payload)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn shell_command_from_payload(payload: &Value) -> Option<String> {
    for path in [
        &["tool_input"][..],
        &["input"][..],
        &["arguments"][..],
        &["tool_call", "arguments"][..],
        &["call", "arguments"][..],
        &[][..],
    ] {
        if let Some(value) = value_at(payload, path) {
            if let Some(command) = command_from_value(value) {
                return Some(command);
            }
        }
    }
    None
}

pub(crate) fn looks_like_file_write_tool(tool_name: &str) -> bool {
    let normalized = tool_name.to_ascii_lowercase();
    normalized == "posttooluse"
        || normalized.contains("write")
        || normalized.contains("edit")
        || normalized.contains("patch")
}

pub(crate) fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

pub(crate) fn first_string<'a>(value: &'a Value, paths: &[&[&str]]) -> Option<&'a str> {
    paths
        .iter()
        .find_map(|path| value_at(value, path)?.as_str())
}

fn command_from_value(value: &Value) -> Option<String> {
    if let Some(command) = first_string(
        value,
        &[
            &["command"],
            &["cmd"],
            &["command_line"],
            &["script"],
            &["shell_command"],
        ],
    ) {
        return Some(command.to_string());
    }
    for key in ["args", "argv"] {
        if let Some(args) = value.get(key).and_then(Value::as_array) {
            let parts: Vec<String> = args
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect();
            if !parts.is_empty() {
                return Some(parts.join(" "));
            }
        }
    }
    None
}

