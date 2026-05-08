use super::{AgentPlugin, HookAdapter};
use super::generic::GenericAdapter;
use std::path::{Path, PathBuf};

pub struct CopilotAgent;
pub(crate) static PLUGIN: CopilotAgent = CopilotAgent;

impl AgentPlugin for CopilotAgent {
    fn name(&self) -> &'static str { "copilot" }

    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GenericAdapter }

    /// Copilot auto-discovers `.github/copilot-instructions.md`.
    fn skill_file(&self, cwd: &Path, _shai_dir: &Path, _skills_dir: &Path) -> PathBuf {
        cwd.join(".github/copilot-instructions.md")
    }

    // existing_content: default (reads skill_file)
    // merge_content: default (strip + append)
    // cmd_setup: default (pass args through — copilot auto-discovers the file)
}
