//! How a patch replaces one block of a view the daemon holds: reading the
//! path an agent names the block by, in the grammar a refusal names blocks
//! by, and the view with that block swapped.

use crate::agent_view::{AgentView, Block};

impl AgentView {
    /// The view with the block at `path` replaced by `block`, or what is
    /// wrong with the path. The result is checked again by the caller, as a
    /// shown view is.
    pub fn patched(self, path: &str, block: Block) -> Result<AgentView, String> {
        let _ = block;
        Err(format!("{path}: patching a view is not supported yet"))
    }
}
