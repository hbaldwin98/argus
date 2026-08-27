use std::ffi::OsString;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use argus_protocol::{ChildAgentInfo, PaneInfo, PaneKind, PaneStatus, ProjectInfo};
use tokio::process::Command;

const SOURCE: &str = "argus:client";
const AGENT: &str = "argus";
const MAX_MESSAGE_CHARS: usize = 240;

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
    fn next_update(&mut self, tree: &[ProjectInfo], workspace: &str) -> Option<Update> {
        let update = aggregate(tree, workspace).unwrap_or(Update::Release);
        if self.last.as_ref() == Some(&update) {
            return None;
        }
        self.last = Some(update.clone());
        Some(update)
    }
}

fn aggregate(tree: &[ProjectInfo], workspace: &str) -> Option<Update> {
    let agents = tree
        .iter()
        .flat_map(|project| &project.repositories)
        .flat_map(|repository| &repository.checkouts)
        .flat_map(|checkout| &checkout.panes)
        .filter(|pane| pane.kind == PaneKind::Agent)
        .flat_map(|pane| {
            std::iter::once((pane, None))
                .chain(pane.children.iter().map(move |child| (pane, Some(child))))
        })
        .filter(|agent| !matches!(agent_status(*agent), PaneStatus::Exited { .. }))
        .collect::<Vec<_>>();

    if agents.is_empty() {
        return None;
    }

    let attention = agents.iter().find_map(|agent| attention_message(*agent));
    let state = if attention.is_some() {
        AgentState::Blocked
    } else if agents
        .iter()
        .any(|agent| agent_status(*agent) == PaneStatus::Working)
    {
        AgentState::Working
    } else {
        AgentState::Idle
    };

    let message = format_message(workspace, attention, group_agents(agents));
    Some(Update::Report {
        state,
        message: Some(message),
    })
}

fn agent_status(agent: (&PaneInfo, Option<&ChildAgentInfo>)) -> PaneStatus {
    agent.1.map_or(agent.0.status, |child| child.status)
}

fn attention_message((pane, child): (&PaneInfo, Option<&ChildAgentInfo>)) -> Option<String> {
    let status = child.map_or(pane.status, |child| child.status);
    status.needs_you().then(|| {
        let name = child.map_or_else(
            || pane.title.clone(),
            |child| format!("{} / {}", pane.title, child.label),
        );
        let note = match child {
            Some(child) => child.note.as_deref(),
            None => pane.note.as_deref(),
        };
        format!("{}: {}", name, note.unwrap_or_else(|| status_label(status)))
    })
}

fn group_agents<'a>(
    agents: Vec<(&'a PaneInfo, Option<&'a ChildAgentInfo>)>,
) -> Vec<(&'a str, Vec<String>)> {
    let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
    for (pane, child) in agents {
        let harness = pane.template.as_deref().unwrap_or("agent");
        let name = child.map_or_else(
            || pane.title.clone(),
            |child| format!("{} / {}", pane.title, child.label),
        );
        let status = child.map_or(pane.status, |child| child.status);
        let entry = format!("{name} [{}]", status_label(status));
        if let Some((_, panes)) = groups.iter_mut().find(|(name, _)| *name == harness) {
            panes.push(entry);
        } else {
            groups.push((harness, vec![entry]));
        }
    }
    groups
}

fn format_message(
    workspace: &str,
    attention: Option<String>,
    groups: Vec<(&str, Vec<String>)>,
) -> String {
    let mut parts = Vec::new();
    if !workspace.is_empty() {
        parts.push(workspace.to_string());
    }
    if let Some(attention) = attention {
        parts.push(attention);
    }
    parts.extend(
        groups
            .into_iter()
            .map(|(harness, panes)| format!("{harness}: {}", panes.join(", "))),
    );
    truncate_message(parts.join(" | "))
}

fn status_label(status: PaneStatus) -> &'static str {
    if status == PaneStatus::Idle {
        "idle"
    } else if status == PaneStatus::Working {
        "working"
    } else if status == PaneStatus::Waiting {
        "waiting"
    } else {
        finished_status_label(status)
    }
}

fn finished_status_label(status: PaneStatus) -> &'static str {
    if status == PaneStatus::NeedsReview {
        "review"
    } else if status == PaneStatus::Done {
        "done"
    } else if status == PaneStatus::Failed {
        "failed"
    } else {
        debug_assert!(matches!(status, PaneStatus::Exited { .. }));
        "exited"
    }
}

fn truncate_message(message: String) -> String {
    if message.chars().count() <= MAX_MESSAGE_CHARS {
        return message;
    }

    message
        .chars()
        .take(MAX_MESSAGE_CHARS - 3)
        .chain("...".chars())
        .collect()
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

    pub fn update(&mut self, tree: &[ProjectInfo], workspace: &str) {
        if let Some(update) = self.sync.next_update(tree, workspace) {
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
        CheckoutId, CheckoutInfo, ChildAgentInfo, PaneId, PaneInfo, PaneKind, PaneStatus,
        ProjectId, ProjectInfo, RepositoryId, RepositoryInfo,
    };

    use super::{AgentState, HerdrSync, Update};

    fn tree(statuses: &[PaneStatus]) -> Vec<ProjectInfo> {
        vec![ProjectInfo {
            id: ProjectId(1),
            name: "project".into(),
            repositories: vec![RepositoryInfo {
                id: RepositoryId(2),
                name: "repo".into(),
                branches: Vec::new(),
                default_branch: None,
                remote_branches: Vec::new(),
                checkouts: vec![CheckoutInfo {
                    id: CheckoutId(3),
                    name: "checkout".into(),
                    path: "/repo".into(),
                    panes: statuses
                        .iter()
                        .enumerate()
                        .map(|(i, status)| PaneInfo {
                            id: PaneId(i as u64 + 4),
                            kind: PaneKind::Agent,
                            title: format!("agent-{i}"),
                            status: *status,
                            note: None,
                            template: Some("opencode".into()),
                            children: Vec::new(),
                        })
                        .collect(),
                    git: None,
                    primary: true,
                }],
            }],
        }]
    }

    #[test]
    fn a_reopened_client_reports_agents_that_are_already_running() {
        let mut sync = HerdrSync::default();

        assert_eq!(
            sync.next_update(&tree(&[PaneStatus::Idle]), "work"),
            Some(Update::Report {
                state: AgentState::Idle,
                message: Some("work | opencode: agent-0 [idle]".into()),
            })
        );
    }

    #[test]
    fn the_most_urgent_agent_state_is_reported() {
        let mut sync = HerdrSync::default();

        assert_eq!(
            sync.next_update(
                &tree(&[
                    PaneStatus::Idle,
                    PaneStatus::Working,
                    PaneStatus::Waiting,
                ]),
                "work",
            ),
            Some(Update::Report {
                state: AgentState::Blocked,
                message: Some(
                    "work | agent-2: waiting | opencode: agent-0 [idle], agent-1 [working], agent-2 [waiting]"
                        .into(),
                ),
            })
        );
    }

    #[test]
    fn review_is_blocked_and_done_is_idle_in_herdr() {
        let mut review = HerdrSync::default();
        assert_eq!(
            review.next_update(&tree(&[PaneStatus::NeedsReview]), "work"),
            Some(Update::Report {
                state: AgentState::Blocked,
                message: Some("work | agent-0: review | opencode: agent-0 [review]".into()),
            })
        );

        let mut done = HerdrSync::default();
        assert_eq!(
            done.next_update(&tree(&[PaneStatus::Done]), "work"),
            Some(Update::Report {
                state: AgentState::Idle,
                message: Some("work | opencode: agent-0 [done]".into()),
            })
        );
    }

    #[test]
    fn aggregation_includes_agents_from_every_repository() {
        let mut agents = tree(&[PaneStatus::Working]);
        agents[0].repositories.push(RepositoryInfo {
            id: RepositoryId(20),
            name: "second".into(),
            branches: Vec::new(),
            default_branch: None,
            remote_branches: Vec::new(),
            checkouts: vec![CheckoutInfo {
                id: CheckoutId(21),
                name: "main".into(),
                path: "/second".into(),
                panes: vec![PaneInfo {
                    id: PaneId(22),
                    kind: PaneKind::Agent,
                    title: "blocked elsewhere".into(),
                    status: PaneStatus::Waiting,
                    note: None,
                    template: Some("opencode".into()),
                    children: Vec::new(),
                }],
                git: None,
                primary: true,
            }],
        });
        let mut sync = HerdrSync::default();

        assert_eq!(
            sync.next_update(&agents, "work"),
            Some(Update::Report {
                state: AgentState::Blocked,
                message: Some(
                    "work | blocked elsewhere: waiting | opencode: agent-0 [working], blocked elsewhere [waiting]"
                        .into(),
                ),
            })
        );
    }

    #[test]
    fn child_statuses_affect_the_aggregate_and_name_the_parent_pane() {
        let mut agents = tree(&[PaneStatus::Idle]);
        agents[0].repositories[0].checkouts[0].panes[0].children = vec![ChildAgentInfo {
            label: "database migration".into(),
            status: PaneStatus::Waiting,
            note: Some("needs production approval".into()),
        }];

        assert_eq!(
            HerdrSync::default().next_update(&agents, "work"),
            Some(Update::Report {
                state: AgentState::Blocked,
                message: Some(
                    "work | agent-0 / database migration: needs production approval | opencode: agent-0 [idle], agent-0 / database migration [waiting]"
                        .into(),
                ),
            })
        );
    }

    #[test]
    fn unchanged_trees_do_not_repeat_reports() {
        let agents = tree(&[PaneStatus::Working]);
        let mut sync = HerdrSync::default();

        assert!(sync.next_update(&agents, "work").is_some());
        assert_eq!(sync.next_update(&agents, "work"), None);
    }

    #[test]
    fn the_message_groups_named_agents_by_harness() {
        let mut agents = tree(&[PaneStatus::Working, PaneStatus::Idle, PaneStatus::Failed]);
        let panes = &mut agents[0].repositories[0].checkouts[0].panes;
        panes[0].title = "auth".into();
        panes[1].title = "tests".into();
        panes[2].title = "review".into();
        panes[2].template = Some("codex".into());
        panes[2].note = Some("cargo test failed".into());

        assert_eq!(
            HerdrSync::default().next_update(&agents, "platform"),
            Some(Update::Report {
                state: AgentState::Blocked,
                message: Some(
                    "platform | review: cargo test failed | opencode: auth [working], tests [idle] | codex: review [failed]"
                        .into(),
                ),
            })
        );
    }

    #[test]
    fn long_unicode_messages_are_safely_bounded() {
        let workspace = "fold-".repeat(60) + "脑";
        let Some(Update::Report { message, .. }) =
            HerdrSync::default().next_update(&tree(&[PaneStatus::Idle]), &workspace)
        else {
            panic!("a live agent should be reported");
        };
        let message = message.unwrap();

        assert_eq!(message.chars().count(), super::MAX_MESSAGE_CHARS);
        assert!(message.ends_with("..."));
    }

    #[test]
    fn exited_agents_are_omitted_from_the_aggregate() {
        assert_eq!(
            HerdrSync::default().next_update(
                &tree(&[PaneStatus::Exited { code: Some(0) }, PaneStatus::Working,]),
                "work",
            ),
            Some(Update::Report {
                state: AgentState::Working,
                message: Some("work | opencode: agent-1 [working]".into()),
            })
        );
    }

    #[test]
    fn losing_the_last_live_agent_releases_argus_from_herdr() {
        let mut sync = HerdrSync::default();
        sync.next_update(&tree(&[PaneStatus::Idle]), "work");

        assert_eq!(sync.next_update(&tree(&[]), "work"), Some(Update::Release));
        assert_eq!(sync.next_update(&tree(&[]), "work"), None);
    }
}
