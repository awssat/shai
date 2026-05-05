mod gemini;
mod generic;
#[cfg(test)]
mod tests;

use gemini::GeminiAdapter;
use generic::{CopilotAdapter, GenericAdapter, OpencodeAdapter};
use serde_json::Value;
use std::path::{Path, PathBuf};

const BUILTIN_ADAPTERS: [&str; 5] = [
    "claude", "copilot", "gemini", "generic", "opencode",
];

pub trait HookAdapter {
    fn tool_name(&self, payload: &Value) -> Option<String>;
    fn file_path(&self, tool_name: &str, payload: &Value) -> Option<String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInventoryItem {
    pub name: String,
    pub path: Option<PathBuf>,
}

pub fn adapter_for(llm: &str) -> &'static dyn HookAdapter {
    match llm {
        "claude" => &GenericAdapter,
        "gemini" => &GeminiAdapter,
        "opencode" => &OpencodeAdapter,
        "copilot" => &CopilotAdapter,
        "generic" => &GenericAdapter,
        _ => &GenericAdapter,
    }
}

pub fn list_adapters_for(_start: &Path) -> Vec<AdapterInventoryItem> {
    let mut items = Vec::with_capacity(BUILTIN_ADAPTERS.len());

    for name in BUILTIN_ADAPTERS {
        items.push(AdapterInventoryItem {
            name: name.to_string(),
            path: None,
        });
    }

    items.sort_by(|left, right| left.name.cmp(&right.name));
    items
}
