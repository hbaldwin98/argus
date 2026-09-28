//! The tree a client renders: projects, their repositories, each
//! repository's checkouts and branches, and the panes running in them.
//!
//! Sent whole on every structural change rather than as a patch — it is
//! small, and a client that can only ever be handed the current truth has
//! no stale-state bugs to have.

use serde::{Deserialize, Serialize};

use crate::ids::{CheckoutId, PaneId, ProjectId, RepositoryId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneKind {
    Shell,
    Agent,
    /// The user's own editor, opened on a file from the review (§6).
    Editor,
}

/// See DESIGN.md §8b. Everything but `Exited` comes from the agent itself,
/// through whatever hook mechanism its harness supports (§11); a harness
/// that reports nothing sits at `Idle` until it `Exited` — coarse, but
/// that's the accepted fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneStatus {
    Idle,
    Working,
    /// Stopped, needing a human. [`PaneInfo::note`] says what for.
    Waiting,
    /// Work is ready for the operator to inspect.
    NeedsReview,
    /// Work is finished and has been reviewed.
    Done,
    /// Still running, but something went wrong and the agent said so.
    /// Distinct from `Exited`: the process is alive, so the row is worth
    /// going to rather than worth closing.
    Failed,
    Exited {
        code: Option<i32>,
    },
}

impl PaneStatus {
    /// Whether this row is stalled on a human. What the eye should land on
    /// first when scanning a column of agents.
    pub fn needs_you(self) -> bool {
        matches!(
            self,
            PaneStatus::Waiting | PaneStatus::NeedsReview | PaneStatus::Failed
        )
    }

    /// An order, not a scale: a row standing for several agents shows the
    /// largest state beneath it (DESIGN.md §8b). Here rather than in the
    /// client because two clients disagreeing on which agent is the more
    /// urgent would be two different products.
    pub fn urgency(self) -> u8 {
        match self {
            PaneStatus::Exited { code: Some(0) } => 0,
            PaneStatus::Idle => 1,
            PaneStatus::Done => 2,
            PaneStatus::Working => 3,
            PaneStatus::Exited { .. } => 4,
            PaneStatus::NeedsReview => 5,
            PaneStatus::Failed => 6,
            PaneStatus::Waiting => 7,
        }
    }

    /// What a row standing for several states shows: the most urgent of
    /// them, the first on a tie. `None` when there are none.
    pub fn loudest(statuses: impl IntoIterator<Item = PaneStatus>) -> Option<PaneStatus> {
        loudest_by(statuses, |status| *status)
    }
}

fn loudest_by<T>(items: impl IntoIterator<Item = T>, status: impl Fn(&T) -> PaneStatus) -> Option<T> {
    items.into_iter().reduce(|loudest, next| {
        if status(&next).urgency() > status(&loudest).urgency() {
            next
        } else {
            loudest
        }
    })
}

/// One state a pane row stands for: the pane's own, or a child agent's
/// reported through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneState<'a> {
    pub status: PaneStatus,
    /// The child's label, or `None` for the pane itself.
    pub child: Option<&'a str>,
    pub note: Option<&'a str>,
}

impl PaneState<'_> {
    /// [`PaneStatus::loudest`], keeping which agent it came from.
    pub fn loudest<'a>(states: impl IntoIterator<Item = PaneState<'a>>) -> Option<PaneState<'a>> {
        loudest_by(states, |state| state.status)
    }
}

impl PaneInfo {
    /// Every state this row stands for: the pane's own, then each child's
    /// in the order they first reported. A child is an agent in its own
    /// right — one of them waiting is a person being waited on — but the
    /// parent's row is the only place it can be gone to.
    pub fn states(&self) -> impl Iterator<Item = PaneState<'_>> {
        std::iter::once(self.own_state()).chain(self.children.iter().map(|child| PaneState {
            status: child.status,
            child: Some(child.label.as_str()),
            note: child.note.as_deref(),
        }))
    }

    /// The state the row speaks with: the most urgent of [`Self::states`],
    /// the pane's own on a tie.
    pub fn loudest_state(&self) -> PaneState<'_> {
        PaneState::loudest(self.states()).unwrap_or_else(|| self.own_state())
    }

    fn own_state(&self) -> PaneState<'_> {
        PaneState {
            status: self.status,
            child: None,
            note: self.note.as_deref(),
        }
    }
}

/// Another agent reporting through a pane it does not own — a CLI spawned
/// from inside the pane's own agent, which inherits the hook environment and
/// so would otherwise rewrite the row belonging to its parent. Listed under
/// the parent, and never allowed to change it: see DESIGN.md §8b.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildAgentInfo {
    /// What the child called itself, or a generic word until it says.
    pub label: String,
    pub status: PaneStatus,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneInfo {
    pub id: PaneId,
    pub kind: PaneKind,
    pub title: String,
    pub status: PaneStatus,
    /// One line from the agent about its current state — the question it is
    /// blocked on, or what failed. The point of a status column is knowing
    /// whether to go somewhere; the point of this is knowing why, without
    /// having to.
    #[serde(default)]
    pub note: Option<String>,
    /// The agent template this pane runs, for an agent pane. `title` starts
    /// as this and then becomes whatever the agent renames itself to, so
    /// without carrying it separately a renamed row stops saying which CLI
    /// is in it — which is exactly what you want to know when several are
    /// running side by side.
    #[serde(default)]
    pub template: Option<String>,
    /// Agents running underneath this one, in the order they first
    /// reported. Read-only: the pane's own status and title stay the
    /// parent's to set.
    #[serde(default)]
    pub children: Vec<ChildAgentInfo>,
    /// What the agent last said about its model, context, spend and the
    /// tool it is running. Every field is optional because each harness
    /// exposes a different subset of them.
    #[serde(default)]
    pub telemetry: AgentTelemetry,
    // No transcript. It rode here once, up to 200 events a pane in every
    // tree every client was sent, and no client read it; the daemon keeps
    // it to itself now. A daemon from before still sends the field, and
    // the derive ignores a field it does not know.
}

/// Harness-neutral telemetry for one agent pane.
///
/// The same shape travels in both directions: an adapter POSTs a partial
/// one to the pane API, where each field it sets replaces the last, and the
/// daemon hands the merged result to clients. Claude Code and Codex are
/// read from their hook payloads and transcripts, opencode and pi report
/// from their plugins, and any other agent can send it with
/// `argus-hook telemetry`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentTelemetry {
    #[serde(default)]
    pub model: Option<String>,
    /// Tokens the conversation currently occupies in the model's context.
    #[serde(default)]
    pub context_tokens: Option<u64>,
    /// The model's context size, when the harness knows it.
    #[serde(default)]
    pub context_window: Option<u64>,
    /// Cumulative tokens across the session.
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    /// Cumulative spend in US dollars, for harnesses that price their calls.
    #[serde(default)]
    pub cost_usd: Option<f64>,
    /// The tool running right now. In a report, an empty name says the
    /// tool finished.
    #[serde(default)]
    pub tool: Option<String>,
    /// Tool calls started in this session.
    #[serde(default)]
    pub tool_calls: Option<u64>,
}

impl AgentTelemetry {
    pub fn is_empty(&self) -> bool {
        *self == AgentTelemetry::default()
    }

    /// Applies a partial report. Returns whether anything changed.
    ///
    /// A tool start that does not carry its own count is counted here, so
    /// adapters that only see individual tool events still get a total.
    pub fn merge(&mut self, report: AgentTelemetry) -> bool {
        let before = self.clone();
        if report
            .model
            .as_deref()
            .is_some_and(|m| !m.trim().is_empty())
        {
            self.model = report.model.map(|m| m.trim().to_string());
        }
        for (field, value) in [
            (&mut self.context_tokens, report.context_tokens),
            (&mut self.context_window, report.context_window),
            (&mut self.input_tokens, report.input_tokens),
            (&mut self.output_tokens, report.output_tokens),
        ] {
            if value.is_some() {
                *field = value;
            }
        }
        if report.cost_usd.is_some_and(|c| c.is_finite() && c >= 0.0) {
            self.cost_usd = report.cost_usd;
        }
        match report.tool.as_deref().map(str::trim) {
            Some("") => self.tool = None,
            Some(name) => {
                if report.tool_calls.is_none() {
                    self.tool_calls = Some(self.tool_calls.unwrap_or(0) + 1);
                }
                self.tool = Some(name.to_string());
            }
            None => {}
        }
        if report.tool_calls.is_some() {
            self.tool_calls = report.tool_calls;
        }
        *self != before
    }
}

/// Read-only git status for a checkout, polled from the working directory
/// (see DESIGN.md §4 Level 2). `None` on `CheckoutInfo::git` means the path
/// isn't a git repo at all, not that status is merely unknown yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatus {
    /// `None` for a detached HEAD.
    pub branch: Option<String>,
    pub dirty: bool,
    pub changed_files: usize,
    /// Commits ahead/behind the upstream tracking branch; both 0 if there is
    /// no upstream configured.
    pub ahead: usize,
    pub behind: usize,
}

impl CheckoutInfo {
    /// The panes the tree lists. An editor belongs to the window it opened
    /// in, not to the checkout's pane list — it is a way of looking at a
    /// file, not something running here that you might come back to.
    pub fn listed_panes(&self) -> impl Iterator<Item = &PaneInfo> {
        self.panes.iter().filter(|p| p.kind != PaneKind::Editor)
    }

    /// Every state the checkout's rows stand for, children included: what
    /// a checkout, a repository or a whole fleet rolls up.
    pub fn statuses(&self) -> impl Iterator<Item = PaneStatus> + '_ {
        self.listed_panes()
            .flat_map(|pane| pane.states().map(|state| state.status))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutInfo {
    pub id: CheckoutId,
    pub name: String,
    pub path: String,
    pub panes: Vec<PaneInfo>,
    pub git: Option<GitStatus>,
    /// True for a repo's original working directory (as configured), false
    /// for a linked worktree Argus created. The primary checkout can't be
    /// removed — see DESIGN.md §4 Level 2.
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryInfo {
    pub id: RepositoryId,
    pub name: String,
    pub checkouts: Vec<CheckoutInfo>,
    /// Local branches no checkout of this repository is sitting on, sorted.
    /// The client decides which of them get rows — all of them only while
    /// the column is expanded, since a long branch list buries the
    /// checkouts that are the point of the column.
    #[serde(default)]
    pub branches: Vec<String>,
    /// The repository's main line of development, whatever it is named:
    /// `origin/HEAD` where the remote says, a conventional name where it
    /// doesn't. It leads the checkouts column whether or not anything is
    /// sitting on it, so that "how far is this from main" has a fixed place
    /// to be asked from.
    #[serde(default)]
    pub default_branch: Option<String>,
    /// Remote-tracking branches with no local branch of the same name, as
    /// `origin/feature`. What a fetch turns up: work that exists but isn't
    /// here yet, and can be had by switching to it or giving it a worktree.
    #[serde(default)]
    pub remote_branches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: ProjectId,
    pub name: String,
    /// The directory this project scans for repositories, when it has one.
    #[serde(default)]
    pub root: Option<String>,
    pub repositories: Vec<RepositoryInfo>,
}

/// A named group of projects. Exactly one workspace is *open* at a time —
/// daemon-global state, broadcast to every client — and the project tree a
/// client sees is scoped to it. Other workspaces' panes keep running in the
/// background; they are simply not shown.
///
/// Deliberately not a fourth navigation column: the left-to-right spine is
/// project → checkout → pane (DESIGN.md §4), and workspaces sit *above*
/// that as a scope switch rather than another step along it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: WorkspaceId,
    pub name: String,
    /// How many projects this workspace holds, so the picker can show it
    /// without the client having to hold every workspace's tree.
    pub projects: usize,
    /// Live panes across the whole workspace, open or not — the reason to
    /// show this at all is spotting an agent still working somewhere you
    /// are not currently looking.
    pub panes: usize,
    pub open: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(status: PaneStatus, children: &[PaneStatus]) -> PaneInfo {
        PaneInfo {
            id: PaneId(1),
            kind: PaneKind::Agent,
            title: "claude".into(),
            status,
            note: None,
            template: None,
            children: children
                .iter()
                .enumerate()
                .map(|(i, &status)| ChildAgentInfo {
                    label: format!("child {i}"),
                    status,
                    note: None,
                })
                .collect(),
            telemetry: AgentTelemetry::default(),
        }
    }

    fn checkout(panes: Vec<PaneInfo>) -> CheckoutInfo {
        CheckoutInfo {
            id: CheckoutId(1),
            name: "main".into(),
            path: "/repo".into(),
            panes,
            git: None,
            primary: true,
        }
    }

    #[test]
    fn the_loudest_of_several_states_is_the_most_urgent() {
        use PaneStatus::*;
        let cases: &[(&[PaneStatus], Option<PaneStatus>)] = &[
            (&[], None),
            (&[Idle], Some(Idle)),
            (&[Working, Idle, Done], Some(Working)),
            // A failed exit is news; a working agent beside it is not.
            (&[Working, Exited { code: Some(1) }], Some(Exited { code: Some(1) })),
            // A kill has no exit code, and is no calmer for it.
            (&[Working, Exited { code: None }], Some(Exited { code: None })),
            // Failed is still running, so it is worth going to; an exit is not.
            (&[Exited { code: Some(1) }, Failed], Some(Failed)),
            (&[Working, Failed], Some(Failed)),
            (&[Exited { code: Some(0) }, Idle], Some(Idle)),
            (&[NeedsReview, Failed, Waiting], Some(Waiting)),
            (&[NeedsReview, Failed], Some(Failed)),
            (&[Working, NeedsReview], Some(NeedsReview)),
        ];
        for (statuses, loudest) in cases {
            assert_eq!(
                PaneStatus::loudest(statuses.iter().copied()),
                *loudest,
                "{statuses:?}"
            );
        }
    }

    #[test]
    fn a_tie_keeps_the_first_state() {
        let failed = PaneStatus::Exited { code: Some(1) };
        let killed = PaneStatus::Exited { code: None };
        assert_eq!(PaneStatus::loudest([failed, killed]), Some(failed));
        assert_eq!(PaneStatus::loudest([killed, failed]), Some(killed));
    }

    #[test]
    fn a_waiting_child_speaks_for_its_working_parent() {
        let mut p = pane(PaneStatus::Working, &[PaneStatus::Idle, PaneStatus::Waiting]);
        p.children[1].note = Some("approve rm".into());
        assert_eq!(
            p.loudest_state(),
            PaneState {
                status: PaneStatus::Waiting,
                child: Some("child 1"),
                note: Some("approve rm"),
            }
        );
    }

    #[test]
    fn a_child_as_loud_as_its_parent_does_not_take_the_row() {
        let p = pane(PaneStatus::Waiting, &[PaneStatus::Waiting]);
        assert_eq!(p.loudest_state().child, None);
    }

    #[test]
    fn a_checkout_rolls_up_every_pane_and_child_it_lists() {
        let mut editor = pane(PaneStatus::Waiting, &[]);
        editor.kind = PaneKind::Editor;
        let c = checkout(vec![
            pane(PaneStatus::Working, &[PaneStatus::Waiting]),
            pane(PaneStatus::Idle, &[]),
            editor,
        ]);
        assert_eq!(
            c.statuses().collect::<Vec<_>>(),
            [PaneStatus::Working, PaneStatus::Waiting, PaneStatus::Idle],
            "the editor is not listed, so it is not rolled up"
        );
        assert_eq!(PaneStatus::loudest(c.statuses()), Some(PaneStatus::Waiting));
        assert_eq!(PaneStatus::loudest(checkout(Vec::new()).statuses()), None);
    }

    #[test]
    fn a_pane_from_a_daemon_that_still_sends_its_transcript_reads() {
        // An older daemon puts the transcript in every pane. Dropping the
        // field here must not cost a newer client the tree.
        #[derive(Serialize)]
        struct OlderPaneInfo {
            #[serde(flatten)]
            pane: PaneInfo,
            transcript: Vec<crate::transcript::AgentTranscriptEvent>,
        }
        let older = OlderPaneInfo {
            pane: pane(PaneStatus::Working, &[]),
            transcript: vec![crate::transcript::AgentTranscriptEvent {
                kind: crate::transcript::TranscriptKind::Prompt,
                text: Some("fix the bug".into()),
                tool: None,
            }],
        };

        let bytes = rmp_serde::to_vec_named(&older).unwrap();
        let read: PaneInfo = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(read.status, PaneStatus::Working);
        assert_eq!(read.title, "claude");
    }
}
