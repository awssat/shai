use super::{AgentPlugin, HookAdapter};
use super::generic::GenericAdapter;
use std::path::{Path, PathBuf};

pub struct KiloAgent;
pub(crate) static PLUGIN: KiloAgent = KiloAgent;

impl AgentPlugin for KiloAgent {
    fn name(&self) -> &'static str { "kilo" }

    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GenericAdapter }

    /// Kilo auto-discovers `AGENTS.md` at the project root (OpenCode fork).
    fn skill_file(&self, cwd: &Path, _shai_dir: &Path, _skills_dir: &Path) -> PathBuf {
        cwd.join("AGENTS.md")
    }

    // existing_content: default (reads AGENTS.md)
    // merge_content: default (strip + append)
    // cmd_setup: default (pass args through — kilo auto-discovers AGENTS.md)
}
