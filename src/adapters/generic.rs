use super::HookAdapter;
use serde_json::Value;

pub(crate) struct GenericAdapter;

impl HookAdapter for GenericAdapter {
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
}

pub(crate) struct OpencodeAdapter;

impl HookAdapter for OpencodeAdapter {
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
}

pub(crate) struct CopilotAdapter;

impl HookAdapter for CopilotAdapter {
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
}
