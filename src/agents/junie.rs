use super::{AgentPlugin, CmdSetup, HookAdapter};
use super::generic::GenericAdapter;
use std::path::Path;

pub struct JunieAgent;
pub(crate) static PLUGIN: JunieAgent = JunieAgent;

impl AgentPlugin for JunieAgent {
    fn name(&self) -> &'static str { "junie" }

    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GenericAdapter }

    /// Junie accepts `--skill-location <dir>` pointing to the shai skills directory.
    fn cmd_setup(
        &self,
        args: &[String],
        _skill_file: &Path,
        _skill_content: &str,
        skills_dir: &Path,
    ) -> CmdSetup {
        CmdSetup {
            filtered_args: args.to_vec(),
            extra_args: vec!["--skill-location".to_string(), skills_dir.to_string_lossy().to_string()],
            envs: vec![],
        }
    }
}
