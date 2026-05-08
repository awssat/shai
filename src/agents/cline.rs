use super::{AgentPlugin, HookAdapter};
use super::generic::GenericAdapter;
use std::path::{Path, PathBuf};

pub struct ClineAgent;
pub(crate) static PLUGIN: ClineAgent = ClineAgent;

impl AgentPlugin for ClineAgent {
    fn name(&self) -> &'static str { "cline" }

    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GenericAdapter }

    /// Cline reads `.clinerules` from the project root for custom instructions.
    /// It accepts either a file or a directory (all files inside are loaded).
    /// If the path already exists as a directory we write inside it so shai
    /// doesn't shadow other rules files that live there.
    fn skill_file(&self, cwd: &Path, _shai_dir: &Path, _skills_dir: &Path) -> PathBuf {
        let base = cwd.join(".clinerules");
        if base.is_dir() {
            base.join("shai-context.md")
        } else {
            base
        }
    }

    // existing_content: default (reads .clinerules if it exists)
    // merge_content: default (strip + append)
    // cmd_setup: default (Cline discovers .clinerules automatically — no flag needed)
}
