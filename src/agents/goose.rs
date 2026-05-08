use super::{AgentPlugin, CmdSetup, HookAdapter};
use super::generic::GenericAdapter;
use std::path::Path;

pub struct GooseAgent;
pub(crate) static PLUGIN: GooseAgent = GooseAgent;

impl AgentPlugin for GooseAgent {
    fn name(&self) -> &'static str { "goose" }

    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GenericAdapter }

    /// Goose injects `GOOSE_MOIM_MESSAGE_FILE` contents into working memory every turn,
    /// which is more reliable than AGENTS.md (re-injected each turn, not just at startup).
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
            envs: vec![("GOOSE_MOIM_MESSAGE_FILE".to_string(), skill_file.to_string_lossy().to_string())],
        }
    }
}
