//! How shared work is bounded when a caller reads or writes it.

use serde::{Deserialize, Serialize};

/// Repository and branch is the normal boundary. Workspace scope is an
/// explicit escape hatch for changes whose decisions and tasks span them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactScope {
    #[default]
    RepositoryBranch,
    Workspace,
}
