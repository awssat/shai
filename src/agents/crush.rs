use super::{AgentPlugin, HookAdapter};
use super::generic::GenericAdapter;
use std::path::{Path, PathBuf};

pub struct CrushAgent;
pub(crate) static PLUGIN: CrushAgent = CrushAgent;

impl AgentPlugin for CrushAgent {
    fn name(&self) -> &'static str { "crush" }

    fn hook_adapter(&self) -> &'static dyn HookAdapter { &GenericAdapter }

    /// Crush auto-reads `CRUSH.md` from the project root (ahead of AGENTS.md/CLAUDE.md/GEMINI.md).
    /// Writing here ensures shai context takes effect without clobbering other agents' files.
    fn skill_file(&self, cwd: &Path, _shai_dir: &Path, _skills_dir: &Path) -> PathBuf {
        cwd.join("CRUSH.md")
    }

    // existing_content: default (reads CRUSH.md if it exists)
    // merge_content: default (strip + append)
    // cmd_setup: default (crush discovers CRUSH.md automatically — no flag needed)
}
