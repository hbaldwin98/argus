//! The tree a client renders: projects, their repositories, each
//! repository's checkouts and branches, and the panes running in them.
//!
//! Sent whole on every structural change rather than as a patch — it is
//! small, and a client that can only ever be handed the current truth has
//! no stale-state bugs to have.

pub mod column;
pub mod focus;
pub mod held;
pub mod ids;

use serde::{Deserialize, Serialize};

use crate::tree::ids::{CheckoutId, PaneId, ProjectId, RepositoryId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneKind {
    Shell,
    Agent,
    /// The user's own editor, opened on a file from the review (§6).
    Editor,
    /// An agent's browser, owned by it. Its content is pictures, not a
    /// terminal, so a client that cannot draw them is never sent one.
    Browser,
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

    /// The pane API's word for a status, which is also what `argus focus`
    /// calls it.
    pub fn word(self) -> &'static str {
        match self {
            PaneStatus::Idle => "idle",
            PaneStatus::Working => "working",
            PaneStatus::Waiting => "waiting",
            PaneStatus::NeedsReview => "needs-review",
            PaneStatus::Done => "done",
            PaneStatus::Failed => "failed",
            PaneStatus::Exited { .. } => "exited",
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
    /// Every state this row stands for: the pane's own, the work its agent
    /// left running behind its turn, then each child's in the order they
    /// first reported. A child is an agent in its own right — one of them
    /// waiting is a person being waited on — but the parent's row is the
    /// only place it can be gone to.
    pub fn states(&self) -> impl Iterator<Item = PaneState<'_>> {
        let background = self.background.then_some(PaneState {
            status: PaneStatus::Working,
            child: None,
            note: None,
        });
        let children = self.children.iter().map(|child| PaneState {
            status: child.status,
            child: Some(child.label.as_str()),
            note: child.note.as_deref(),
        });
        std::iter::once(self.own_state()).chain(background).chain(children)
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
    /// Whether the agent's last turn ended with work still running behind
    /// it — a workflow it started. `status` stays the agent's own, since
    /// an agent idle at its prompt can be typed to; the row reads as
    /// working through [`PaneInfo::loudest_state`].
    #[serde(default)]
    pub background: bool,
    /// What the agent last said about its model, context, spend and the
    /// tool it is running. Every field is optional because each harness
    /// exposes a different subset of them.
    #[serde(default)]
    pub telemetry: AgentTelemetry,
    /// When the pane's status last changed, in seconds since the Unix
    /// epoch by the daemon's clock. `None` from a daemon that does not say.
    #[serde(default)]
    pub since: Option<u64>,
    /// Whether a plugin inside the agent holds the pane's inbox open, so
    /// what is said to it goes through the harness rather than being typed.
    #[serde(default)]
    pub live: bool,
    // No transcript. It rode here once, up to 200 events a pane in every
    // tree every client was sent, and no client read it. A daemon from
    // before still sends the field, and the derive ignores a field it does
    // not know. `has_transcript` and `queued` went the same way with
    // conversation following and the outbox; both were read with a
    // default, so a client from before does without them.
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
    /// Paths with any change, staged or not, each counted once.
    pub changed_files: usize,
    /// Paths whose index differs from HEAD. A path both staged and edited
    /// again counts here and in `unstaged`, which is why neither is derived
    /// from `changed_files`.
    #[serde(default)]
    pub staged: usize,
    /// Paths whose working tree differs from the index, untracked included.
    #[serde(default)]
    pub unstaged: usize,
    /// Commits ahead/behind the upstream tracking branch; both 0 if there is
    /// no upstream configured.
    pub ahead: usize,
    pub behind: usize,
    /// The tracking branch, as `origin/main`; `None` when there is none, and
    /// from a daemon that does not say.
    #[serde(default)]
    pub upstream: Option<String>,
    /// The commit HEAD is on: what was last done here. `None` for a
    /// repository with no commits yet, and from a daemon that does not say.
    #[serde(default)]
    pub last_commit: Option<crate::CommitInfo>,
    /// A fingerprint of the working changes, which differs whenever a
    /// review of them could read differently: an edit to a file already
    /// counted changes no count, so a review cannot follow the counts.
    /// Only ever compared; 0 from a daemon that does not say.
    #[serde(default)]
    pub changes: u64,
}

impl GitStatus {
    /// A checkout's git state as every client shows it: `↑` ahead of and
    /// `↓` behind its upstream, `+` staged and `!` unstaged paths, else
    /// `clean`. Built from parts so that being dirty never hides how far the
    /// branch is from its upstream.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.ahead > 0 {
            parts.push(format!("↑{}", self.ahead));
        }
        if self.behind > 0 {
            parts.push(format!("↓{}", self.behind));
        }
        if self.staged > 0 {
            parts.push(format!("+{}", self.staged));
        }
        if self.unstaged > 0 {
            parts.push(format!("!{}", self.unstaged));
        }
        // A daemon from before the staged counts sends only the total.
        if self.dirty && self.staged == 0 && self.unstaged == 0 {
            parts.push(format!("!{}", self.changed_files));
        }
        if !self.dirty {
            parts.push("clean".into());
        }
        parts.join(" ")
    }
}

impl CheckoutInfo {
    /// The panes the tree lists. An editor belongs to the window it opened
    /// in, not to the checkout's pane list — it is a way of looking at a
    /// file, not something running here that you might come back to. A
    /// browser is listed only in the Browsers section, beside the agent
    /// that owns it, and so would be a second row here for one thing.
    pub fn listed_panes(&self) -> impl Iterator<Item = &PaneInfo> {
        self.panes.iter().filter(|p| !matches!(p.kind, PaneKind::Editor | PaneKind::Browser))
    }

    /// Every state the checkout's rows stand for, children included: what
    /// a checkout, a repository or a whole fleet rolls up.
    pub fn statuses(&self) -> impl Iterator<Item = PaneStatus> + '_ {
        self.listed_panes()
            .flat_map(|pane| pane.states().map(|state| state.status))
    }

    /// How many agents share the checkout, when more than one does. Shown
    /// rather than prevented (an `exclusive` project refuses it instead),
    /// since two agents in one directory is never something to find out
    /// from the diff later. Shells do not count, nor an agent's children,
    /// which run inside its own turn, nor an agent that has exited: its
    /// pane stays until closed, but it no longer works in the directory.
    pub fn shared_by(&self) -> Option<usize> {
        let agents = self
            .panes
            .iter()
            .filter(|p| p.kind == PaneKind::Agent && !matches!(p.status, PaneStatus::Exited { .. }))
            .count();
        (agents > 1).then_some(agents)
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
    /// For each of `branches`, how many of its commits the main branch does
    /// not have: zero for one already merged. A branch missing from it has
    /// not been counted — no main branch, or a daemon that does not say.
    #[serde(default)]
    pub unmerged: std::collections::BTreeMap<String, usize>,
    /// When a pane last closed in any of its checkouts, in seconds since the
    /// Unix epoch by the daemon's clock: the moment it last went quiet, which
    /// is what orders the repositories with nothing running. `None` for one
    /// never used, and from a daemon that does not say.
    #[serde(default)]
    pub last_used: Option<u64>,
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

impl ProjectInfo {
    /// The repositories by index as (active, quiet): the active, with a
    /// listed pane in any checkout, in tree order, since what is running is
    /// what a rail is for; then the quiet, the most recently used first so
    /// the one just left is nearest to hand, the never used last. Here so
    /// that every client's rail lists them in one order.
    pub fn repositories_by_activity(&self) -> (Vec<usize>, Vec<usize>) {
        let (active, mut quiet): (Vec<usize>, Vec<usize>) =
            (0..self.repositories.len()).partition(|index| {
                self.repositories[*index]
                    .checkouts
                    .iter()
                    .any(|checkout| checkout.listed_panes().next().is_some())
            });
        quiet.sort_by_key(|index| std::cmp::Reverse(self.repositories[*index].last_used));
        (active, quiet)
    }
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
            background: false,
            since: None,
            live: false,
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
    fn a_checkout_is_shared_by_its_agents_only_when_there_are_several() {
        let shell = PaneInfo { kind: PaneKind::Shell, ..pane(PaneStatus::Idle, &[]) };
        let agent = || pane(PaneStatus::Working, &[PaneStatus::Working, PaneStatus::Working]);
        assert_eq!(checkout(vec![agent(), shell.clone()]).shared_by(), None, "children and shells are not agents of the checkout");
        assert_eq!(checkout(vec![agent(), shell, agent(), agent()]).shared_by(), Some(3));
    }

    #[test]
    fn an_exited_agent_no_longer_shares_its_checkout() {
        let agent = || pane(PaneStatus::Working, &[]);
        let exited = pane(PaneStatus::Exited { code: Some(0) }, &[]);
        assert_eq!(checkout(vec![exited.clone(), agent()]).shared_by(), None);
        assert_eq!(checkout(vec![exited, agent(), agent()]).shared_by(), Some(2));
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
    fn an_idle_pane_with_work_left_running_reads_as_working() {
        let mut p = pane(PaneStatus::Idle, &[]);
        p.background = true;
        assert_eq!(p.loudest_state().status, PaneStatus::Working);
        // Its own status is untouched: the agent is at its prompt.
        assert_eq!(p.status, PaneStatus::Idle);
        // Something louder than working still speaks over it.
        p.status = PaneStatus::Waiting;
        assert_eq!(p.loudest_state().status, PaneStatus::Waiting);
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
    fn a_browser_is_not_listed_under_its_checkout_but_stays_in_its_panes() {
        let mut browser = pane(PaneStatus::Idle, &[]);
        browser.kind = PaneKind::Browser;
        let c = checkout(vec![pane(PaneStatus::Working, &[]), browser]);
        assert_eq!(c.listed_panes().count(), 1, "the Browsers section lists it");
        assert_eq!(c.panes.len(), 2, "the grid still tiles it");
    }

    #[test]
    fn a_pane_from_a_daemon_that_still_sends_its_transcript_reads() {
        // An older daemon puts the transcript in every pane. Dropping the
        // field here must not cost a newer client the tree.
        #[derive(Serialize)]
        struct OlderEvent {
            kind: &'static str,
            text: Option<String>,
            tool: Option<String>,
        }
        #[derive(Serialize)]
        struct OlderPaneInfo {
            #[serde(flatten)]
            pane: PaneInfo,
            transcript: Vec<OlderEvent>,
        }
        let older = OlderPaneInfo {
            pane: pane(PaneStatus::Working, &[]),
            transcript: vec![OlderEvent {
                kind: "prompt",
                text: Some("fix the bug".into()),
                tool: None,
            }],
        };

        let bytes = rmp_serde::to_vec_named(&older).unwrap();
        let read: PaneInfo = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(read.status, PaneStatus::Working);
        assert_eq!(read.title, "claude");
    }

    fn git(dirty: bool, staged: usize, unstaged: usize, ahead: usize, behind: usize) -> GitStatus {
        GitStatus {
            branch: Some("main".into()),
            dirty,
            changed_files: staged + unstaged,
            staged,
            unstaged,
            ahead,
            behind,
            upstream: None,
            last_commit: None,
            changes: 0,
        }
    }

    #[test]
    fn a_git_summary_says_distance_and_changes_together() {
        assert_eq!(git(false, 0, 0, 0, 0).summary(), "clean");
        assert_eq!(git(false, 0, 0, 2, 1).summary(), "↑2 ↓1 clean");
        assert_eq!(git(true, 2, 8, 0, 0).summary(), "+2 !8");
        assert_eq!(git(true, 0, 1, 0, 1).summary(), "↓1 !1");
    }

    #[test]
    fn a_git_summary_from_a_daemon_without_staged_counts_says_the_total() {
        let older = GitStatus {
            changed_files: 3,
            ..git(true, 0, 0, 0, 0)
        };
        assert_eq!(older.summary(), "!3");
    }

    fn repository(name: &str, panes: usize, last_used: Option<u64>) -> RepositoryInfo {
        RepositoryInfo {
            id: RepositoryId(0),
            name: name.into(),
            checkouts: vec![CheckoutInfo {
                id: CheckoutId(0),
                name: "main".into(),
                path: String::new(),
                panes: (0..panes).map(|_| pane(PaneStatus::Idle, &[])).collect(),
                git: None,
                primary: true,
            }],
            branches: Vec::new(),
            default_branch: None,
            remote_branches: Vec::new(),
            unmerged: Default::default(),
            last_used,
        }
    }

    #[test]
    fn active_repositories_lead_in_tree_order_and_quiet_ones_follow_by_last_use() {
        let project = ProjectInfo {
            id: ProjectId(0),
            name: "src".into(),
            root: None,
            repositories: vec![
                repository("never", 0, None),
                repository("hearth", 1, Some(5)),
                repository("old", 0, Some(10)),
                repository("argus", 2, None),
                repository("recent", 0, Some(30)),
            ],
        };
        assert_eq!(project.repositories_by_activity(), (vec![1, 3], vec![4, 2, 0]));
    }

    #[test]
    fn git_status_from_a_daemon_without_staged_counts_reads_as_zero() {
        #[derive(Serialize)]
        struct OlderGitStatus {
            branch: Option<String>,
            dirty: bool,
            changed_files: usize,
            ahead: usize,
            behind: usize,
        }
        let older = OlderGitStatus {
            branch: Some("main".into()),
            dirty: true,
            changed_files: 2,
            ahead: 1,
            behind: 0,
        };
        let bytes = rmp_serde::to_vec_named(&older).unwrap();
        let read: GitStatus = rmp_serde::from_slice(&bytes).unwrap();
        assert!(read.upstream.is_none() && read.last_commit.is_none());
        assert_eq!((read.changed_files, read.ahead), (2, 1));
        assert_eq!((read.staged, read.unstaged), (0, 0));
    }
}
