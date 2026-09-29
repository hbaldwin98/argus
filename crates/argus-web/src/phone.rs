//! What the page and the server say to each other: one JSON message per
//! WebSocket frame, tagged by `type`.
//!
//! Its own contract rather than the daemon's protocol carried whole: the
//! page is served by the binary that speaks this, so the two cannot drift,
//! and a phone on a slow link needs its list and its conversation, not
//! every grid and board the daemon sends a terminal. Reply text arrives
//! here already rendered to HTML that runs nothing ([`crate::markdown`]);
//! every other string is text, for the page to set as text.

use argus_protocol::{
    Body, Earlier, PaneInfo, PaneKind, PaneStatus, Sent, ToolState, Update, WorkspaceTree,
};
use serde::{Deserialize, Serialize};

use crate::markdown;

/// What the server tells the page.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToPhone {
    /// Sent first, and again whenever the daemon connection changes.
    Server {
        version: String,
        /// Whether the daemon is connected right now.
        connected: bool,
        /// The daemon's version, when it greeted and differs from this
        /// build's: what to restart to make them match.
        daemon_version: Option<String>,
    },
    /// Every agent the daemon runs, whole, after any change.
    Agents { workspaces: Vec<Workspace> },
    /// Updates to a watched conversation. `fresh` replaces what is held.
    Transcript {
        pane: u64,
        fresh: bool,
        earlier: Option<Earlier>,
        updates: Vec<PhoneUpdate>,
    },
    /// What came before `before`, to go ahead of what is held.
    Earlier {
        pane: u64,
        before: Earlier,
        earlier: Option<Earlier>,
        updates: Vec<PhoneUpdate>,
    },
    /// What became of a message this page sent.
    Sent {
        pane: u64,
        /// `typed`, `queued` or `refused`.
        outcome: &'static str,
        reason: Option<String>,
    },
    /// A watched pane's screen: whole when `fresh`, else the rows that
    /// changed.
    Screen {
        pane: u64,
        #[serde(flatten)]
        update: crate::screen::ScreenUpdate,
    },
    /// A message this page queued that the daemon no longer holds and never
    /// said it typed: it restarted, and its queue went with it.
    NotSent { pane: u64, text: String },
    Error { message: String },
}

/// What the page asks of the server.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromPhone {
    Watch { pane: u64 },
    Unwatch { pane: u64 },
    Earlier { pane: u64, before: Earlier },
    /// Say `text` to the agent; `now` types it even mid-turn.
    Send { pane: u64, text: String, now: bool },
    /// Take back a message still queued.
    Cancel { pane: u64, id: u64 },
    /// Interrupt the agent.
    Stop { pane: u64 },
    /// Start or stop watching a pane's screen.
    Screen { pane: u64 },
    Unscreen { pane: u64 },
    /// One key from the key bar, by name, straight to the pane.
    Key { pane: u64, key: String },
}

/// A daemon's answer to a message, as the page reads it.
pub fn sent(pane: u64, sent: Sent) -> ToPhone {
    let (outcome, reason) = match sent {
        Sent::Typed => ("typed", None),
        Sent::Queued { .. } => ("queued", None),
        Sent::Refused { reason } => ("refused", Some(reason)),
    };
    ToPhone::Sent {
        pane,
        outcome,
        reason,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Workspace {
    pub name: String,
    pub open: bool,
    pub agents: Vec<Agent>,
}

/// One agent pane, as a row on the phone.
#[derive(Debug, Clone, Serialize)]
pub struct Agent {
    pub pane: u64,
    pub title: String,
    pub template: Option<String>,
    pub project: String,
    pub checkout: String,
    /// The pane's own status, in the pane API's words.
    pub status: &'static str,
    pub note: Option<String>,
    /// The most urgent state among the pane and its children, which is
    /// what the row leads with, and who it belongs to when not the pane.
    pub loudest: &'static str,
    pub loudest_child: Option<String>,
    pub since: Option<u64>,
    pub has_transcript: bool,
    pub model: Option<String>,
    pub tool: Option<String>,
    /// Messages waiting for the agent's prompt, oldest first.
    pub queued: Vec<Queued>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Queued {
    pub id: u64,
    pub text: String,
}

/// One change to the page's copy of a conversation.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PhoneUpdate {
    Upsert { entry: PhoneEntry },
    Append { id: String, delta: String },
    Tool { id: String, state: &'static str },
}

/// An entry, flattened for a page that switches on `kind`. Only the fields
/// its kind uses are present.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PhoneEntry {
    pub id: String,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A reply, rendered. Set as markup; everything else is set as text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub millis: Option<u64>,
}

/// The pane API's word for a status, which is also what the page's styles
/// are named after.
pub fn status_word(status: PaneStatus) -> &'static str {
    match status {
        PaneStatus::Idle => "idle",
        PaneStatus::Working => "working",
        PaneStatus::Waiting => "waiting",
        PaneStatus::NeedsReview => "needs-review",
        PaneStatus::Done => "done",
        PaneStatus::Failed => "failed",
        PaneStatus::Exited { .. } => "exited",
    }
}

fn tool_word(state: ToolState) -> &'static str {
    match state {
        ToolState::Running => "running",
        ToolState::Done => "done",
        ToolState::Failed => "failed",
    }
}

/// Every agent in every workspace, the most urgent first in each, with the
/// order the daemon lists them in breaking ties.
pub fn agents(tree: &[WorkspaceTree]) -> Vec<Workspace> {
    tree.iter()
        .map(|workspace| {
            let mut agents: Vec<(u8, Agent)> = workspace
                .projects
                .iter()
                .flat_map(|project| {
                    project.repositories.iter().flat_map(move |repository| {
                        repository.checkouts.iter().flat_map(move |checkout| {
                            checkout
                                .panes
                                .iter()
                                .filter(|pane| pane.kind == PaneKind::Agent)
                                .map(move |pane| agent(&project.name, &checkout.name, pane))
                        })
                    })
                })
                .collect();
            agents.sort_by(|(a, _), (b, _)| b.cmp(a));
            Workspace {
                name: workspace.name.clone(),
                open: workspace.open,
                agents: agents.into_iter().map(|(_, agent)| agent).collect(),
            }
        })
        .collect()
}

fn agent(project: &str, checkout: &str, pane: &PaneInfo) -> (u8, Agent) {
    let loudest = pane.loudest_state();
    let row = Agent {
        pane: pane.id.0,
        title: pane.title.clone(),
        template: pane.template.clone(),
        project: project.to_string(),
        checkout: checkout.to_string(),
        status: status_word(pane.status),
        note: pane.note.clone(),
        loudest: status_word(loudest.status),
        loudest_child: loudest.child.map(str::to_string),
        since: pane.since,
        has_transcript: pane.has_transcript,
        model: pane.telemetry.model.clone(),
        tool: pane.telemetry.tool.clone(),
        queued: pane
            .queued
            .iter()
            .map(|m| Queued {
                id: m.id,
                text: m.text.clone(),
            })
            .collect(),
    };
    (loudest.status.urgency(), row)
}

/// A daemon update, as the page reads it.
pub fn update(update: Update) -> PhoneUpdate {
    match update {
        Update::Upsert(entry) => {
            let mut out = PhoneEntry {
                id: entry.id,
                at: entry.at,
                ..Default::default()
            };
            match entry.body {
                Body::Prompt { text } => {
                    out.kind = "prompt";
                    out.text = Some(text);
                }
                Body::Reply { text } => {
                    out.kind = "reply";
                    out.html = Some(markdown::to_html(&text));
                }
                Body::Thinking { text } => {
                    out.kind = "thinking";
                    out.text = Some(text);
                }
                Body::ToolCall {
                    tool,
                    summary,
                    input,
                    state,
                } => {
                    out.kind = "tool_call";
                    out.tool = Some(tool);
                    out.summary = Some(summary);
                    out.input = Some(input);
                    out.state = Some(tool_word(state));
                }
                Body::ToolResult {
                    call,
                    output,
                    failed,
                } => {
                    out.kind = "tool_result";
                    out.call = Some(call);
                    out.text = Some(output);
                    out.failed = Some(failed);
                }
                Body::Notice { text } => {
                    out.kind = "notice";
                    out.text = Some(text);
                }
                Body::TurnEnd { millis } => {
                    out.kind = "turn_end";
                    out.millis = millis;
                }
                Body::Divider { text } => {
                    out.kind = "divider";
                    out.text = Some(text);
                }
            }
            PhoneUpdate::Upsert { entry: out }
        }
        Update::AppendText { id, delta } => PhoneUpdate::Append { id, delta },
        Update::ToolState { id, state } => PhoneUpdate::Tool {
            id,
            state: tool_word(state),
        },
    }
}

pub fn updates(updates: Vec<Update>) -> Vec<PhoneUpdate> {
    updates.into_iter().map(update).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{CheckoutInfo, Entry, PaneId, ProjectInfo, RepositoryInfo, WorkspaceId};

    fn pane(id: u64, kind: PaneKind, status: PaneStatus) -> PaneInfo {
        PaneInfo {
            id: PaneId(id),
            kind,
            title: format!("pane {id}"),
            status,
            note: None,
            template: Some("claude".into()),
            children: Vec::new(),
            telemetry: Default::default(),
            has_transcript: true,
            since: Some(1_790_000_000),
            queued: Vec::new(),
        }
    }

    fn workspace(panes: Vec<PaneInfo>) -> WorkspaceTree {
        WorkspaceTree {
            id: WorkspaceId(0),
            name: "default".into(),
            open: true,
            projects: vec![ProjectInfo {
                id: argus_protocol::ProjectId(0),
                name: "argus".into(),
                root: None,
                repositories: vec![RepositoryInfo {
                    id: argus_protocol::RepositoryId(0),
                    name: "argus".into(),
                    branches: Vec::new(),
                    default_branch: None,
                    remote_branches: Vec::new(),
                    checkouts: vec![CheckoutInfo {
                        id: argus_protocol::CheckoutId(0),
                        name: "main".into(),
                        path: "/repo".into(),
                        panes,
                        git: None,
                        primary: true,
                    }],
                }],
            }],
        }
    }

    #[test]
    fn agents_are_listed_most_urgent_first_and_shells_are_not() {
        let tree = [workspace(vec![
            pane(1, PaneKind::Agent, PaneStatus::Working),
            pane(2, PaneKind::Shell, PaneStatus::Idle),
            pane(3, PaneKind::Agent, PaneStatus::Waiting),
            pane(4, PaneKind::Agent, PaneStatus::Idle),
        ])];
        let rows = agents(&tree);
        let order: Vec<u64> = rows[0].agents.iter().map(|a| a.pane).collect();
        assert_eq!(order, [3, 1, 4]);
        assert_eq!(rows[0].agents[0].status, "waiting");
        assert_eq!(rows[0].agents[0].checkout, "main");
    }

    #[test]
    fn a_reply_arrives_rendered_and_a_prompt_as_text() {
        let reply = update(Update::Upsert(Entry {
            id: "1".into(),
            at: None,
            body: Body::Reply {
                text: "**done** <b>x</b>".into(),
            },
        }));
        let json = serde_json::to_value(&reply).unwrap();
        assert_eq!(json["entry"]["kind"], "reply");
        assert!(json["entry"]["html"].as_str().unwrap().contains("<strong>done</strong>"));
        assert!(json["entry"]["html"].as_str().unwrap().contains("&lt;b&gt;"));
        assert!(json["entry"].get("text").is_none());

        let prompt = update(Update::Upsert(Entry {
            id: "2".into(),
            at: None,
            body: Body::Prompt {
                text: "<b>raw</b>".into(),
            },
        }));
        let json = serde_json::to_value(&prompt).unwrap();
        assert_eq!(json["entry"]["text"], "<b>raw</b>");
        assert!(json["entry"].get("html").is_none());
    }

    #[test]
    fn the_page_asks_in_tagged_json() {
        let ask: FromPhone = serde_json::from_str(r#"{"type":"watch","pane":7}"#).unwrap();
        assert_eq!(ask, FromPhone::Watch { pane: 7 });
        let ask: FromPhone =
            serde_json::from_str(r#"{"type":"earlier","pane":7,"before":{"file":0,"offset":99}}"#)
                .unwrap();
        assert_eq!(
            ask,
            FromPhone::Earlier {
                pane: 7,
                before: Earlier { file: 0, offset: 99 }
            }
        );
        let ask: FromPhone =
            serde_json::from_str(r#"{"type":"send","pane":7,"text":"hi","now":false}"#).unwrap();
        assert_eq!(
            ask,
            FromPhone::Send {
                pane: 7,
                text: "hi".into(),
                now: false
            }
        );
    }

    #[test]
    fn a_refusal_says_why() {
        let json = serde_json::to_value(sent(
            3,
            Sent::Refused {
                reason: "that is not a running agent".into(),
            },
        ))
        .unwrap();
        assert_eq!(json["type"], "sent");
        assert_eq!(json["outcome"], "refused");
        assert_eq!(json["reason"], "that is not a running agent");
    }
}
