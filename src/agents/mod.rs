/// Per-agent plugin system.
///
/// To add a new agent:
///   1. Create `src/agents/<name>.rs` and implement `AgentPlugin` + `HookAdapter`.
///   2. Export `pub(crate) static PLUGIN: <Type> = <Type>;` from that file.
///
/// The build script discovers `src/agents/*.rs` automatically, so no registry
/// edits are needed when adding a new agent.
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// HookAdapter — runtime JSON sniffer trait
// ---------------------------------------------------------------------------

/// Extracts tool name, file path, and shell command from an agent's tool-call JSON.
pub trait HookAdapter: Send + Sync {
    fn tool_name(&self, payload: &serde_json::Value) -> Option<String>;
    fn file_path(&self, tool_name: &str, payload: &serde_json::Value) -> Option<String>;
    fn command_text(&self, tool_name: &str, payload: &serde_json::Value) -> Option<String>;
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Describes how to invoke the agent process after shai injects its context.
pub struct CmdSetup {
    /// Args to pass to the agent (may have user-supplied flags filtered out).
    pub filtered_args: Vec<String>,
    /// Extra args to append after `filtered_args` (flat: `["--flag", "value", ...]`).
    pub extra_args: Vec<String>,
    /// Environment variables to set on the agent process.
    pub envs: Vec<(String, String)>,
}

impl CmdSetup {
    pub fn passthrough(args: &[String]) -> Self {
        CmdSetup {
            filtered_args: args.to_vec(),
            extra_args: vec![],
            envs: vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait AgentPlugin: Send + Sync {
    /// The executable name used to identify this agent (e.g. `"goose"`).
    fn name(&self) -> &'static str;

    /// Which HookAdapter to use when sniffing this agent's PTY output.
    fn hook_adapter(&self) -> &'static dyn HookAdapter;

    /// Where to write the shai context file for this agent.
    ///
    /// Default: `.shai/skills/shai-context.md`
    fn skill_file(&self, _cwd: &Path, _shai_dir: &Path, skills_dir: &Path) -> PathBuf {
        skills_dir.join("shai-context.md")
    }

    /// Return any pre-existing content this agent already has for its system prompt.
    ///
    /// Default: read from `skill_file` if it exists.
    fn existing_content(&self, _args: &[String], skill_file: &Path) -> Option<String> {
        std::fs::read_to_string(skill_file).ok()
    }

    /// Merge the agent's existing content with the new shai content block.
    ///
    /// Default: strip previous shai block from `existing`, then append `shai_content`.
    fn merge_content(&self, existing: Option<&str>, shai_content: &str) -> String {
        default_merge(existing, shai_content)
    }

    /// Return how the agent process should be invoked: filtered args, extra args, env vars.
    ///
    /// Default: pass all args through unchanged, no extra args, no extra env vars.
    fn cmd_setup(
        &self,
        args: &[String],
        _skill_file: &Path,
        _skill_content: &str,
        _skills_dir: &Path,
    ) -> CmdSetup {
        CmdSetup::passthrough(args)
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

include!(concat!(env!("OUT_DIR"), "/agents_registry.rs"));

/// Return the plugin for a given agent executable name, or `None` for unknown agents.
pub fn find(name: &str) -> Option<&'static dyn AgentPlugin> {
    GENERATED_REGISTRY
        .iter()
        .find(|p| p.name() == name)
        .copied()
}

/// Names of all registered agents (used by `shai adapters list`).
pub fn all_names() -> Vec<&'static str> {
    let mut names: Vec<_> = GENERATED_REGISTRY.iter().map(|p| p.name()).collect();
    names.sort_unstable();
    names
}

/// Return the HookAdapter for a given agent name.
/// Falls back to the generic adapter for unknown agents.
pub fn adapter_for(llm: &str) -> &'static dyn HookAdapter {
    find(llm)
        .map(|p| p.hook_adapter())
        .unwrap_or_else(|| find("generic").expect("generic agent always registered").hook_adapter())
}

/// Inventory item used by `shai adapters list`.
pub struct AdapterInventoryItem {
    pub name: String,
}

/// List all registered agents (used by `shai adapters list`).
pub fn list_adapters() -> Vec<AdapterInventoryItem> {
    let mut items: Vec<AdapterInventoryItem> = all_names()
        .into_iter()
        .map(|name| AdapterInventoryItem { name: name.to_string() })
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    items
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Remove any previously-written shai context block from a file's content so
/// each run replaces it cleanly rather than accumulating duplicates.
pub(crate) fn strip_shai_block(content: &str) -> &str {
    const MARKER: &str = "# SHAI — Persistent Project Memory";
    if let Some(pos) = content.find(MARKER) {
        let trimmed = content[..pos].trim_end();
        if trimmed.is_empty() {
            ""
        } else {
            trimmed
        }
    } else {
        content
    }
}

pub(crate) fn default_merge(existing: Option<&str>, shai_content: &str) -> String {
    match existing {
        None => shai_content.to_string(),
        Some(e) => {
            let base = strip_shai_block(e);
            if base.is_empty() {
                shai_content.to_string()
            } else {
                format!("{}\n\n{}", base, shai_content)
            }
        }
    }
}

/// Filter `--flag value` pairs from a slice of args.
pub(crate) fn filter_flag(args: &[String], flag: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == flag {
            skip_next = true;
            continue;
        }
        out.push(arg.clone());
    }
    out
}

/// Extract the value of `--flag value` from args, if present.
pub(crate) fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == flag)?;
    args.get(idx + 1).map(|s| s.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registry_discovers_all_agent_files() {
        assert_eq!(
            all_names(),
            vec!["claude", "copilot", "gemini", "generic", "goose", "junie", "kilo", "opencode",]
        );
        for name in all_names() {
            assert!(find(name).is_some(), "missing plugin for {name}");
        }
    }

    #[test]
    fn claude_cmd_setup_replaces_existing_system_prompt_flag() {
        let plugin = find("claude").unwrap();
        let args = vec![
            "chat".to_string(),
            "--append-system-prompt-file".to_string(),
            "/tmp/old.md".to_string(),
        ];
        let setup = plugin.cmd_setup(
            &args,
            Path::new("/tmp/new.md"),
            "ignored",
            Path::new("/tmp/skills"),
        );

        assert_eq!(
            setup.filtered_args,
            vec![
                "chat".to_string(),
                "--append-system-prompt-file".to_string(),
                "/tmp/new.md".to_string(),
            ]
        );
    }

    #[test]
    fn native_context_paths_match_agent_conventions() {
        let cwd = Path::new("/repo");
        let shai_dir = Path::new("/repo/.shai");
        let skills_dir = Path::new("/repo/.shai/skills");

        assert_eq!(
            find("copilot")
                .unwrap()
                .skill_file(cwd, shai_dir, skills_dir),
            PathBuf::from("/repo/.github/copilot-instructions.md")
        );
        assert_eq!(
            find("kilo").unwrap().skill_file(cwd, shai_dir, skills_dir),
            PathBuf::from("/repo/AGENTS.md")
        );
        assert_eq!(
            find("generic")
                .unwrap()
                .skill_file(cwd, shai_dir, skills_dir),
            PathBuf::from("/repo/.shai/skills/shai-context.md")
        );
    }

    #[test]
    fn env_and_flag_plugins_use_native_launch_setup() {
        let skill_file = Path::new("/repo/.shai/skills/shai-context.md");
        let skills_dir = Path::new("/repo/.shai/skills");

        let gemini = find("gemini")
            .unwrap()
            .cmd_setup(&[], skill_file, "ignored", skills_dir);
        assert_eq!(
            gemini.envs,
            vec![(
                "GEMINI_SYSTEM_MD".to_string(),
                "/repo/.shai/skills/shai-context.md".to_string(),
            )]
        );

        let goose = find("goose")
            .unwrap()
            .cmd_setup(&[], skill_file, "ignored", skills_dir);
        assert_eq!(
            goose.envs,
            vec![(
                "GOOSE_MOIM_MESSAGE_FILE".to_string(),
                "/repo/.shai/skills/shai-context.md".to_string(),
            )]
        );

        let junie = find("junie")
            .unwrap()
            .cmd_setup(&[], skill_file, "ignored", skills_dir);
        assert_eq!(
            junie.extra_args,
            vec![
                "--skill-location".to_string(),
                "/repo/.shai/skills".to_string(),
            ]
        );

        let opencode = find("opencode").unwrap().cmd_setup(
            &["run".to_string(), "--system".to_string(), "old".to_string()],
            skill_file,
            "merged context",
            skills_dir,
        );
        assert_eq!(
            opencode.filtered_args,
            vec![
                "run".to_string(),
                "--system".to_string(),
                "merged context".to_string(),
            ]
        );
    }

    // --- HookAdapter tests (moved from src/adapters/tests.rs) ---

    #[test]
    fn test_gemini_adapter() {
        let adapter = adapter_for("gemini");
        let payload = json!({
            "tool_name": "fs_write",
            "tool_input": { "path": "test.txt" }
        });
        assert_eq!(adapter.tool_name(&payload), Some("fs_write".to_string()));
        assert_eq!(adapter.file_path("fs_write", &payload), Some("test.txt".to_string()));
    }

    #[test]
    fn test_generic_adapter_extracts_shell_command() {
        let adapter = adapter_for("generic");
        let payload = json!({
            "tool_name": "shell",
            "tool_input": { "command": "git reset --hard" }
        });
        assert_eq!(adapter.tool_name(&payload), Some("shell".to_string()));
        assert_eq!(adapter.command_text("shell", &payload), Some("git reset --hard".to_string()));
    }

    #[test]
    fn test_generic_adapter_accepts_common_fallback_fields() {
        let adapter = adapter_for("generic");
        let payload = json!({
            "name": "write_file",
            "arguments": { "file_path": "src/main.rs" }
        });
        assert_eq!(adapter.tool_name(&payload), Some("write_file".to_string()));
        assert_eq!(adapter.file_path("write_file", &payload), Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_generic_adapter_extracts_commands_from_nested_arguments() {
        let adapter = adapter_for("generic");
        let payload = json!({
            "tool": "shell",
            "input": { "argv": ["cargo", "test", "--quiet"] }
        });
        assert_eq!(adapter.tool_name(&payload), Some("shell".to_string()));
        assert_eq!(adapter.command_text("shell", &payload), Some("cargo test --quiet".to_string()));
    }
}

