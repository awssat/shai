use super::{AgentPlugin, CmdSetup, HookAdapter, filter_flag, flag_value, strip_shai_block};
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

    fn merge_content(&self, existing: Option<&str>, shai_content: &str) -> String {
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
