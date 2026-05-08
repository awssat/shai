use super::{AgentPlugin, CmdSetup, HookAdapter, filter_flag, flag_value, default_merge};
use super::generic::GenericAdapter;
use std::path::Path;

pub struct ClaudeAgent;
pub(crate) static PLUGIN: ClaudeAgent = ClaudeAgent;

impl AgentPlugin for ClaudeAgent {
    fn name(&self) -> &'static str { "claude" }

    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GenericAdapter }

    fn existing_content(&self, args: &[String], _skill_file: &Path) -> Option<String> {
        let path = flag_value(args, "--append-system-prompt-file")?;
        std::fs::read_to_string(path).ok()
    }

    fn merge_content(&self, existing: Option<&str>, shai_content: &str) -> String {
        default_merge(existing, shai_content)
    }

    fn cmd_setup(
        &self,
        args: &[String],
        skill_file: &Path,
        _skill_content: &str,
        _skills_dir: &Path,
    ) -> CmdSetup {
        let mut filtered = filter_flag(args, "--append-system-prompt-file");
        filtered.push("--append-system-prompt-file".to_string());
        filtered.push(skill_file.to_string_lossy().to_string());
        CmdSetup { filtered_args: filtered, extra_args: vec![], envs: vec![] }
    }
}
