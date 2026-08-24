use std::ffi::OsString;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use argus_protocol::{PaneKind, PaneStatus, ProjectInfo};
use tokio::process::Command;

const SOURCE: &str = "argus:client";
const AGENT: &str = "argus";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentState {
    Idle,
    Working,
    Blocked,
}

impl AgentState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Update {
    Report {
        state: AgentState,
        message: Option<String>,
    },
    Release,
}

#[derive(Default)]
struct HerdrSync {
    last: Option<Update>,
}

impl HerdrSync {
    fn next_update(&mut self, tree: &[ProjectInfo]) -> Option<Update> {
        let update = aggregate(tree).unwrap_or(Update::Release);
        if self.last.as_ref() == Some(&update) {
            return None;
        }
        self.last = Some(update.clone());
        Some(update)
    }
}

fn aggregate(tree: &[ProjectInfo]) -> Option<Update> {
    let agents = tree
        .iter()
        .flat_map(|project| &project.checkouts)
        .flat_map(|checkout| &checkout.panes)
        .filter(|pane| pane.kind == PaneKind::Agent)
        .filter(|pane| !matches!(pane.status, PaneStatus::Exited { .. }));

    let mut state = None;
    let mut message = None;
    for pane in agents {
        match pane.status {
            PaneStatus::Waiting | PaneStatus::Failed => {
                state = Some(AgentState::Blocked);
                message = Some(pane.note.clone().unwrap_or_else(|| pane.title.clone()));
                break;
            }
            PaneStatus::Working => state = Some(AgentState::Working),
            PaneStatus::Idle if state.is_none() => state = Some(AgentState::Idle),
            PaneStatus::Idle | PaneStatus::Exited { .. } => {}
        }
    }

    state.map(|state| Update::Report { state, message })
}

pub struct HerdrReporter {
    binary: OsString,
    pane: OsString,
    sequence: u64,
    sync: HerdrSync,
}

impl HerdrReporter {
    pub fn from_env() -> Option<Self> {
        if std::env::var_os("HERDR_ENV").as_deref() != Some(std::ffi::OsStr::new("1")) {
            return None;
        }

        Some(Self {
            binary: std::env::var_os("HERDR_BIN_PATH")?,
            pane: std::env::var_os("HERDR_PANE_ID")?,
            sequence: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128 - 1) as u64,
            sync: HerdrSync::default(),
        })
    }

    pub fn update(&mut self, tree: &[ProjectInfo]) {
        if let Some(update) = self.sync.next_update(tree) {
            self.send(update);
        }
    }

    pub fn release(&mut self) {
        self.send(Update::Release);
    }

    fn send(&mut self, update: Update) {
        self.sequence += 1;
        let mut command = Command::new(&self.binary);
        command.arg("pane");

        match update {
            Update::Report { state, message } => {
                command.arg("report-agent").arg(&self.pane).args([
                    "--source",
                    SOURCE,
                    "--agent",
                    AGENT,
                    "--state",
                    state.as_str(),
                ]);
                if let Some(message) = message {
                    command.args(["--message", &message]);
                }
            }
            Update::Release => {
                command
                    .arg("release-agent")
                    .arg(&self.pane)
                    .args(["--source", SOURCE, "--agent", AGENT]);
            }
        }
        command.args(["--seq", &self.sequence.to_string()]);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        match command.spawn() {
            Ok(mut child) => {
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
            }
            Err(error) => tracing::warn!(%error, "failed to synchronize Argus state with Herdr"),
        }
    }
}

#[cfg(test)]
mod tests {
    use argus_protocol::{
        CheckoutId, CheckoutInfo, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId, ProjectInfo,
    };

    use super::{AgentState, HerdrSync, Update};

    fn tree(statuses: &[PaneStatus]) -> Vec<ProjectInfo> {
        vec![ProjectInfo {
            id: ProjectId(1),
            name: "project".into(),
            checkouts: vec![CheckoutInfo {
                id: CheckoutId(2),
                name: "checkout".into(),
                path: "/repo".into(),
                panes: statuses
                    .iter()
                    .enumerate()
                    .map(|(i, status)| PaneInfo {
                        id: PaneId(i as u64 + 3),
                        kind: PaneKind::Agent,
                        title: format!("agent-{i}"),
                        status: *status,
                        note: None,
                        template: Some("opencode".into()),
                    })
                    .collect(),
                git: None,
                primary: true,
            }],
        }]
    }

    #[test]
    fn a_reopened_client_reports_agents_that_are_already_running() {
        let mut sync = HerdrSync::default();

        assert_eq!(
            sync.next_update(&tree(&[PaneStatus::Idle])),
            Some(Update::Report {
                state: AgentState::Idle,
                message: None,
            })
        );
    }

    #[test]
    fn the_most_urgent_agent_state_is_reported() {
        let mut sync = HerdrSync::default();

        assert_eq!(
            sync.next_update(&tree(&[
                PaneStatus::Idle,
                PaneStatus::Working,
                PaneStatus::Waiting,
            ])),
            Some(Update::Report {
                state: AgentState::Blocked,
                message: Some("agent-2".into()),
            })
        );
    }

    #[test]
    fn unchanged_trees_do_not_repeat_reports() {
        let agents = tree(&[PaneStatus::Working]);
        let mut sync = HerdrSync::default();

        assert!(sync.next_update(&agents).is_some());
        assert_eq!(sync.next_update(&agents), None);
    }

    #[test]
    fn losing_the_last_live_agent_releases_argus_from_herdr() {
        let mut sync = HerdrSync::default();
        sync.next_update(&tree(&[PaneStatus::Idle]));

        assert_eq!(sync.next_update(&tree(&[])), Some(Update::Release));
        assert_eq!(sync.next_update(&tree(&[])), None);
    }
}
