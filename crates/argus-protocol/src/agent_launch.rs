//! What an agent asks for when it starts another agent through the daemon,
//! and what it is told it started: a pane of its own, in this checkout, a
//! new worktree, or another repository's checkout, rather than a subagent
//! living and dying inside its turn.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::PaneId;

/// The most a launch's body may be. The prompt reaches the CLI as an
/// argument, and Windows caps a whole command line at 32K characters, so a
/// brief longer than this belongs in a file the prompt points at.
pub const MAX_LAUNCH_BYTES: usize = 16 * 1024;

/// The body of `POST /pane/<id>/agent`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLaunch {
    /// The agent template to start. Absent means the asking pane's own, so
    /// a Claude that starts a helper gets a Claude without having to know
    /// what its template is called.
    #[serde(default)]
    pub template: Option<String>,
    /// The new conversation's first message. Without one the agent sits at
    /// its prompt, which only a person can make use of.
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub place: LaunchPlace,
}

/// Which checkout the new agent works in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum LaunchPlace {
    /// The checkout the asking agent is in.
    #[default]
    Here,
    /// A new worktree on `branch`, made the way the window makes one: an
    /// existing branch gets a directory, a new one is branched off HEAD.
    /// `from` names a checkout of the repository to make it in, for an
    /// agent starting work in a repository other than its own.
    Worktree {
        branch: String,
        #[serde(default)]
        from: Option<PathBuf>,
    },
    /// A checkout Argus already shows, in any repository of any workspace.
    /// One it does not is refused rather than added: what joins the tree is
    /// the person's call.
    Checkout { path: PathBuf },
}

/// What the daemon answers a launch it carried out with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLaunched {
    pub pane: PaneId,
    pub template: String,
    pub checkout: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_launch_starts_the_same_agent_here() {
        let launch: AgentLaunch = serde_json::from_str("{}").unwrap();
        assert_eq!(launch, AgentLaunch::default());
        assert_eq!(launch.place, LaunchPlace::Here);
    }

    #[test]
    fn a_worktree_launch_reads_back_as_written() {
        let launch = AgentLaunch {
            template: Some("claude".into()),
            prompt: Some("fix the resize".into()),
            place: LaunchPlace::Worktree { branch: "resize".into(), from: None },
        };
        let wire = serde_json::to_string(&launch).unwrap();
        assert!(wire.contains(r#""kind":"worktree""#), "{wire}");
        assert_eq!(serde_json::from_str::<AgentLaunch>(&wire).unwrap(), launch);
    }
}
