use super::{AgentPlugin, CmdSetup, HookAdapter, filter_flag, flag_value};
use super::generic::GenericAdapter;
use std::path::Path;

pub struct OpenCodeAgent;
pub(crate) static PLUGIN: OpenCodeAgent = OpenCodeAgent;

impl AgentPlugin for OpenCodeAgent {
    fn name(&self) -> &'static str { "opencode" }

    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GenericAdapter }

    /// OpenCode accepts `--system <text>` inline. Read it from args if present.
    fn existing_content(&self, args: &[String], _skill_file: &Path) -> Option<String> {
        flag_value(args, "--system").map(|s| s.to_string())
    }

    fn cmd_setup(
        &self,
        args: &[String],
        _skill_file: &Path,
        skill_content: &str,
        _skills_dir: &Path,
    ) -> CmdSetup {
        let mut filtered = filter_flag(args, "--system");
        filtered.push("--system".to_string());
        filtered.push(skill_content.to_string());
        CmdSetup { filtered_args: filtered, extra_args: vec![], envs: vec![] }
    }
}
