//! The scenes the screenshot harness draws: a session brought to each
//! screen the window shows, and the events a fresh page hears there.
//!
//! A scene is reached the way a person reaches it, by the page's calls,
//! and a stand-in daemon answers whatever those calls ask, from one tree
//! of the Argus project at work. `page/test/screens` replays each scene's
//! events into the page and photographs it.

mod extension_kinds;
#[cfg(test)]
mod live;
mod saved;

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use argus_protocol::agents::sessions::{Resume, Session as AgentSession};
use argus_protocol::agents::transcript::{Conversation, Entry, Meta, Transcript};
use argus_protocol::{
    grid_runs, AgentTelemetry, Cell, ChangeKind, CheckoutId, CheckoutInfo, ChildAgentInfo, ClientMsg, CodeChange,
    CodeFile, CodeLine, CodeMatch, CodeMatches, CodeSymbol, CodeTree, Color, CommitFile, CommitInfo, Consequence,
    CheckoutDir, CheckoutEntry, EntryGit, Criterion, Cursor, Decision, DecisionBoard, DecisionState, DiagramAction, DiagramList, DiffLine, DirEntry,
    DirListing, ExtDialog, ExtDialogKind, Feature, FeatureState, FileDiff, GitStatus, Hello, HighlightKind,
    HighlightSpan, Hunk, LineKind, Noticed, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId, ProjectInfo,
    ProjectPreview, ProposedLine, Reconcile, Reconciled, RepositoryId, RepositoryInfo, RequestRefusal, Review,
    ReviewBase, SequenceDiagram, ServerMsg, Task, TaskAction, TaskCounts, TaskList, TaskNote, TaskState,
    TrackerLink, WorkspaceId, WorkspaceInfo,
};
use serde_json::{json, Value as Json};

use crate::board::capture_note::CaptureGesture;
use crate::board::features::Layout;
use crate::code::stage::{CodeClick, CodeGesture};
use crate::connection::page::{Call, FontFamily, Lens, ReviewJumpGesture, Side, WorkAt};
use crate::keys::keymap::Action;
use crate::keys::page_key::PageKey;
use crate::layout::docks::{DockGesture, Panel};
use crate::panes::console::ConsoleGesture;
use crate::extensions::views::ExtensionGesture;
use crate::session::Session;

/// The window's clock in every scene, a Monday afternoon just after the
/// commits the history lists, so every "ago" reads as a working day's.
pub const NOW: u64 = 1_792_460_000;
/// The Workspace's room at the window size the screenshots are taken at.
const ROWS: u16 = 40;
const COLS: u16 = 140;

const ORION: &str = r"C:\Source\github.com\hbaldwin98\orion";

fn worktree(name: &str) -> String {
    format!(r"{ORION}\.argus\worktrees\{name}")
}

fn ago(secs: u64) -> i64 {
    (NOW - secs) as i64
}

// ---- The tree -----------------------------------------------------------------

fn commit(oid: &str, summary: &str, secs_ago: u64) -> CommitInfo {
    CommitInfo { oid: oid.into(), short: oid[..7].into(), summary: summary.into(), author: "Hunter Baldwin".into(), time: ago(secs_ago) }
}

/// The newest commits on main, as `git log` has them.
fn history() -> Vec<CommitInfo> {
    vec![
        commit("6ce099a2fe748883fb800a01ecc60bcab524135b", "Bump version to 0.33.6 for release", 5_600),
        commit("5cd8db73d3ee2a08f0dccb08e38666606dc96394", "Stop capping an agent's memory and processes", 5_700),
        commit("3327aa29f1ba6f6cb47dd31d57ad90dfb60505d2", "Hold changed code to 80% and skip daemon layers a change cannot reach", 39_500),
        commit("fb8c04984e953860cafba719ac1b5faf975c1e98", "Record how argus adopted reliab", 39_600),
        commit("2092919f0818fc75cc56c748ade29c875bf1bda5", "Keep the paste property's saved failing case", 39_700),
        commit("caeb3e5fc566fd8fe87f2b01a48aea9724b7e6f6", "Strip paste markers out of a bracketed paste", 81_000),
        commit("b93ea567143556ed6e3c6598f7298e481ffe3312", "Require a reviewer verdict before a push", 84_800),
        commit("05828e1cfb5e9bd0039e2430e5d1e6f6dba16125", "List the other agents for an agent to link them from a view", 90_200),
        commit("aff3cf042180c767086e56b2a5311d817582558c", "Open a view's graph or sequence diagram larger to zoom and drag", 91_400),
        commit("aa10a2010c567a48999479fed257411c0a8b2b67", "List saved views under their scope without saying it twice", 91_450),
        commit("fa9ae243d1ceab8ccfb8c31b2bfe53cc9f4dc8f5", "Lay a graph out from its left edge", 91_500),
        commit("a49ceb2f0bbcd46c80d7039830dcfd66182842eb", "Keep the bridge's pipes out of the daemon it starts", 92_000),
        commit("1428d130538ce56051ad854bdd4f6d46a58b158d", "Bump version to 0.33.5 for release", 92_300),
    ]
}

struct Git {
    branch: &'static str,
    staged: usize,
    unstaged: usize,
    ahead: usize,
    behind: usize,
    last: CommitInfo,
}

fn checkout(id: u64, name: &str, path: String, primary: bool, git: Git, panes: Vec<PaneInfo>) -> CheckoutInfo {
    let changed = git.staged + git.unstaged;
    CheckoutInfo {
        id: CheckoutId(id),
        name: name.into(),
        path,
        panes,
        git: Some(GitStatus {
            branch: Some(git.branch.into()),
            dirty: changed > 0,
            changed_files: changed,
            staged: git.staged,
            unstaged: git.unstaged,
            ahead: git.ahead,
            behind: git.behind,
            upstream: Some(format!("origin/{}", git.branch)),
            last_commit: Some(git.last),
            changes: 0,
        }),
        primary,
    }
}

fn clean(branch: &'static str, last: CommitInfo) -> Git {
    Git { branch, staged: 0, unstaged: 0, ahead: 0, behind: 0, last }
}

fn pane(id: u64, kind: PaneKind, title: &str, status: PaneStatus, since: u64) -> PaneInfo {
    let mut pane = crate::fixtures::pane(id, kind, title, status);
    pane.since = Some(NOW - since);
    pane
}

fn agent(id: u64, title: &str, status: PaneStatus, note: Option<&str>, telemetry: AgentTelemetry, since: u64) -> PaneInfo {
    let template = title.split(" · ").next().unwrap_or("claude");
    PaneInfo {
        note: note.map(str::to_string),
        template: Some(template.into()),
        telemetry,
        live: template == "claude",
        ..pane(id, PaneKind::Agent, title, status, since)
    }
}

fn telemetry(model: &str, context: u64, cost: f64, tool: Option<&str>, calls: u64) -> AgentTelemetry {
    AgentTelemetry {
        model: Some(model.into()),
        context_tokens: Some(context),
        context_window: Some(200_000),
        input_tokens: Some(context * 14),
        output_tokens: Some(context / 3),
        cost_usd: Some(cost),
        tool: tool.map(str::to_string),
        tool_calls: Some(calls),
    }
}

fn repository(id: u64, name: &str, last_used: u64, checkouts: Vec<CheckoutInfo>) -> RepositoryInfo {
    let mut repo = crate::fixtures::repository(id, name, Some(NOW - last_used), checkouts);
    repo.default_branch = Some("main".into());
    repo
}

const P_TRIM: u64 = 1;
const P_SHELL: u64 = 2;
const P_SCREENS: u64 = 3;
const P_EDITOR: u64 = 4;
const P_RATATUI: u64 = 5;
const P_FAULTS: u64 = 6;
const P_FAULTS_TEST: u64 = 7;
const P_NEBULA: u64 = 8;
/// An agent's browser pane, by the agent that owns it.
const B_TRIM: u64 = 30;
const B_SCREENS: u64 = 31;
const B_FAULTS: u64 = 32;
const P_API_SHELL: u64 = 20;
const P_API_AGENT: u64 = 21;
const C_CONSOLE: u64 = 50;
const C_DEV: u64 = 51;

const MAIN: u64 = 100;
const DESKTOP: u64 = 101;
const RATATUI: u64 = 102;
const FAULTS: u64 = 103;
const ARGUS: u64 = 10;

fn tree() -> Vec<ProjectInfo> {
    let log = history();
    let mut argus = repository(ARGUS, "argus", 120, vec![
        checkout(
            MAIN,
            "main",
            ORION.into(),
            true,
            Git { unstaged: 4, ..clean("main", log[0].clone()) },
            vec![
                PaneInfo {
                    children: vec![ChildAgentInfo { label: "Explore · hook callers".into(), status: PaneStatus::Working, note: None }],
                    ..agent(P_TRIM, "claude · trim context", PaneStatus::Working, None, telemetry("claude-opus-4-5", 84_312, 4.18, Some("Bash"), 57), 194)
                },
                pane(P_SHELL, PaneKind::Shell, "pwsh", PaneStatus::Idle, 3_100),
            ],
        ),
        checkout(
            DESKTOP,
            "desktop-design",
            worktree("desktop-design"),
            false,
            Git {
                staged: 2,
                unstaged: 4,
                ahead: 3,
                ..clean("desktop-design", commit("9d41c07e3b2a8f6e1c0d5b4a39f8e7d6c5b4a392", "Draw a feature's flows beside its steps", 2_400))
            },
            vec![
                agent(P_SCREENS, "claude · desktop screenshots", PaneStatus::NeedsReview, Some("34 scenes written; ready for a look"), telemetry("claude-opus-4-5", 131_904, 7.62, None, 112), 610),
                pane(P_EDITOR, PaneKind::Editor, "session.rs", PaneStatus::Idle, 1_900),
            ],
        ),
        checkout(
            RATATUI,
            "ratatui-bump",
            worktree("ratatui-bump"),
            false,
            Git { ahead: 1, behind: 4, ..clean("ratatui-bump", commit("41be7a0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f40", "Bump ratatui to 0.30", 7_300)) },
            vec![agent(P_RATATUI, "codex · ratatui 0.30", PaneStatus::Waiting, Some("approve cargo update -p crossterm?"), telemetry("gpt-5-codex", 52_118, 1.06, None, 23), 75)],
        ),
        checkout(
            FAULTS,
            "store-faults",
            worktree("store-faults"),
            false,
            Git { unstaged: 1, ahead: 2, ..clean("store-faults", commit("c07f3e2a1b9d8c7e6f5a4b3c2d1e0f9a8b7c6d5e", "Inject a torn write into the store's journal", 14_000)) },
            vec![
                agent(P_FAULTS, "claude · store faults", PaneStatus::Failed, Some("3 mutants survived in store::faults"), telemetry("claude-sonnet-4-5", 96_550, 2.31, None, 88), 1_250),
                pane(P_FAULTS_TEST, PaneKind::Shell, "cargo test", PaneStatus::Exited { code: Some(101) }, 1_400),
            ],
        ),
    ]);
    argus.branches = ["main", "desktop-design", "ratatui-bump", "store-faults", "spike/kitty-keys", "old/tui-theme"].map(String::from).to_vec();
    argus.remote_branches = ["origin/main", "origin/ratatui-bump", "origin/old/tui-theme"].map(String::from).to_vec();
    argus.unmerged = BTreeMap::from([("spike/kitty-keys".to_string(), 3), ("old/tui-theme".to_string(), 11)]);
    let nebula = repository(11, "nebula", 2 * 86_400, vec![checkout(
        110,
        "main",
        format!(r"{ORION}\nebula"),
        true,
        clean("main", commit("e5d4c3b2a1f0e9d8c7b6a5f4e3d2c1b0a9f8e7d6", "Pace frames to the display's refresh", 2 * 86_400)),
        vec![agent(P_NEBULA, "claude · render loop", PaneStatus::Done, Some("frame pacing merged"), telemetry("claude-opus-4-5", 40_220, 1.88, None, 41), 2 * 86_400)],
    )]);
    let site = repository(12, "argus-site", 9 * 86_400, vec![checkout(
        120,
        "main",
        format!(r"{ORION}\argus-site"),
        true,
        clean("main", commit("0a1b2c3d4e5f60718293a4b5c6d7e8f901234567", "Show 0.33 on the download page", 9 * 86_400)),
        Vec::new(),
    )]);
    let api = repository(20, "api", 3_600, vec![
        checkout(
            200,
            "main",
            r"C:\Source\work\api".into(),
            true,
            clean("main", commit("7f6e5d4c3b2a19087f6e5d4c3b2a19087f6e5d4c", "Return 429 with Retry-After", 3 * 3_600)),
            vec![pane(P_API_SHELL, PaneKind::Shell, "pwsh", PaneStatus::Idle, 3_600)],
        ),
        checkout(
            201,
            "rate-limits",
            r"C:\Source\work\api\.argus\worktrees\rate-limits".into(),
            false,
            Git { unstaged: 3, ahead: 1, ..clean("rate-limits", commit("1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d", "Count requests per API key", 3_000)) },
            vec![agent(P_API_AGENT, "claude · rate limit headers", PaneStatus::Working, None, telemetry("claude-sonnet-4-5", 61_004, 0.94, Some("Edit"), 19), 300)],
        ),
    ]);
    vec![
        ProjectInfo { id: ProjectId(1), name: "orion".into(), root: Some(ORION.into()), repositories: vec![argus, nebula, site] },
        ProjectInfo { id: ProjectId(2), name: "work".into(), root: Some(r"C:\Source\work".into()), repositories: vec![api] },
    ]
}

fn consoles() -> Vec<PaneInfo> {
    vec![pane(C_CONSOLE, PaneKind::Shell, "pwsh", PaneStatus::Idle, 900), pane(C_DEV, PaneKind::Shell, "npm run dev", PaneStatus::Idle, 2_000)]
}

fn workspaces() -> Vec<WorkspaceInfo> {
    vec![
        WorkspaceInfo { id: WorkspaceId(1), name: "default".into(), projects: 2, panes: 11, open: true },
        WorkspaceInfo { id: WorkspaceId(2), name: "experiments".into(), projects: 1, panes: 0, open: false },
    ]
}

// ---- The board ----------------------------------------------------------------

fn feature(number: i64, slug: &str, title: &str, body: &str, tasks: (usize, usize, usize), days: u64) -> Feature {
    Feature {
        number,
        slug: slug.into(),
        title: title.into(),
        body: body.into(),
        at: ago(days * 86_400),
        tasks: TaskCounts { todo: tasks.0, doing: tasks.1, done: tasks.2 },
        ..Feature::default()
    }
}

fn link(key: &str, role: &str) -> TrackerLink {
    let number = key.trim_start_matches("orion#");
    TrackerLink { tracker: "GitHub".into(), key: key.into(), url: format!("https://github.com/hbaldwin98/orion/issues/{number}"), role: role.into() }
}

fn features() -> Vec<Feature> {
    vec![
        Feature { state: FeatureState::Done, ..feature(36, "agent-views", "Agent views", "Agents publish a view of their work into the window: tables, charts, checklists, a diagram, instead of a wall of terminal text.", (0, 0, 9), 41) },
        Feature { state: FeatureState::Done, ..feature(38, "saved-views", "Saved views", "A view an agent built worth keeping is saved under its repository or feature, rebuilt on request, and linked from other views.", (0, 0, 6), 30) },
        Feature { state: FeatureState::Done, ..feature(39, "ctrlc-windows", "Ctrl-C on Windows", "Ctrl-C reaches a pane's child through ConPTY as the console's own break, not as a byte the child never reads.", (0, 0, 4), 22) },
        Feature {
            checkouts: vec![ORION.into()],
            links: vec![link("orion#212", "the work"), link("orion#214", "the PM's ticket")],
            origin_branch: Some("main".into()),
            reconcile: Some(Reconcile {
                asked: "Bring the GitHub issues in line with where #41 stands".into(),
                pane: PaneId(P_TRIM),
                by: "claude · trim context".into(),
                at: ago(900),
                proposed: true,
                lines: vec![
                    ProposedLine {
                        to: "GitHub".into(),
                        key: "orion#212".into(),
                        does: "comment progress".into(),
                        why: "2 of 7 tasks done since the last sync".into(),
                        text: "The brief route is in (task 1.1); the hook reads it next. Prompt context is down from 10.8k to 3.1k tokens on the orion board.".into(),
                        call: "gh issue comment 212 --body-file -".into(),
                        failed: None,
                    },
                    ProposedLine {
                        to: "GitHub".into(),
                        key: "orion#214".into(),
                        does: "close as done".into(),
                        why: "the route it asked for shipped".into(),
                        text: String::new(),
                        call: "gh issue close 214 --reason completed".into(),
                        failed: None,
                    },
                    ProposedLine {
                        to: "Argus".into(),
                        key: "task 5".into(),
                        does: "add a subtask".into(),
                        why: "#212 asks for a hard size cap".into(),
                        text: "Refuse a brief over 4k tokens".into(),
                        call: "argus-hook task add --parent 5".into(),
                        failed: None,
                    },
                ],
                noticed: vec![Noticed { key: "orion#212".into(), text: "A comment asks whether a sub-agent's prompt gets the brief too".into() }],
                carrying: Vec::new(),
            }),
            reconciled: Some(Reconciled { at: ago(3 * 86_400), by: "claude · trim context".into(), seen: Vec::new() }),
            ..feature(41, "trim", "Trim context injection", "Every prompt an agent gets carries every standing decision on the board, 10k tokens and climbing. Serve a feature's brief on demand from the daemon instead, and cap how much history it carries.", (3, 2, 2), 6)
        },
        Feature {
            checkouts: vec![worktree("desktop-design")],
            origin_branch: Some("desktop-design".into()),
            ..feature(42, "desktop-screens", "Screenshot harness for the desktop page", "Every screen the desktop window can show, rendered headless from real events, so a design review looks at the product rather than at mock-ups.", (1, 1, 4), 2)
        },
        Feature {
            checkouts: vec![worktree("ratatui-bump")],
            held: Some("until crossterm 0.29 is released".into()),
            ..feature(43, "ratatui-bump", "The ratatui 0.30 bump", "ratatui 0.30 splits the crate and changes how a frame's area is read. Bump it, and the terminal client's widgets with it.", (3, 0, 2), 12)
        },
        Feature {
            checkouts: vec![worktree("store-faults")],
            ..feature(44, "store-faults", "Fault injection for the store", "A torn write, a full disk or a lost rename must leave the store readable: inject each into the journal and prove the daemon recovers.", (2, 1, 3), 4)
        },
        feature(45, "mouse-select", "Mouse selection in panes", "Drag to select across a pane's rows, and copy what was selected, without the child seeing the mouse unless it asked for it.", (4, 0, 0), 9),
        Feature { waits_on: vec!["store-faults".into()], ..feature(46, "ssh-hosts", "Remote hosts over ssh", "Reach a daemon on another machine through ssh, as the window already reaches this machine's through the bridge.", (5, 0, 0), 8) },
        feature(47, "palette-recents", "Recent items in the palette", "The palette forgets where you were. Offer the panes, features and checkouts last gone to above the fuzzy matches.", (3, 0, 0), 1),
    ]
}

fn decision(id: i64, feature: &str, state: DecisionState, chose: &str, task: Option<i64>, days: u64) -> Decision {
    Decision {
        id,
        at: ago(days * 86_400),
        feature: Some(feature.into()),
        chose: chose.into(),
        state,
        task,
        session: Some("2b7f1c9e".into()),
        ..Decision::default()
    }
}

fn decisions() -> Vec<Decision> {
    vec![
        Decision {
            over: Some("Inject every standing decision into each prompt".into()),
            because: Some("the cost grows with the board, and most prompts need none of it".into()),
            context: Some("A prompt on the orion board carries 23 standing decisions, 10.8k tokens, whatever it is about.".into()),
            consequences: vec![
                Consequence { gain: true, text: "a prompt pays only for the feature it works on".into() },
                Consequence { gain: false, text: "one more request before an agent's first turn".into() },
            ],
            ..decision(1, "trim", DecisionState::Accepted, "Serve the brief on demand through a daemon route", Some(1), 5)
        },
        Decision {
            parent: Some(1),
            because: Some("the hook already holds the pane's token; a second credential is a second thing to leak".into()),
            ..decision(2, "trim", DecisionState::Accepted, "The hook asks with the pane's own token", Some(3), 4)
        },
        Decision {
            options: vec!["Every standing decision".into(), "Only the binding ones".into(), "The last 20".into()],
            context: Some("A brief with no cap grows back into what it replaced.".into()),
            ..decision(3, "trim", DecisionState::Open, "How much history should a brief carry?", Some(5), 1)
        },
        Decision {
            because: Some("a cached brief is wrong the moment a decision lands, and the route answers in under a millisecond".into()),
            ..decision(4, "trim", DecisionState::Rejected, "Cache the brief in the hook between prompts", Some(1), 4)
        },
        decision(5, "desktop-screens", DecisionState::Accepted, "Drive each scene through the page's own calls", None, 2),
        decision(6, "store-faults", DecisionState::Open, "Should a torn journal entry be dropped or refuse the start?", None, 1),
    ]
}

fn board() -> DecisionBoard {
    DecisionBoard {
        project: Some(ProjectId(1)),
        name: "orion".into(),
        features: features(),
        decisions: decisions(),
        tracker_map: Default::default(),
        repositories: vec![RepositoryId(ARGUS)],
    }
}

fn task(feature: &str, id: i64, parent: Option<i64>, position: i64, state: TaskState, title: &str) -> Task {
    Task { id, feature: feature.into(), parent, title: title.into(), state, position, at: ago(5 * 86_400) + id * 600, ..Task::default() }
}

fn tasks(feature: &str) -> TaskList {
    let tasks = match feature {
        "trim" => vec![
            Task {
                body: Some("The daemon answers `GET /brief` for the pane's feature: its title, its open questions, and the decisions that bind it.".into()),
                ..task(feature, 1, None, 0, TaskState::Doing, "Serve the brief on demand")
            },
            Task {
                criteria: vec![Criterion { text: "The route answers for a pane with a feature".into(), done: true }, Criterion { text: "A pane without one is refused".into(), done: true }],
                ..task(feature, 2, Some(1), 0, TaskState::Done, "Add a brief route to argusd")
            },
            Task {
                claimed_pane: Some(PaneId(P_TRIM)),
                claimed_by: Some("claude · trim context".into()),
                criteria: vec![
                    Criterion { text: "The hook asks for the brief once per prompt".into(), done: true },
                    Criterion { text: "A refused brief leaves the prompt as it was".into(), done: false },
                    Criterion { text: "A test covers both".into(), done: false },
                ],
                notes: vec![TaskNote { at: ago(420), session: Some("2b7f1c9e".into()), pane: Some(PaneId(P_TRIM)), text: "Wiring context.rs to the route now; 14 brief tests pass.".into() }],
                ..task(feature, 3, Some(1), 1, TaskState::Doing, "The hook reads the brief from the route")
            },
            task(feature, 4, Some(1), 2, TaskState::Todo, "Drop the injected decisions from the prompt"),
            task(feature, 5, None, 1, TaskState::Todo, "Cap the history a brief carries"),
            task(feature, 6, Some(5), 0, TaskState::Done, "Measure brief sizes on the orion board"),
            task(feature, 7, None, 2, TaskState::Todo, "Document the brief in DESIGN.md"),
        ],
        "desktop-screens" => vec![
            task(feature, 11, None, 0, TaskState::Done, "Build a session for every screen"),
            task(feature, 12, None, 1, TaskState::Done, "A stand-in daemon answers each request"),
            task(feature, 13, None, 2, TaskState::Doing, "Photograph each scene in headless Chrome"),
            task(feature, 14, None, 3, TaskState::Todo, "Run it in CI on a page change"),
        ],
        _ => vec![
            task(feature, 21, None, 0, TaskState::Done, "Read what the change touches"),
            task(feature, 22, None, 1, TaskState::Todo, "Write the regression tests first"),
        ],
    };
    TaskList { repositories: vec![RepositoryId(ARGUS)], project_name: "orion".into(), feature: Some(feature.into()), tasks }
}

const BRIEF_FLOW: &str = "sequenceDiagram
    participant agent
    participant hook as argus-hook
    participant argusd
    agent->>hook: UserPromptSubmit
    hook->>argusd: GET /brief (pane token)
    %% decision #2
    argusd->>argusd: find the pane's feature
    alt the pane has a feature
        argusd-->>hook: the brief
        hook-->>agent: prompt + brief
    else it has none
        argusd-->>hook: 404 no feature
        hook-->>agent: prompt as it was
    end";

const CAP_FLOW: &str = "sequenceDiagram
    participant argusd
    participant board
    argusd->>board: standing decisions for #41
    board-->>argusd: 23 decisions
    %% decision #3
    argusd->>argusd: keep the binding ones, newest first";

fn diagrams(feature: &str) -> DiagramList {
    let diagram = |id: i64, title: &str, body: &str| SequenceDiagram { id, feature: feature.into(), title: title.into(), body: body.into(), at: ago(86_400), session: None };
    let diagrams = match feature {
        "trim" => vec![diagram(11, "Asking for the brief", BRIEF_FLOW), diagram(12, "Capping the history", CAP_FLOW)],
        _ => Vec::new(),
    };
    DiagramList { repositories: vec![RepositoryId(ARGUS)], project_name: "orion".into(), feature: Some(feature.into()), diagrams }
}

// ---- Code, diffs and history --------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The checkout's files as git would list them, read off this one.
fn files() -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !matches!(name.as_str(), "target" | "node_modules" | "bindings" | "proptest-regressions" | ".git") {
                    walk(&path, root, out);
                }
            } else if [".rs", ".ts", ".toml", ".md", ".css", ".html"].iter().any(|ext| name.ends_with(ext)) {
                let relative = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push(relative);
            }
        }
    }
    let root = repo_root().canonicalize().unwrap();
    let mut out: Vec<String> = ["AGENTS.md", "Cargo.toml", "DESIGN.md", "README.md", "ROADMAP.md", "TARGET.md"]
        .into_iter()
        .filter(|f| root.join(f).exists())
        .map(String::from)
        .collect();
    walk(&root.join("crates"), &root, &mut out);
    out.sort();
    out
}

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "else", "enum", "false", "fn", "for", "if", "impl",
    "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct",
    "super", "trait", "true", "type", "use", "where", "while", "Some", "None", "Ok", "Err",
];

/// A Rust line's tokens, as near to the daemon's tree-sitter highlighting
/// as a screenshot needs.
fn highlight(line: &str) -> Vec<HighlightSpan> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    let span = |start: usize, end: usize, kind| HighlightSpan { start: start as u32, end: end as u32, kind };
    while i < bytes.len() {
        let c = bytes[i];
        if line[i..].starts_with("//") {
            spans.push(span(i, bytes.len(), HighlightKind::Comment));
            break;
        } else if c == b'"' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'"' {
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(bytes.len());
            spans.push(span(i, end, HighlightKind::Str));
            i = end;
        } else if c.is_ascii_digit() {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'.') {
                j += 1;
            }
            spans.push(span(i, j, HighlightKind::Number));
            i = j;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let word = &line[i..j];
            let next = line[j..].trim_start().chars().next();
            let kind = if KEYWORDS.contains(&word) {
                Some(HighlightKind::Keyword)
            } else if next == Some('(') || next == Some('!') {
                Some(HighlightKind::Function)
            } else if word.len() > 1 && word.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) {
                Some(HighlightKind::Constant)
            } else if word.starts_with(|c: char| c.is_ascii_uppercase()) {
                Some(HighlightKind::Type)
            } else if i > 0 && bytes[i - 1] == b'.' {
                Some(HighlightKind::Property)
            } else {
                None
            };
            if let Some(kind) = kind {
                spans.push(span(i, j, kind));
            }
            i = j;
        } else if b"=<>!&|+-*/%?".contains(&c) {
            spans.push(span(i, i + 1, HighlightKind::Operator));
            i += 1;
        } else {
            i += line[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    spans
}

fn spans_for(path: &str, text: &str) -> Vec<HighlightSpan> {
    if path.ends_with(".rs") { highlight(text) } else { Vec::new() }
}

/// What a Rust file defines, as the daemon's outline names it.
fn outline(lines: &[&str]) -> Vec<CodeSymbol> {
    let mut symbols = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let indented = line.starts_with(' ');
        let bare = line.trim_start().trim_start_matches("pub(crate) ").trim_start_matches("pub ");
        let named = |prefix: &str| bare.strip_prefix(prefix).map(|rest| rest.split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or("").to_string());
        let found = if let Some(name) = named("fn ") {
            Some((name, if indented { "method" } else { "function" }))
        } else if let Some(name) = named("struct ").or_else(|| named("enum ")) {
            Some((name, "class"))
        } else if let Some(name) = named("trait ") {
            Some((name, "interface"))
        } else if let Some(name) = named("mod ") {
            Some((name, "module"))
        } else {
            named("impl ").map(|name| (name, "class"))
        };
        if let Some((name, kind)) = found.filter(|(name, _)| !name.is_empty()) {
            symbols.push(CodeSymbol { name, kind: kind.into(), line: i as u32 + 1 });
        }
    }
    symbols
}

fn read_file(path: &str) -> String {
    std::fs::read_to_string(repo_root().join(path)).unwrap_or_else(|_| "// not on this machine\n".into())
}

/// A unified diff's hunks as the daemon sends them.
fn hunks(path: &str, patch: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let (mut old, mut new) = (0, 0);
    for line in patch.lines() {
        if let Some(header) = line.strip_prefix("@@ -") {
            let numbers: Vec<u32> = header
                .split([' ', ',', '+'])
                .take_while(|part| *part != "@@")
                .filter_map(|part| part.parse().ok())
                .collect();
            // Every patch here names both counts: `-old,n +new,m`.
            old = numbers[0];
            new = numbers[2];
            hunks.push(Hunk { header: line.into(), lines: Vec::new() });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else { continue };
        let (kind, text) = match line.chars().next() {
            Some('+') => (LineKind::Added, &line[1..]),
            Some('-') => (LineKind::Removed, &line[1..]),
            Some(' ') => (LineKind::Context, &line[1..]),
            _ => (LineKind::Context, ""),
        };
        let (old_lineno, new_lineno) = match kind {
            LineKind::Added => (None, Some(new)),
            LineKind::Removed => (Some(old), None),
            LineKind::Context => (Some(old), Some(new)),
        };
        if kind != LineKind::Added {
            old += 1;
        }
        if kind != LineKind::Removed {
            new += 1;
        }
        hunk.lines.push(DiffLine { kind, old_lineno, new_lineno, text: text.into(), spans: spans_for(path, text) });
    }
    hunks
}

fn file_diff(path: &str, kind: ChangeKind, patch: &str) -> FileDiff {
    FileDiff { path: path.into(), old_path: None, kind, hunks: hunks(path, patch), note: None, new_blob: None, new_lines: None }
}

const BRIEF_RS: &str = "@@ -0,0 +1,31 @@
+//! A feature's brief, served to the hook on demand: its title, its open
+//! questions, and the decisions that bind it.
+
+use argus_protocol::{Decision, DecisionBoard, PaneId};
+
+use crate::board::standing;
+
+/// What an agent is told about the feature its pane works on.
+pub struct Brief {
+    pub title: String,
+    pub open: Vec<String>,
+    pub binding: Vec<Decision>,
+}
+
+impl Brief {
+    /// The brief of `pane`'s feature, or `None` for a pane with none.
+    pub fn for_pane(board: &DecisionBoard, pane: PaneId) -> Option<Brief> {
+        let feature = board.feature_of(pane)?;
+        let binding = standing::binding(board, &feature.slug).cloned().collect();
+        Some(Brief { title: feature.title.clone(), open: standing::open(board, &feature.slug), binding })
+    }
+
+    pub fn summary(&self) -> String {
+        let mut text = format!(\"Feature: {}\\n\", self.title);
+        for decision in &self.binding {
+            text.push_str(&format!(\"- decided: {}\\n\", decision.chose));
+        }
+        text
+    }
+}
";

const CONTEXT_RS: &str = "@@ -52,14 +52,11 @@ pub async fn prompt_context(client: &Client, pane: PaneId) -> Result<String> {
     let mut prompt = String::new();
     prompt.push_str(&client.instructions().await?);

-    // Every standing decision rides along, so an agent never acts against
-    // one it was not told about.
-    let board = client.board().await?;
-    for decision in board.standing() {
-        prompt.push_str(&decision.as_line());
-        prompt.push('\\n');
-    }
+    // The brief of the pane's feature, asked for now: a pane with no
+    // feature is refused, and its prompt goes as it was.
+    if let Some(brief) = client.brief(pane).await? {
+        prompt.push_str(&brief.summary());
+    }

     Ok(prompt)
 }
@@ -88,6 +85,13 @@ impl Client {
         self.get(\"/board\").await
     }

+    /// The brief of `pane`'s feature; `None` when the daemon says it has none.
+    pub async fn brief(&self, pane: PaneId) -> Result<Option<Brief>> {
+        match self.get(&format!(\"/brief?pane={pane}\")).await {
+            Err(e) if e.is_not_found() => Ok(None),
+            other => other.map(Some),
+        }
+    }
+
     async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
         let response = self.http.get(self.url(path)).bearer_auth(&self.token).send().await?;
";

const STANDING_RS: &str = "@@ -14,9 +14,17 @@ use argus_protocol::{Decision, DecisionBoard, DecisionState};
 /// The decisions still standing: accepted, not withdrawn, not superseded.
 pub fn standing(board: &DecisionBoard) -> impl Iterator<Item = &Decision> {
     board.decisions.iter().filter(|d| d.state == DecisionState::Accepted && !d.superseded() && !d.withdrawn())
 }

+/// The standing decisions made under `feature`, the ones its brief binds.
+pub fn binding<'a>(board: &'a DecisionBoard, feature: &'a str) -> impl Iterator<Item = &'a Decision> {
+    standing(board).filter(move |d| d.feature.as_deref() == Some(feature))
+}
+
+/// The questions still open under `feature`.
+pub fn open(board: &DecisionBoard, feature: &str) -> Vec<String> {
+    board.decisions.iter().filter(|d| d.state == DecisionState::Open && d.feature.as_deref() == Some(feature)).map(|d| d.chose.clone()).collect()
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
";

const DESIGN_MD: &str = "@@ -1412,8 +1412,10 @@ The hook speaks for the pane it runs in.

 ### What an agent is told

-Each prompt carries the board's standing decisions, so an agent never acts
-against one it was not told about.
+Each prompt carries its feature's brief, asked of the daemon as the prompt
+is sent: the feature's title, its open questions and the decisions that
+bind it. A pane with no feature is refused, and its prompt goes as it was;
+the rest of the board is a request away (`argus-hook board`).

 The instructions themselves come from `argus-hook instructions`, which
";

const HOOK_MOD_RS: &str = "@@ -31,6 +31,8 @@ pub const INSTRUCTIONS_COMMAND: &str = \"instructions\";
 pub const CONTEXT_COMMAND: &str = \"context\";
+/// The route a pane's brief is read from.
+pub const BRIEF_PATH: &str = \"/brief\";

 /// The header a request names its session with.
 pub const SESSION_HEADER: &str = \"x-argus-session\";
";

const JOB_RS: &str = "@@ -4,20 +4,12 @@
 //! parent unless something reaches them. On Windows a job object is that
 //! owner: every pane's console server and child are assigned to one at
 //! spawn, so closing the pane, or the death of the process that holds the
-//! job, takes them with it, and an agent's runaway cannot take the machine
-//! down on its way. On Unix the child is its own session leader, and the
-//! session is what gets swept.
+//! job, takes them with it. On Unix the child is its own session leader,
+//! and the session is what gets swept.

 use super::*;


-#[cfg(windows)]
-#[derive(Clone, Copy)]
-pub(super) struct JobLimits {
-    pub(super) memory_bytes: usize,
-    pub(super) active_processes: u32,
-}
-
 #[cfg(windows)]
 pub(crate) struct ProcessJob {
     handle: OwnedHandle,
@@ -25,19 +17,16 @@ pub(crate) struct ProcessJob {

 #[cfg(windows)]
 impl ProcessJob {
-    /// An agent's job: bounded, and nothing the agent starts leaves it.
-    pub(super) fn new(limits: JobLimits) -> anyhow::Result<Self> {
+    /// An agent's job: nothing the agent starts leaves it. Unbounded: a
+    /// memory or process cap ended agents mid-task when a build or test run
+    /// crossed it, which cost more than the runaway it guarded against.
+    pub(super) fn agent() -> anyhow::Result<Self> {
         use windows_sys::Win32::System::JobObjects::{
-            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
-            JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
+            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
         };

         let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
-        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_JOB_MEMORY
-            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
-            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
-        info.BasicLimitInformation.ActiveProcessLimit = limits.active_processes;
-        info.JobMemoryLimit = limits.memory_bytes;
+        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
         Self::with(&info)
     }

@@ -254,10 +243,7 @@ pub(super) fn hold_test_process() -> Option<&'static ProcessJob> {
 pub(super) fn job_for(resource_policy: ResourcePolicy) -> anyhow::Result<ProcessJob> {
     match resource_policy {
         ResourcePolicy::Unrestricted => ProcessJob::shell(),
-        ResourcePolicy::Agent => ProcessJob::new(JobLimits {
-            memory_bytes: AGENT_JOB_MEMORY_BYTES,
-            active_processes: AGENT_JOB_PROCESS_LIMIT,
-        }),
+        ResourcePolicy::Agent => ProcessJob::agent(),
     }
 }
";

const JOB_DESIGN_MD: &str = "@@ -2210,9 +2210,9 @@ An agent's pane is started in a job of its own.

-On Windows that job also caps the agent at 32 GiB of committed memory and
-256 processes, so a runaway cannot take the machine down.
+On Windows that job ends everything the agent started when the pane closes
+or the daemon dies, and keeps what the agent starts from breaking away. It
+caps nothing: a cap ended agents mid-build.

";

const JOB_MOD_RS: &str = "@@ -18,11 +18,6 @@ mod job;

-/// What an agent's job may commit before its allocations are refused.
-const AGENT_JOB_MEMORY_BYTES: usize = 32 << 30;
-/// How many processes an agent's job may hold at once.
-const AGENT_JOB_PROCESS_LIMIT: u32 = 256;
-
 pub use job::ProcessJob;
";

fn working_changes(base: ReviewBase) -> Vec<FileDiff> {
    match base {
        ReviewBase::Staged => vec![file_diff("crates/argus-protocol/src/hook/mod.rs", ChangeKind::Modified, HOOK_MOD_RS)],
        _ => vec![
            file_diff("crates/argus-hook/src/context.rs", ChangeKind::Modified, CONTEXT_RS),
            file_diff("crates/argusd/src/board/standing.rs", ChangeKind::Modified, STANDING_RS),
            file_diff("crates/argusd/src/hook/brief.rs", ChangeKind::Untracked, BRIEF_RS),
            file_diff("DESIGN.md", ChangeKind::Modified, DESIGN_MD),
        ],
    }
}

fn commit_changes() -> Vec<FileDiff> {
    vec![
        file_diff("crates/argusd/src/pty/job.rs", ChangeKind::Modified, JOB_RS),
        file_diff("DESIGN.md", ChangeKind::Modified, JOB_DESIGN_MD),
        file_diff("crates/argusd/src/pty/mod.rs", ChangeKind::Modified, JOB_MOD_RS),
    ]
}

fn changed(diffs: &[FileDiff]) -> Vec<CodeChange> {
    diffs.iter().map(|d| CodeChange { path: d.path.clone(), added: d.added_lines() as u32, removed: d.removed_lines() as u32 }).collect()
}

// ---- Agent sessions -----------------------------------------------------------

fn listed(harness: &str, id: &str, checkout: u64, title: &str, secs_ago: u64, turns: u32, pane: Option<u64>) -> AgentSession {
    AgentSession {
        harness: harness.into(),
        session_id: id.into(),
        checkout: CheckoutId(checkout),
        removed_worktree: None,
        title: Some(title.into()),
        started: Some("2026-10-19T14:02:11Z".into()),
        ended: pane.is_none().then(|| "2026-10-19T16:40:52Z".into()),
        modified: Some((NOW - secs_ago) * 1000),
        turns: Some(turns),
        pane: pane.map(PaneId),
    }
}

fn agent_sessions() -> Vec<AgentSession> {
    vec![
        listed("claude", "2b7f1c9e-41d3-4c0a-9a57-3e1f0b6c8d21", MAIN, "Trim the context the hook injects", 40, 31, Some(P_TRIM)),
        listed("claude", "8e0d4b6a-1f2c-4e9d-b3a7-5c6d7e8f9a01", DESKTOP, "Screenshot harness for the desktop page", 610, 58, Some(P_SCREENS)),
        listed("codex", "019a3f7c-2d4e-7b10-8c95-ae4f61d02b77", RATATUI, "Bump ratatui to 0.30 and fix what breaks", 75, 12, Some(P_RATATUI)),
        listed("claude", "c4a9e2f1-7b3d-4f60-8e21-9d0c5b7a3e48", MAIN, "Stop capping an agent's memory and processes", 5_700, 64, None),
        listed("claude", "5f1e8d2c-9a4b-4c7d-a6e3-0b2f1d9c8e75", MAIN, "Strip paste markers out of a bracketed paste", 81_000, 27, None),
        AgentSession {
            removed_worktree: Some("kitty-keys".into()),
            ..listed("codex", "019a1b2c-3d4e-7f50-9a61-b2c3d4e5f607", MAIN, "Shift-Enter through the kitty keyboard protocol", 6 * 86_400, 41, None)
        },
        listed("claude", "a7c3e9f2-5d1b-4e8a-9c60-1f3e5a7c9b24", FAULTS, "Fault injection for the store's journal", 1_250, 73, None),
    ]
}

fn transcript() -> Conversation {
    let user = |text: &str, at: &str| Entry::User { text: text.into(), at: Some(at.into()) };
    let said = |text: &str, at: &str| Entry::Agent { text: text.into(), at: Some(at.into()) };
    let tool = |tool: &str, summary: &str, failed: bool| Entry::Tool { tool: tool.into(), summary: summary.into(), at: None, failed };
    Conversation::Read(Transcript {
        meta: Meta {
            title: Some("Trim the context the hook injects".into()),
            started: Some("2026-10-19T14:02:11Z".into()),
            ended: None,
            turns: 31,
            cwd: Some(ORION.into()),
        },
        entries: vec![
            user("Trim the context the hook injects. Serve the brief on demand rather than pasting every standing decision into each prompt — feature #41, the tasks are on the board.", "14:02"),
            said("I'll read how the hook builds its context today, then the board's brief for #41.", "14:02"),
            tool("Read", "crates/argus-hook/src/context.rs", false),
            tool("Grep", "standing_decisions in crates/ — 5 files", false),
            said("Every prompt pays for 23 standing decisions today, about 10.8k tokens. I'll add a `/brief` route to argusd first and test it before the hook uses it.", "14:06"),
            tool("Write", "crates/argusd/src/hook/brief.rs", false),
            tool("Edit", "crates/argusd/src/board/standing.rs", false),
            tool("Bash", "cargo test -p argusd --locked brief", true),
            said("One test fails: a pane whose feature is done still gets a brief. A done feature binds nothing, so the route refuses it.", "14:19"),
            tool("Edit", "crates/argusd/src/hook/brief.rs", false),
            tool("Bash", "cargo test -p argusd --locked brief — 14 passed", false),
            user("Good. Cap the history too, but ask me before choosing a number.", "14:31"),
            said("Raised decision #3 under task 5 with three options; I'll wire the hook to the route meanwhile.", "14:31"),
            tool("Edit", "crates/argus-hook/src/context.rs", false),
        ],
        earlier: 0,
    })
}

// ---- Views and extensions -----------------------------------------------------

fn text(text: &str) -> Json {
    json!({"kind": "text", "text": text})
}

fn number(number: f64, unit: Option<&str>) -> Json {
    json!({"kind": "number", "number": number, "unit": unit})
}

fn chip(label: &str, tone: &str) -> Json {
    json!({"kind": "chip", "chip": {"label": label, "tone": tone}})
}

fn shown_views() -> Vec<ServerMsg> {
    let progress = json!({
        "id": "trim-progress",
        "title": "Trim context · progress",
        "status": {"label": "in progress", "tone": "active"},
        "feature": 41,
        "actions": [{"id": "measure", "label": "Measure again"}],
        "blocks": [
            {"kind": "stats", "stats": [
                {"label": "Prompt context", "value": "3.1k", "delta": "−71%", "tone": "good", "trend": [10.8, 10.9, 11.2, 9.4, 6.1, 3.4, 3.1]},
                {"label": "Standing decisions", "value": "23", "delta": "+2 this week", "tone": "neutral"},
                {"label": "Brief requests", "value": "148", "tone": "info", "trend": [0, 0, 12, 40, 61, 98, 148]},
                {"label": "Tests", "value": "526 / 526", "tone": "good"}
            ]},
            {"kind": "chart", "shape": "bar", "x": ["start", "plan", "edit", "test", "review", "commit"], "unit": "tokens",
             "series": [
                {"name": "injected", "values": [11240, 10960, 11410, 11020, 10880, 11120], "tone": "neutral"},
                {"name": "brief", "values": [3180, 2940, 3260, 3010, 2890, 3120], "tone": "good"}
             ]},
            {"kind": "columns", "columns": [
                [{"kind": "section", "heading": "Tasks", "blocks": [{"kind": "checklist", "items": [
                    {"label": "Add a brief route to argusd", "done": true},
                    {"label": "Measure brief sizes on the orion board", "done": true},
                    {"label": "The hook reads the brief from the route", "done": false, "note": "wiring context.rs"},
                    {"label": "Drop the injected decisions from the prompt", "done": false},
                    {"label": "Cap the history a brief carries", "done": false, "note": "waits on decision #3"}
                ]}]}],
                [{"kind": "section", "heading": "Changed", "blocks": [{"kind": "table",
                    "columns": [{"title": "File"}, {"title": "+", "numeric": true}, {"title": "−", "numeric": true}],
                    "rows": [
                        [{"kind": "path", "path": "crates/argusd/src/hook/brief.rs"}, number(31.0, None), number(0.0, None)],
                        [{"kind": "path", "path": "crates/argus-hook/src/context.rs"}, number(11.0, None), number(7.0, None)],
                        [{"kind": "path", "path": "crates/argusd/src/board/standing.rs"}, number(10.0, None), number(0.0, None)],
                        [{"kind": "path", "path": "DESIGN.md"}, number(4.0, None), number(2.0, None)]
                    ]}]}]
            ]},
            {"kind": "callout", "tone": "warn", "text": "Decision #3 is still open: how much history should a brief carry? Task 5 waits on it."}
        ]
    });
    let scenes = json!({
        "id": "screens",
        "title": "Desktop screenshots",
        "status": {"label": "ready for review", "tone": "good"},
        "feature": 42,
        "blocks": [
            {"kind": "progress", "label": "Scenes photographed", "value": 34, "max": 36, "tone": "active", "note": "float and docks left"},
            {"kind": "table", "columns": [{"title": "Scene"}, {"title": "Stage"}, {"title": "State"}],
             "rows": [
                [text("workspace-agent"), text("Workspace"), chip("drawn", "good")],
                [text("feature-flows"), text("Feature"), chip("drawn", "good")],
                [text("review-comment"), text("Review"), chip("drawn", "good")],
                [text("float"), text("Saved view"), chip("pending", "warn")]
             ]}
        ]
    });
    let mutants = json!({
        "id": "mutants",
        "title": "Mutants · store::faults",
        "status": {"label": "3 survived", "tone": "bad"},
        "blocks": [
            {"kind": "stats", "stats": [
                {"label": "Caught", "value": "41", "tone": "good"},
                {"label": "Survived", "value": "3", "tone": "bad"},
                {"label": "Unviable", "value": "6", "tone": "neutral"}
            ]},
            {"kind": "log", "lines": [
                {"level": "error", "text": "MISSED faults.rs:88: replace FaultPlan::due -> bool with true"},
                {"level": "error", "text": "MISSED faults.rs:121: replace < with <= in Journal::torn_at"},
                {"level": "error", "text": "MISSED faults.rs:140: delete ! in Journal::replay"},
                {"level": "info", "text": "41 caught, 6 unviable in 3m 12s"}
            ]}
        ]
    });
    [(P_TRIM, progress, 120), (P_SCREENS, scenes, 600), (P_FAULTS, mutants, 1_300)]
        .into_iter()
        .map(|(pane, view, secs)| {
            let view = serde_json::from_value(view).unwrap();
            ServerMsg::ViewShown(Box::new(argus_protocol::ShownView { pane: PaneId(pane), at: ago(secs), subject: None, view }))
        })
        .collect()
}

fn saved_views() -> Vec<argus_protocol::SavedView> {
    let release = json!({
        "id": "release-0-34",
        "scope": {"kind": "repository", "repository": ARGUS},
        "recipe": "What stands between main and 0.34: open features, failing checks, unreleased commits.",
        "built_at": ago(3 * 3_600),
        "built_commit": "6ce099a",
        "view": {
            "id": "release-0-34",
            "title": "Release 0.34",
            "status": {"label": "2 blockers", "tone": "warn"},
            "blocks": [
                {"kind": "progress", "label": "Features done", "value": 3, "max": 6, "tone": "active"},
                {"kind": "timeline", "events": [
                    {"at": "Oct 12", "title": "0.33.5 released", "tone": "good"},
                    {"at": "Oct 19", "title": "0.33.6 released", "tone": "good", "body": "Stops capping an agent's memory"},
                    {"at": "Oct 22", "title": "Trim context lands", "tone": "active"},
                    {"at": "Oct 26", "title": "0.34 cut", "tone": "info"}
                ]},
                {"kind": "section", "heading": "Blockers", "blocks": [{"kind": "list", "items": [
                    {"title": "Trim context injection", "body": "decision #3 is open", "chip": {"label": "#41", "tone": "active"}},
                    {"title": "Fault injection for the store", "body": "3 mutants survived", "chip": {"label": "#44", "tone": "bad"}}
                ]}]},
                {"kind": "matrix", "rows": ["Windows", "Linux", "macOS"], "columns": ["build", "test", "clippy", "fuzz"],
                 "cells": [
                    [{"tone": "good", "weight": 1}, {"tone": "good", "weight": 1}, {"tone": "good", "weight": 1}, {"tone": "neutral", "weight": 0.2, "label": "n/a"}],
                    [{"tone": "good", "weight": 1}, {"tone": "good", "weight": 1}, {"tone": "good", "weight": 1}, {"tone": "good", "weight": 1}],
                    [{"tone": "good", "weight": 1}, {"tone": "warn", "weight": 0.6, "label": "1 flaky"}, {"tone": "good", "weight": 1}, {"tone": "neutral", "weight": 0.2, "label": "n/a"}]
                 ]}
            ]
        }
    });
    let architecture = json!({
        "id": "architecture",
        "scope": {"kind": "global"},
        "built_at": ago(6 * 86_400),
        "stale": true,
        "view": {
            "id": "architecture",
            "title": "How Argus fits together",
            "blocks": [
                {"kind": "graph", "direction": "right",
                 "nodes": [
                    {"id": "argus", "label": "argus (TUI)"}, {"id": "desktop", "label": "argus-desktop"},
                    {"id": "argusd", "label": "argusd", "tone": "active"}, {"id": "hook", "label": "argus-hook"},
                    {"id": "agents", "label": "agents"}
                 ],
                 "edges": [
                    {"from": "argus", "to": "argusd", "label": "protocol"}, {"from": "desktop", "to": "argusd", "label": "bridge"},
                    {"from": "agents", "to": "hook"}, {"from": "hook", "to": "argusd", "label": "pane API"}
                 ]},
                {"kind": "markdown", "text": "Every binary agrees on **argus-protocol**; the client never predicts a request's result."}
            ]
        }
    });
    [release, architecture].into_iter().map(|v| serde_json::from_value(v).unwrap()).collect()
}

fn extension_views() -> ServerMsg {
    let running = |name: &str, image: &str, ports: &str, cpu: f64, mem: f64| {
        json!({"key": name, "status": {"label": "running", "tone": "good"},
               "cells": [text(name), text(image), chip("running", "good"), text(ports), number(cpu, Some("%")), number(mem, Some("MiB"))]})
    };
    let views = json!([{
        "extension": "docker",
        "name": "containers",
        "title": "Containers",
        "icon": "box",
        "columns": [{"title": "Name"}, {"title": "Image"}, {"title": "State"}, {"title": "Ports"}, {"title": "CPU", "numeric": true}, {"title": "Memory", "numeric": true}],
        "rows": [
            running("api-web", "work/api:dev", "8080→80", 3.4, 212.0),
            running("api-worker", "work/api:dev", "—", 11.8, 388.0),
            running("postgres", "postgres:16-alpine", "5432→5432", 0.6, 74.0),
            running("redis", "redis:7.4", "6379→6379", 0.2, 9.0),
            running("jaeger", "jaegertracing/all-in-one:1.62", "16686→16686", 1.1, 96.0),
            {"key": "minio", "status": {"label": "exited (137)", "tone": "bad"},
             "cells": [text("minio"), text("minio/minio:latest"), chip("exited 137", "bad"), text("—"), number(0.0, Some("%")), number(0.0, Some("MiB"))]}
        ],
        "actions": [
            {"id": "logs", "label": "Logs", "key": "l"},
            {"id": "restart", "label": "Restart", "key": "r"},
            {"id": "stop", "label": "Stop", "key": "s", "confirm": "Stop this container?"},
            {"id": "shell", "label": "Shell", "key": "e"},
            {"id": "prune", "label": "Prune stopped", "view_wide": true}
        ],
        "default": "logs"
    }]);
    ServerMsg::ExtensionViews(serde_json::from_value(views).unwrap())
}

fn extension_commands() -> ServerMsg {
    let commands = json!([
        {"extension": "docker", "name": "up", "title": "Compose up this checkout", "scope": "checkout", "key": {"key": "u", "alt": true}},
        {"extension": "docker", "name": "prune", "title": "Prune stopped containers"}
    ]);
    ServerMsg::ExtensionCommands(serde_json::from_value(commands).unwrap())
}

// ---- What the panes show ------------------------------------------------------

/// A row's cells from text marked with `«x»` style switches.
fn styled(line: &str, cols: usize) -> Vec<Cell> {
    let mut cells = Vec::new();
    let mut style = Cell::default();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '«' {
            let key: String = chars.by_ref().take_while(|&c| c != '»').collect();
            style = ink(&key);
        } else if cells.len() < cols {
            cells.push(Cell { ch: c.to_string().into(), ..style.clone() });
        }
    }
    cells.resize(cols, Cell::default());
    cells
}

fn ink(key: &str) -> Cell {
    let fg = |fg: Color| Cell { fg, ..Cell::default() };
    match key {
        "o" => fg(Color::Rgb(215, 119, 87)),
        "O" => Cell { bold: true, ..fg(Color::Rgb(215, 119, 87)) },
        "d" => fg(Color::Rgb(136, 136, 136)),
        "i" => Cell { italic: true, ..fg(Color::Rgb(136, 136, 136)) },
        "g" => fg(Color::Rgb(78, 186, 101)),
        "r" => fg(Color::Rgb(255, 107, 128)),
        "b" => Cell { bold: true, ..Cell::default() },
        "+" => Cell { bg: Color::Rgb(34, 92, 43), ..Cell::default() },
        "-" => Cell { bg: Color::Rgb(122, 41, 54), ..Cell::default() },
        "p" => fg(Color::Rgb(177, 185, 249)),
        "G" => Cell { bold: true, ..fg(Color::Idx(10)) },
        "Y" => Cell { bold: true, ..fg(Color::Idx(11)) },
        "R" => Cell { bold: true, ..fg(Color::Idx(9)) },
        "2" => fg(Color::Idx(2)),
        "1" => fg(Color::Idx(1)),
        "3" => fg(Color::Idx(3)),
        "4" => fg(Color::Idx(4)),
        "6" => fg(Color::Idx(6)),
        "8" => fg(Color::Idx(8)),
        "inv" => Cell { reverse: true, ..Cell::default() },
        _ => Cell::default(),
    }
}

/// A box of `width` around `inside`, its border in `border`.
fn boxed(border: &str, inside: &[&str], width: usize) -> Vec<String> {
    let visible = |s: &str| {
        let mut n = 0;
        let mut marked = false;
        for c in s.chars() {
            match c {
                '«' => marked = true,
                '»' => marked = false,
                _ if !marked => n += 1,
                _ => {}
            }
        }
        n
    };
    let mut lines = vec![format!("{border}╭{}╮«/»", "─".repeat(width - 2))];
    for line in inside {
        let pad = (width - 4).saturating_sub(visible(line));
        lines.push(format!("{border}│«/» {line}{} {border}│«/»", " ".repeat(pad)));
    }
    lines.push(format!("{border}╰{}╯«/»", "─".repeat(width - 2)));
    lines
}

/// What a pane's screen holds, and the cursor, as the lines it scrolled to.
fn screen_of(pane: u64, cols: usize) -> (Vec<String>, (usize, usize)) {
    match pane {
        P_TRIM => claude_trim(cols),
        P_SCREENS => claude_screens(cols),
        P_RATATUI => codex_waiting(),
        P_FAULTS => claude_faults(cols),
        P_FAULTS_TEST => cargo_failed(),
        P_SHELL => cargo_passed(),
        C_CONSOLE => console_git(),
        C_DEV => npm_dev(),
        P_API_AGENT => claude_api(cols),
        _ => prompt(r"C:\Source\work\api", Vec::new()),
    }
}

fn prompt(cwd: &str, mut lines: Vec<String>) -> (Vec<String>, (usize, usize)) {
    let at = format!("PS {cwd}> ");
    let col = at.chars().count();
    lines.push(format!("«2»PS«/» {cwd}> "));
    (lines.clone(), (lines.len() - 1, col))
}

fn prompt_box(cols: usize, below: &str) -> Vec<String> {
    let mut lines = boxed("«d»", &["«b»>«/» "], cols);
    lines.push(format!("  {below}"));
    lines
}

fn claude_trim(cols: usize) -> (Vec<String>, (usize, usize)) {
    let mut lines = boxed("«o»", &[
        "«o»✻«/» Welcome to «b»Claude Code«/»!",
        "",
        "  «i»/help for help, /status for your current setup«/»",
        "",
        &format!("  «i»cwd: {ORION}«/»"),
    ], 58);
    lines.extend([
        "",
        "«d»>«/» Trim the context the hook injects. Serve the brief on demand rather than pasting every standing decision",
        "  into each prompt — feature #41, the tasks are on the board.",
        "",
        "«o»●«/» I'll read how the hook builds its context today, then the board's brief for #41.",
        "",
        "«g»●«/» «b»Read«/»(crates/argus-hook/src/context.rs)",
        "  ⎿  Read «b»186«/» lines «d»(ctrl+r to expand)«/»",
        "",
        "«g»●«/» «b»Search«/»(pattern: \"standing_decisions\", path: \"crates\")",
        "  ⎿  Found «b»5«/» files «d»(ctrl+r to expand)«/»",
        "",
        "«o»●«/» Every prompt pays for 23 standing decisions today, about 10.8k tokens. The route is in; now the hook side.",
        "",
        "«g»●«/» «b»Update«/»(crates/argus-hook/src/context.rs)",
        "  ⎿  Updated «b»crates/argus-hook/src/context.rs«/» with «b»5«/» additions and «b»7«/» removals",
        "       «d»55«/» «-»-    // Every standing decision rides along, so an agent never acts                «/»",
        "       «d»56«/» «-»-    let board = client.board().await?;                                             «/»",
        "       «d»57«/» «-»-    for decision in board.standing() {                                             «/»",
        "       «d»55«/» «+»+    // The brief of the pane's feature, asked for now: a pane with no               «/»",
        "       «d»56«/» «+»+    if let Some(brief) = client.brief(pane).await? {                                 «/»",
        "       «d»57«/» «+»+        prompt.push_str(&brief.summary());                                           «/»",
        "",
        "«g»●«/» «b»Bash«/»(cargo test -p argusd --locked brief)",
        "  ⎿  test hook::brief::tests::a_pane_without_a_feature_is_refused ... «g»ok«/»",
        "     test result: «g»ok«/». 14 passed; 0 failed; 0 ignored; 0 measured; 512 filtered out",
        "",
        "«g»●«/» «b»Task«/»(Explore · hook callers)",
        "  ⎿  Found 3 callers of standing(): argusd/src/board/brief.rs, argus-hook/src/context.rs, argus/src/ui/brief.rs",
        "",
        "«o»✶ Dropping the injected decisions from the prompt…«/» «d»(3m 12s · ↓ 6.8k tokens · esc to interrupt)«/»",
        "",
    ].map(String::from));
    lines.extend(prompt_box(cols, "«p»⏵⏵ accept edits on«/» «d»(shift+tab to cycle)«/»"));
    let at = lines.len() - 3;
    (lines, (at, 4))
}

fn claude_screens(cols: usize) -> (Vec<String>, (usize, usize)) {
    let mut lines: Vec<String> = [
        "«g»●«/» «b»Bash«/»(cargo test -p argus-desktop-core --locked write_screens -- --ignored)",
        "  ⎿  running 1 test",
        "     test screens::write_screens ... «g»ok«/»",
        "     test result: «g»ok«/». 1 passed; 0 failed; 0 ignored; 0 measured; 433 filtered out; finished in 1.92s",
        "",
        "«g»●«/» «b»Bash«/»(cargo clippy -p argus-desktop --all-targets --locked -- -D warnings)",
        "  ⎿      Checking argus-desktop v0.33.6",
        "         Finished `dev` profile [unoptimized + debuginfo] target(s) in 18.40s",
        "",
        "«o»●«/» Done. The harness writes «b»34 scenes«/» to «b»page/test/screens/scenes.json«/».",
        "",
        "  «b»What changed«/»",
        "  • «b»screens.rs«/» brings a session to each screen the window shows and records what a fresh page hears there",
        "  • a stand-in daemon answers every request a scene's calls send, from one tree of the Argus project",
        "  • DESIGN.md has the module's row; design_table passes",
        "",
        "  «b»Not reachable from the session«/»",
        "  • a dialog only the page opens is opened by the harness with a click",
        "",
        "  «d»cargo test -p argus-desktop-core --locked 434 passed«/»",
        "  «d»cargo clippy … -D warnings               clean«/»",
        "",
    ]
    .map(String::from)
    .to_vec();
    lines.extend(prompt_box(cols, "«d»? for shortcuts«/»"));
    let at = lines.len() - 3;
    (lines, (at, 4))
}

fn claude_faults(cols: usize) -> (Vec<String>, (usize, usize)) {
    let mut lines: Vec<String> = [
        "«g»●«/» «b»Bash«/»(git diff > change.diff && cargo mutants -p argusd --in-diff change.diff --copy-target=false -j 4)",
        "  ⎿  «r»Error: 3 mutants survived«/»",
        "     Found 50 mutants to test",
        "     ok       Unmutated baseline in 41.2s build + 28.7s test",
        "     «r»MISSED«/»   crates/argusd/src/store/faults.rs:88:9: replace FaultPlan::due -> bool with true",
        "     «r»MISSED«/»   crates/argusd/src/store/faults.rs:121:24: replace < with <= in Journal::torn_at",
        "     «r»MISSED«/»   crates/argusd/src/store/faults.rs:140:12: delete ! in Journal::replay",
        "     50 mutants tested in 3m 12s: 3 missed, 41 caught, 6 unviable",
        "",
        "«o»●«/» Two of these change what a reader would see, but DESIGN.md says nothing about a torn entry at the",
        "  journal's very end: dropped, or a refused start? I won't pin either without that answer.",
        "",
        "«g»●«/» «b»argus-hook decide«/»(\"Should a torn journal entry be dropped or refuse the start?\")",
        "  ⎿  Raised decision #6 under store-faults",
        "",
        "«r»●«/» «b»argus-hook status failed«/»(\"3 mutants survived in store::faults\")",
        "",
    ]
    .map(String::from)
    .to_vec();
    lines.extend(prompt_box(cols, "«d»? for shortcuts«/»"));
    let at = lines.len() - 3;
    (lines, (at, 4))
}

fn claude_api(cols: usize) -> (Vec<String>, (usize, usize)) {
    let mut lines: Vec<String> = [
        "«d»>«/» Send RateLimit-Remaining and Retry-After on every limited route",
        "",
        "«g»●«/» «b»Read«/»(src/limits/mod.rs)",
        "  ⎿  Read «b»142«/» lines",
        "",
        "«g»●«/» «b»Update«/»(src/limits/headers.rs)",
        "  ⎿  Updated «b»src/limits/headers.rs«/» with «b»22«/» additions",
        "",
        "«o»✶ Running the limits tests…«/» «d»(48s · esc to interrupt)«/»",
        "",
    ]
    .map(String::from)
    .to_vec();
    lines.extend(prompt_box(cols, "«d»? for shortcuts«/»"));
    let at = lines.len() - 3;
    (lines, (at, 4))
}

fn codex_waiting() -> (Vec<String>, (usize, usize)) {
    let lines: Vec<String> = [
        "«d»╭──────────────────────────────────────────────────────╮«/»",
        "«d»│«/» «b»>_ OpenAI Codex«/» «d»(v0.46.0)«/»                            «d»│«/»",
        "«d»│«/»                                                      «d»│«/»",
        "«d»│«/» model:     «b»gpt-5-codex«/»   «6»/model«/» to change            «d»│«/»",
        "«d»│«/» directory: ~\\orion\\.argus\\worktrees\\ratatui-bump    «d»│«/»",
        "«d»╰──────────────────────────────────────────────────────╯«/»",
        "",
        "«6»▌«/» Bump ratatui to 0.30 and fix what breaks",
        "",
        "«d»•«/» «b»Ran«/» cargo update -p ratatui --precise 0.30.0",
        "  «d»└«/» Updating crates.io index",
        "      Updating ratatui v0.29.0 -> v0.30.0",
        "      Adding ratatui-core v0.1.0",
        "",
        "«d»•«/» «b»Ran«/» cargo check -p argus",
        "  «d»└«/» «R»error[E0599]«/»: no method named `size` found for struct `Frame` in the current scope",
        "         --> crates/argus-client/src/ui/mod.rs:48:23",
        "      «d»… +11 lines«/»",
        "",
        "«d»•«/» ratatui 0.30 needs crossterm 0.29. Updating it moves the terminal client's key reading too.",
        "",
        "«3»Allow command?«/»",
        "",
        "  $ cargo update -p crossterm --precise 0.29.0",
        "",
        "«inv»▶ 1. Yes«/»   2. Always for this command   3. No, and tell Codex what to do differently",
        "",
        "«d»Press Enter to confirm or Esc to cancel«/»",
    ]
    .map(String::from)
    .to_vec();
    (lines, (25, 0))
}

fn cargo_passed() -> (Vec<String>, (usize, usize)) {
    let lines: Vec<String> = [
        format!("«2»PS«/» {ORION}> cargo test -p argusd --locked"),
        "   «G»Compiling«/» argus-protocol v0.33.6 (C:\\Source\\github.com\\hbaldwin98\\orion\\crates\\argus-protocol)".into(),
        "   «G»Compiling«/» argusd v0.33.6 (C:\\Source\\github.com\\hbaldwin98\\orion\\crates\\argusd)".into(),
        "    «G»Finished«/» `test` profile [unoptimized + debuginfo] target(s) in 1m 04s".into(),
        "     «G»Running«/» unittests src\\main.rs (target\\debug\\deps\\argusd-3f9c1e2a7b4d8e06.exe)".into(),
        String::new(),
        "running 526 tests".into(),
        "test board::standing::tests::a_withdrawn_decision_no_longer_stands ... «2»ok«/»".into(),
        "test board::standing::tests::binding_keeps_only_the_features_own ... «2»ok«/»".into(),
        "test hook::brief::tests::a_done_feature_binds_nothing ... «2»ok«/»".into(),
        "test hook::brief::tests::a_pane_without_a_feature_is_refused ... «2»ok«/»".into(),
        "test hook::brief::tests::the_brief_names_its_open_questions ... «2»ok«/»".into(),
        "test pty::job::tests::an_agent_job_kills_on_close_alone ... «2»ok«/»".into(),
        "test pty::tests::a_pane_survives_its_child_closing_stdin ... «2»ok«/»".into(),
        "test store::tests::a_torn_journal_tail_is_dropped_on_open ... «2»ok«/»".into(),
        "test store::tests::restores_session_after_reconnect ... «2»ok«/»".into(),
        "test views::saved::tests::a_stale_view_is_rebuilt_on_refresh ... «2»ok«/»".into(),
        "«d»…«/»".into(),
        String::new(),
        "test result: «2»ok«/». 526 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 8.41s".into(),
        String::new(),
        "     «G»Running«/» tests\\protocol.rs (target\\debug\\deps\\protocol-a91c0d2e4f6b8a13.exe)".into(),
        String::new(),
        "running 18 tests".into(),
        "test result: «2»ok«/». 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.37s".into(),
        String::new(),
    ]
    .to_vec();
    prompt(ORION, lines)
}

fn cargo_failed() -> (Vec<String>, (usize, usize)) {
    let lines: Vec<String> = [
        "     «G»Running«/» unittests src\\main.rs (target\\debug\\deps\\argusd-3f9c1e2a7b4d8e06.exe)",
        "",
        "running 529 tests",
        "test store::faults::tests::a_full_disk_refuses_the_write ... «2»ok«/»",
        "test store::faults::tests::a_lost_rename_keeps_the_old_file ... «2»ok«/»",
        "test store::faults::tests::a_torn_entry_at_the_tail_is_dropped ... «R»FAILED«/»",
        "",
        "failures:",
        "",
        "---- store::faults::tests::a_torn_entry_at_the_tail_is_dropped stdout ----",
        "thread 'store::faults::tests::a_torn_entry_at_the_tail_is_dropped' panicked at crates\\argusd\\src\\store\\faults.rs:212:9:",
        "assertion `left == right` failed: the entry before the tear survives",
        "  left: 40",
        " right: 41",
        "",
        "failures:",
        "    store::faults::tests::a_torn_entry_at_the_tail_is_dropped",
        "",
        "test result: «R»FAILED«/». 528 passed; 1 failed; 3 ignored; 0 measured; 0 filtered out; finished in 9.02s",
        "",
        "«R»error«/»: test failed, to rerun pass `-p argusd --bin argusd`",
        "",
        "«d»[process exited with code 101]«/»",
    ]
    .map(String::from)
    .to_vec();
    (lines, (22, 0))
}

fn console_git() -> (Vec<String>, (usize, usize)) {
    let lines: Vec<String> = [
        format!("«2»PS«/» {ORION}> git status -sb"),
        "## «2»main«/»...«1»origin/main«/»".into(),
        " «1»M«/» DESIGN.md".into(),
        " «1»M«/» crates/argus-hook/src/context.rs".into(),
        " «1»M«/» crates/argusd/src/board/standing.rs".into(),
        "«1»??«/» crates/argusd/src/hook/brief.rs".into(),
        format!("«2»PS«/» {ORION}> argus server status"),
        "argusd 0.33.6 · up 5h 12m · 11 panes in 2 projects · 3 extensions".into(),
    ]
    .to_vec();
    prompt(ORION, lines)
}

fn npm_dev() -> (Vec<String>, (usize, usize)) {
    let lines: Vec<String> = [
        "«6»> argus-desktop-page@0.33.6 dev«/»",
        "«6»> esbuild src/main.ts --bundle --outdir=../assets/js --watch«/»",
        "",
        "[watch] build finished, watching for changes...",
        "[watch] build started (change: \"src/board/flows.ts\")",
        "  «2»../assets/js/main.js«/»  412.8kb",
        "[watch] build finished",
    ]
    .map(String::from)
    .to_vec();
    let at = lines.len();
    (lines, (at, 0))
}

/// An editor open on `session.rs`, its rows highlighted as the review's are.
fn editor_rows(rows: usize, cols: usize) -> Vec<Vec<Cell>> {
    let source = read_file("crates/argus-desktop-core/src/session.rs");
    let colour = |kind: HighlightKind| match kind {
        HighlightKind::Keyword => Color::Rgb(203, 166, 247),
        HighlightKind::Str => Color::Rgb(166, 227, 161),
        HighlightKind::Comment => Color::Rgb(127, 132, 156),
        HighlightKind::Number | HighlightKind::Constant => Color::Rgb(250, 179, 135),
        HighlightKind::Type => Color::Rgb(249, 226, 175),
        HighlightKind::Function => Color::Rgb(137, 180, 250),
        HighlightKind::Property => Color::Rgb(148, 226, 213),
        HighlightKind::Operator | HighlightKind::Punctuation => Color::Rgb(137, 220, 235),
    };
    let first = 196;
    let mut grid: Vec<Vec<Cell>> = source
        .lines()
        .skip(first)
        .take(rows - 2)
        .enumerate()
        .map(|(i, line)| {
            let mut cells = styled(&format!("«d»{:>5} «/»", first + i + 1), cols);
            cells.truncate(6);
            let mut fg: Vec<Color> = vec![Color::Default; line.len()];
            for span in highlight(line) {
                for c in &mut fg[span.start as usize..span.end as usize] {
                    *c = colour(span.kind);
                }
            }
            for (at, ch) in line.char_indices() {
                cells.push(Cell { ch: ch.to_string().into(), fg: fg[at], ..Cell::default() });
            }
            cells.resize(cols, Cell::default());
            cells.truncate(cols);
            cells
        })
        .collect();
    let mode = format!("«inv» NORMAL «/» crates\\argus-desktop-core\\src\\session.rs{}rust  utf-8  {}:9 ", " ".repeat(cols.saturating_sub(70)), first + 4);
    grid.push(styled(&mode, cols));
    grid.push(styled("", cols));
    grid
}

fn rows_msg(pane: u64, rows: u16, cols: u16) -> ServerMsg {
    let (r, c) = (rows as usize, cols as usize);
    let (grid, cursor) = if pane == P_EDITOR {
        (editor_rows(r, c), (3, 9))
    } else {
        let (lines, (at, col)) = screen_of(pane, c);
        let skip = lines.len().saturating_sub(r);
        let mut grid: Vec<Vec<Cell>> = lines[skip..].iter().map(|l| styled(l, c)).collect();
        grid.resize(r, vec![Cell::default(); c]);
        (grid, (at.saturating_sub(skip), col))
    };
    ServerMsg::PaneRows {
        pane: PaneId(pane),
        rows,
        cols,
        runs: grid_runs(&grid),
        cursor: Cursor { row: cursor.0 as u16, col: cursor.1 as u16, visible: pane != P_FAULTS_TEST, ..Cursor::default() },
        mouse: Default::default(),
        alternate_screen: pane == P_EDITOR,
        keyboard: Default::default(),
    }
}

// ---- The stand-in daemon ------------------------------------------------------

/// Feeds `s` what argusd would answer each request in `sent` with, and in
/// turn whatever those answers make it ask, until it asks nothing more: a
/// live click that opens a review or unfolds a commit gets its data.
pub fn answer(s: &mut Session, sent: &[ClientMsg]) {
    let mut asked: VecDeque<ClientMsg> = sent.iter().cloned().collect();
    let mut answered = 0;
    while let Some(msg) = asked.pop_front() {
        answered += 1;
        assert!(answered < 1_000, "the window and the daemon talk forever");
        for reply in reply(&msg) {
            asked.extend(s.receive(reply));
        }
    }
}

/// What a folder of a checkout holds on disk, for the files panel: the
/// shape of this repository, with a build directory git ignores and some
/// changes under `crates`.
fn folder(path: &str) -> Vec<CheckoutEntry> {
    let dir = |name: &str, git: Option<EntryGit>| CheckoutEntry { name: name.into(), dir: true, size: None, modified: Some(NOW - 3_600), git };
    let file = |name: &str, size: u64, ago: u64, git: Option<EntryGit>| CheckoutEntry {
        name: name.into(),
        dir: false,
        size: Some(size),
        modified: Some(NOW - ago),
        git,
    };
    match path {
        "" => vec![
            dir(".argus", None),
            dir("crates", Some(EntryGit::Changed)),
            dir("scripts", None),
            dir("target", Some(EntryGit::Ignored)),
            dir("target-check", Some(EntryGit::Ignored)),
            file("AGENTS.md", 9_400, 86_400 * 3, None),
            file("Cargo.lock", 148_000, 86_400, None),
            file("DESIGN.md", 301_000, 600, Some(EntryGit::Changed)),
            file("README.md", 27_800, 86_400 * 9, None),
        ],
        "crates" => vec![
            dir("argus-client", None),
            dir("argus-desktop", Some(EntryGit::Changed)),
            dir("argus-protocol", None),
            dir("argusd", None),
            file("Cargo.toml", 1_200, 86_400 * 4, None),
        ],
        "crates/argus-desktop" => vec![
            dir("assets", Some(EntryGit::Changed)),
            dir("page", None),
            dir("src", None),
            file("Cargo.toml", 2_100, 86_400, None),
        ],
        "crates/argus-desktop/assets" => vec![
            dir("fonts", None),
            dir("js", Some(EntryGit::Changed)),
            file("index.html", 4_300, 86_400 * 2, None),
            file("style.css", 212 * 1024, 240, Some(EntryGit::Changed)),
        ],
        _ => Vec::new(),
    }
}

/// What argusd would answer `msg` with, given this tree.
fn reply(msg: &ClientMsg) -> Vec<ServerMsg> {
    match msg {
        ClientMsg::Resize { pane, rows, cols } => vec![rows_msg(pane.0, *rows, *cols)],
        ClientMsg::WatchViews => shown_views(),
        ClientMsg::WatchSavedViews => vec![ServerMsg::SavedViews(saved_views())],
        ClientMsg::GetDecisions { project, .. } if *project == ProjectId(1) => vec![ServerMsg::Decisions(Box::new(board()))],
        ClientMsg::GetDecisions { .. } => {
            let empty = DecisionBoard { project: Some(ProjectId(2)), name: "work".into(), features: Vec::new(), decisions: Vec::new(), tracker_map: Default::default(), repositories: vec![RepositoryId(20)] };
            vec![ServerMsg::Decisions(Box::new(empty))]
        }
        ClientMsg::Task { feature, action: TaskAction::List, .. } => vec![ServerMsg::Tasks(Box::new(tasks(feature)))],
        ClientMsg::SequenceDiagram { feature, action: DiagramAction::List, .. } => vec![ServerMsg::SequenceDiagrams(Box::new(diagrams(feature)))],
        ClientMsg::ListDir { request_id, checkout, path } => vec![ServerMsg::CheckoutDir(CheckoutDir {
            request_id: *request_id,
            checkout: *checkout,
            path: path.clone(),
            entries: folder(path),
            truncated: false,
        })],
        ClientMsg::ListCode { request_id, checkout, version } => vec![ServerMsg::CodeTree(CodeTree {
            request_id: *request_id,
            checkout: *checkout,
            version: version.clone(),
            files: files(),
            changes: changed(&working_changes(ReviewBase::Unstaged)),
        })],
        ClientMsg::ReadCode { request_id, checkout, path, version } => {
            let source = read_file(path);
            let lines: Vec<&str> = source.lines().collect();
            vec![ServerMsg::Code(Box::new(CodeFile {
                request_id: *request_id,
                checkout: *checkout,
                path: path.clone(),
                version: version.clone(),
                lines: lines.iter().map(|l| CodeLine { text: l.to_string(), spans: spans_for(path, l), added: false }).collect(),
                removed: Vec::new(),
                symbols: if path.ends_with(".rs") { outline(&lines) } else { Vec::new() },
                note: None,
            }))]
        }
        ClientMsg::SearchCode { request_id, checkout, query, options } => {
            let matches: Vec<CodeMatch> = files()
                .iter()
                .filter(|f| f.ends_with(".rs"))
                .flat_map(|path| {
                    read_file(path)
                        .lines()
                        .enumerate()
                        .filter_map(|(i, line)| {
                            let at = line.find(query.as_str())?;
                            Some(CodeMatch { path: path.clone(), line: i as u32 + 1, text: line.to_string(), ranges: vec![(at as u32, (at + query.len()) as u32)] })
                        })
                        .collect::<Vec<_>>()
                })
                .take(80)
                .collect();
            vec![ServerMsg::CodeMatches(CodeMatches { request_id: *request_id, checkout: *checkout, query: query.clone(), options: options.clone(), matches, truncated: false })]
        }
        ClientMsg::Review { request_id, checkout, base, commit } => {
            let (files, info) = match commit {
                Some(oid) => (commit_changes(), history().into_iter().find(|c| &c.oid == oid)),
                None => (working_changes(*base), None),
            };
            vec![ServerMsg::Review(Review { request_id: *request_id, checkout: *checkout, base: *base, files, commit: info })]
        }
        ClientMsg::ListCommits { request_id, checkout } => vec![ServerMsg::Commits { request_id: *request_id, checkout: *checkout, commits: history() }],
        ClientMsg::ListCommitFiles { checkout, commit } => {
            let file = |path: &str, added, removed| CommitFile { path: path.into(), old_path: None, kind: ChangeKind::Modified, added, removed };
            let files = if commit.starts_with("5cd8db7") {
                vec![file("crates/argusd/src/pty/job.rs", 9, 23), file("DESIGN.md", 3, 3), file("crates/argusd/src/pty/mod.rs", 0, 5), file("crates/argusd/src/pty/tests.rs", 7, 92)]
            } else {
                vec![file("Cargo.toml", 1, 1), file("Cargo.lock", 4, 4)]
            };
            vec![ServerMsg::CommitFiles { checkout: *checkout, commit: commit.clone(), files }]
        }
        ClientMsg::ListSessions { request_id, repository } => vec![ServerMsg::Sessions { request_id: *request_id, repository: *repository, sessions: agent_sessions() }],
        ClientMsg::ListResumes { request_id } => vec![ServerMsg::Resumes {
            request_id: *request_id,
            resumes: vec![
                Resume { harness: "claude".into(), argv: vec!["claude".into(), "--resume".into(), "{session_id}".into()] },
                Resume { harness: "codex".into(), argv: vec!["codex".into(), "resume".into(), "{session_id}".into()] },
            ],
        }],
        ClientMsg::ReadSession { request_id, session_id, .. } => vec![ServerMsg::SessionTranscript { request_id: *request_id, session_id: session_id.clone(), conversation: Box::new(transcript()) }],
        ClientMsg::ListDirectories { request_id, .. } => {
            let entry = |name: &str, is_repo| DirEntry { name: name.into(), is_repo };
            vec![ServerMsg::Directories(DirListing {
                request_id: *request_id,
                path: r"C:\Source\github.com\hbaldwin98".into(),
                parent: Some(r"C:\Source\github.com".into()),
                entries: vec![entry("dotfiles", true), entry("nebula-site", true), entry("orion", true), entry("scratch", false), entry("talks", false)],
                error: None,
            })]
        }
        ClientMsg::PreviewProject { request_id, path, .. } => vec![ServerMsg::ProjectPreview(ProjectPreview {
            request_id: *request_id,
            path: path.clone(),
            repositories: vec!["dotfiles".into(), "nebula-site".into(), "orion".into(), "orion/nebula".into()],
            depth: 8,
            always_skipped: vec![".git".into(), "node_modules".into(), "target".into()],
            error: None,
        })],
        _ => Vec::new(),
    }
}


// ---- Scenes -------------------------------------------------------------------

/// One screen the window shows: how a session gets there, and what the
/// page itself must do after it has heard everything.
pub struct Scene {
    /// Kebab-case and unique: the screenshot's file name.
    pub name: &'static str,
    pub title: &'static str,
    pub build: fn() -> Session,
    pub then: Vec<Step>,
}

/// A gesture only the page can make.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    /// A click on the element the CSS selector names.
    Click(&'static str),
}

/// A window and the daemon standing in for argusd, each request the window
/// sends answered before the next gesture.
struct Window {
    s: Session,
}

impl Window {
    /// A window that has just reached the daemon and heard its tree.
    fn connected() -> Window {
        let mut s = Session::default();
        let opening = vec![
            ServerMsg::Tree(tree()),
            ServerMsg::Consoles(consoles()),
            ServerMsg::Templates(vec!["claude".into(), "codex".into(), "gemini".into()]),
            ServerMsg::Workspaces(workspaces()),
            extension_views(),
            extension_commands(),
        ];
        let sent = s.connected(Some(Hello::this_build()), None, opening);
        answer(&mut s, &sent);
        Window { s }
    }

    /// Connected, with `pane` on the Workspace at the window's size.
    fn showing(pane: u64) -> Window {
        let mut w = Window::connected();
        w.call(Call::Resize { side: Side::Workspace, rows: ROWS, cols: COLS });
        w.call(Call::Select { pane });
        w
    }

    fn call(&mut self, call: Call) -> &mut Window {
        let sent = self.s.call(call);
        answer(&mut self.s, &sent);
        self
    }

    fn receive(&mut self, msg: ServerMsg) -> &mut Window {
        let sent = self.s.receive(msg);
        answer(&mut self.s, &sent);
        self
    }
}

/// A picture of a made-up page for a browser scene: a bar in `accent`, a
/// heading and lines of text, as a PNG in base64. The window decodes a
/// picture by its bytes, so a PNG stands in for the browser's JPEG.
fn page_shot(accent: [u8; 3]) -> String {
    const W: u32 = 640;
    const H: u32 = 400;
    let mut pixels = vec![255u8; (W * H * 3) as usize];
    let mut fill = |x0: u32, y0: u32, w: u32, h: u32, rgb: [u8; 3]| {
        for y in y0..(y0 + h).min(H) {
            for x in x0..(x0 + w).min(W) {
                let at = ((y * W + x) * 3) as usize;
                pixels[at..at + 3].copy_from_slice(&rgb);
            }
        }
    };
    fill(0, 0, W, 36, accent);
    fill(32, 64, 300, 22, [40, 40, 48]);
    for line in 0..8 {
        fill(32, 108 + line * 22, if line % 3 == 2 { 380 } else { 560 }, 10, [200, 202, 208]);
    }
    fill(32, 300, 120, 32, accent);
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, W, H);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
    }
    base64(&png)
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// The tree with each browser a pane in its agent's checkout, as the
/// daemon holds them.
fn tree_with_browsers(owners: &[(u64, u64)]) -> Vec<ProjectInfo> {
    let mut tree = tree();
    for &(browser, owner) in owners {
        let checkouts = tree.iter_mut().flat_map(|p| &mut p.repositories).flat_map(|r| &mut r.checkouts);
        let checkout = checkouts.into_iter().find(|c| c.panes.iter().any(|p| p.id == PaneId(owner))).expect("the owner is in the tree");
        checkout.panes.push(pane(browser, PaneKind::Browser, "Browser", PaneStatus::Idle, 60));
    }
    tree
}

/// Three agents each with a browser open, and a picture of each. The
/// window shows the first agent, so its browser opens beside it.
fn browsing() -> Window {
    browsing_beside(P_TRIM)
}

/// `browsing`, with the first agent's browser having no tab open: it is
/// listed, and has no picture (SHELL-BROWSER-10).
fn browsing_tabless() -> Window {
    browsing_with(P_TRIM, true)
}

/// `browsing`, with the window on `agent`, whose browser opens beside it.
fn browsing_beside(agent: u64) -> Window {
    browsing_with(agent, false)
}

fn browsing_with(agent: u64, tabless: bool) -> Window {
    use argus_protocol::{BrowserDriver, BrowserFrame, BrowserPage, BrowserTab};
    let mut w = Window::showing(agent);
    w.receive(ServerMsg::Tree(tree_with_browsers(&[(B_TRIM, P_TRIM), (B_SCREENS, P_SCREENS), (B_FAULTS, P_FAULTS)])));
    let open = [
        (B_TRIM, P_TRIM, "http://localhost:5173/settings", "Argus — Settings", [64, 112, 224]),
        (B_SCREENS, P_SCREENS, "https://docs.rs/tokio-tungstenite/latest/tokio_tungstenite/", "tokio_tungstenite - Rust", [56, 142, 60]),
        (B_FAULTS, P_FAULTS, "http://127.0.0.1:8080/login", "Sign in", [200, 90, 40]),
    ];
    for (pane, owner, url, title, accent) in open {
        if tabless && pane == B_TRIM {
            let page = BrowserPage { pane: PaneId(pane), owner: PaneId(owner), url: String::new(), title: String::new(), tabs: Vec::new(), shown: None, following: true, held: false, driver: None };
            w.receive(ServerMsg::BrowserPage(page));
            continue;
        }
        let tabs = vec![
            BrowserTab { id: format!("{pane}-docs"), url: "https://docs.rs/".into(), title: "Docs.rs".into() },
            BrowserTab { id: format!("{pane}-work"), url: url.into(), title: title.into() },
        ];
        let shown = Some(format!("{pane}-work"));
        // One agent at work in its browser, one the person has taken from
        // its agent, and one looked at on a tab the person picked.
        let driver = (owner == P_TRIM).then_some(BrowserDriver::Agent);
        let held = owner == P_FAULTS;
        let following = owner != P_SCREENS;
        w.receive(ServerMsg::BrowserPage(BrowserPage { pane: PaneId(pane), owner: PaneId(owner), url: url.into(), title: title.into(), tabs, shown, following, held, driver }));
        w.receive(ServerMsg::BrowserFrame(Box::new(BrowserFrame { pane: PaneId(pane), jpeg: page_shot(accent), width: 1280, height: 800 })));
    }
    w
}

fn on_feature(w: &mut Window, slug: &str) {
    w.call(Call::ShowFeatures { open: true }).call(Call::OpenFeature { slug: Some(slug.into()) });
}

/// The tracker dialogs after the daemon pushes the board again with `change` made to the open feature
/// and to the repository's map: the states a fixture board does not start in.
fn tracking(open: Call, change: impl FnOnce(&mut Feature, &mut argus_protocol::TrackerMap)) -> Window {
    let mut w = Window::showing(P_TRIM);
    on_feature(&mut w, "trim");
    w.call(open);
    let mut pushed = board();
    if let Some(feature) = pushed.features.iter_mut().find(|f| f.slug == "trim") {
        change(feature, &mut pushed.tracker_map);
    }
    w.receive(ServerMsg::Decisions(Box::new(pushed)));
    w
}

fn reconciling(change: impl FnOnce(&mut Feature)) -> Window {
    tracking(Call::OpenReconcile, |feature, _| change(feature))
}

fn linking(map: bool, change: impl FnOnce(&mut argus_protocol::TrackerMap)) -> Window {
    tracking(Call::OpenLink { map }, |_, tracker_map| change(tracker_map))
}

fn key(key: &str, code: &str, ctrl: bool) -> Call {
    Call::Key { key: PageKey { key: key.into(), code: code.into(), ctrl, ..PageKey::default() } }
}

/// The review of main's working changes, its cursor on a changed line.
fn reviewing() -> Window {
    let mut w = Window::showing(P_TRIM);
    w.call(Call::OpenReview { checkout: MAIN });
    for _ in 0..6 {
        w.call(Call::ReviewAction { action: Action::Next });
    }
    w
}

const CAPTURE_NOTE: &str = "Recent items in the palette

The palette forgets where you were. Offer the panes, features and checkouts last gone to above the fuzzy matches.

- Remember the last ten places gone to
  - Keep them per project
  - Forget a pane once it closes
- Rank recent items above fuzzy matches
  ? How many recents per group?
    3
    5
! Keep recents in the window, not the daemon
  another window is another person's place";

fn scene(name: &'static str, title: &'static str, build: fn() -> Session) -> Scene {
    Scene { name, title, build, then: Vec::new() }
}

pub fn scenes() -> Vec<Scene> {
    let mut all = main_scenes();
    all.extend(saved::scenes());
    all.extend(extension_kinds::scenes());
    all
}

fn main_scenes() -> Vec<Scene> {
    vec![
        scene("connecting", "Connecting — before any daemon answers", || {
            let mut s = Session::default();
            s.connecting(None);
            s
        }),
        scene("stopped", "Stopped — the daemon went away", || {
            let mut w = Window::showing(P_TRIM);
            w.s.stopped();
            w.s
        }),
        scene("workspace-agent", "Workspace — an agent at work", || Window::showing(P_TRIM).s),
        scene("workspace-shell", "Workspace — a shell after a test run", || Window::showing(P_SHELL).s),
        scene("workspace-waiting", "Workspace — an agent waiting on a yes", || Window::showing(P_RATATUI).s),
        scene("workspace-failed", "Workspace — an agent that failed", || Window::showing(P_FAULTS).s),
        scene("workspace-exited", "Workspace — a pane whose program exited", || Window::showing(P_FAULTS_TEST).s),
        scene("workspace-editor", "Workspace — an editor pane", || Window::showing(P_EDITOR).s),
        scene("workspace-split", "Workspace — two panes side by side", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenBeside { pane: P_SHELL })
                .call(Call::Resize { side: Side::Workspace, rows: ROWS, cols: COLS / 2 - 1 })
                .call(Call::Resize { side: Side::Beside(0), rows: ROWS, cols: COLS / 2 - 1 });
            w.s
        }),
        scene("workspace-grid", "Workspace — three panes: two above, one under both", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenBeside { pane: P_SHELL })
                .call(Call::OpenBeside { pane: P_SCREENS })
                .call(Call::Resize { side: Side::Workspace, rows: ROWS / 2 - 3, cols: COLS / 2 - 1 })
                .call(Call::Resize { side: Side::Beside(0), rows: ROWS / 2 - 3, cols: COLS / 2 - 1 })
                .call(Call::Resize { side: Side::Beside(1), rows: ROWS / 2 - 3, cols: COLS - 2 });
            w.s
        }),
        scene("workspace-grid-four", "Workspace — four panes, the keys on the last", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenBeside { pane: P_SHELL })
                .call(Call::OpenBeside { pane: P_SCREENS })
                .call(Call::OpenBeside { pane: P_EDITOR })
                .call(Call::Resize { side: Side::Workspace, rows: ROWS / 2 - 3, cols: COLS / 2 - 1 })
                .call(Call::Resize { side: Side::Beside(0), rows: ROWS / 2 - 3, cols: COLS / 2 - 1 })
                .call(Call::Resize { side: Side::Beside(1), rows: ROWS / 2 - 3, cols: COLS / 2 - 1 })
                .call(Call::Resize { side: Side::Beside(2), rows: ROWS / 2 - 3, cols: COLS / 2 - 1 })
                .call(Call::FocusView { pane: P_EDITOR });
            w.s
        }),
        scene("workspace-console", "Workspace — the console docked under the pane", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::Console { gesture: ConsoleGesture::Toggle })
                .call(Call::Resize { side: Side::Workspace, rows: 26, cols: COLS })
                .call(Call::Resize { side: Side::First, rows: 12, cols: COLS });
            w.s
        }),
        scene("workspace-empty", "Workspace — a project with no pane on screen", || {
            let mut w = Window::connected();
            w.call(Call::Resize { side: Side::Workspace, rows: ROWS, cols: COLS }).call(Call::SelectProject { project: 2 });
            w.s
        }),
        scene("new", "New — starting an agent", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowComposer { open: true });
            w.s
        }),
        scene("features-board", "Features — the board, peeking at one", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowFeatures { open: true }).call(Call::SetLayout { layout: Layout::Board }).call(Call::Peek { slug: "trim".into() });
            w.s
        }),
        scene("features-list", "Features — the list, peeking at one", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowFeatures { open: true }).call(Call::SetLayout { layout: Layout::List }).call(Call::Peek { slug: "store-faults".into() });
            w.s
        }),
        scene("feature-work", "Feature — the Work lens: tasks, decisions, criteria", || {
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(Call::SelectWork { at: WorkAt::Task { id: 3 } });
            w.s
        }),
        scene("feature-record", "Feature — the Record lens, a decision read as its record", || {
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(Call::SetLens { lens: Lens::Record }).call(Call::SelectDecision { id: 1 });
            w.s
        }),
        scene("feature-flows", "Feature — the Flows lens, a step selected", || {
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(Call::SetLens { lens: Lens::Flows }).call(Call::SelectStep { diagram: 11, step: Some(2) });
            w.s
        }),
        scene("capture", "Capture — a new feature written as one note", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowFeatures { open: true })
                .call(Call::Capture { gesture: CaptureGesture::Open })
                .call(Call::Capture { gesture: CaptureGesture::Note { note: CAPTURE_NOTE.into() } });
            w.s
        }),
        scene("link", "Link — a feature's tickets", || {
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(Call::OpenLink { map: false });
            w.s
        }),
        scene("reconcile", "Reconcile — an agent's proposal to bring the tickets in line", || {
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(Call::OpenReconcile);
            w.s
        }),
        scene("link-map", "Link — the tracker map's editor", || linking(true, |_| {}).s),
        scene("link-draft", "Link — an agent's draft of the tracker map waits", || {
            linking(false, |map| {
                map.body = "Linear: a feature is an issue, a task a sub-issue\nGitHub: a feature is a milestone".into();
                map.draft = Some(argus_protocol::MapDraft { body: "Linear: a feature is a project".into(), by: "pane #32".into(), at: NOW as i64 - 600 });
            })
            .s
        }),
        scene("link-typed", "Link — a URL typed, read as its tracker and key", || {
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(Call::OpenLink { map: false }).call(Call::Link {
                gesture: crate::board::tracker::link_dialog::LinkGesture::Fields { url: "https://acme.atlassian.net/browse/ARG-142".into(), role: "the PM's ticket".into() },
            });
            w.s
        }),
        scene("link-refused", "Link — a URL that names no ticket is said, not sent", || {
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(Call::OpenLink { map: false }).call(Call::Link {
                gesture: crate::board::tracker::link_dialog::LinkGesture::Fields { url: "ENG-88".into(), role: String::new() },
            });
            w.s
        }),
        scene("reconcile-ticked", "Reconcile — two lines ticked, the third open", || {
            use crate::board::tracker::reconcile_dialog::ReconcileGesture as G;
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(Call::OpenReconcile)
                .call(Call::Reconcile { gesture: G::Tick { line: 0 } })
                .call(Call::Reconcile { gesture: G::Tick { line: 2 } })
                .call(Call::Reconcile { gesture: G::Select { line: 2 } });
            w.s
        }),
        scene("reconcile-edited", "Reconcile — a line's words rewritten before ticking", || {
            use crate::board::tracker::reconcile_dialog::ReconcileGesture as G;
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(Call::OpenReconcile).call(Call::Reconcile { gesture: G::Text { line: 0, text: "The brief route is in; the hook reads it next.".into() } });
            w.s
        }),
        scene("reconcile-none", "Reconcile — nothing under way: the ask", || reconciling(|feature| feature.reconcile = None).s),
        scene("reconcile-asked", "Reconcile — asked, the agent still reading", || {
            reconciling(|feature| {
                if let Some(r) = &mut feature.reconcile {
                    r.proposed = false;
                    r.lines.clear();
                    r.noticed.clear();
                }
            })
            .s
        }),
        scene("reconcile-carrying", "Reconcile — the agent carrying out the ticked lines", || {
            reconciling(|feature| {
                if let Some(r) = &mut feature.reconcile {
                    r.carrying = vec![argus_protocol::Ticked { line: 0, text: r.lines[0].text.clone() }];
                }
            })
            .s
        }),
        scene("reconcile-failed", "Reconcile — a line that failed to carry out", || {
            reconciling(|feature| {
                if let Some(r) = &mut feature.reconcile {
                    r.lines[1].failed = Some("GitHub answered 404: no issue 214".into());
                }
            })
            .s
        }),
        scene("checkouts", "Checkouts — a repository's worktrees and branches", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowCheckouts { open: true }).call(Call::SelectCheckoutRow { key: format!("c{DESKTOP}") });
            w.s
        }),
        scene("checkouts-files", "Checkouts — a worktree's files, opened to a changed one", || {
            use crate::checkouts::files::FilesGesture;
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowCheckouts { open: true })
                .call(Call::SelectCheckoutRow { key: format!("c{DESKTOP}") })
                .call(Call::CheckoutsAction { action: Action::Files });
            for path in ["crates", "crates/argus-desktop", "crates/argus-desktop/assets"] {
                w.call(Call::Files { gesture: FilesGesture::Click { path: path.into() } });
            }
            w.call(Call::Files { gesture: FilesGesture::Click { path: "crates/argus-desktop/assets/style.css".into() } });
            w.s
        }),
        scene("browsers", "Browsers — every agent's browser at once, live", || {
            let mut w = browsing();
            w.call(Call::ShowBrowsers { pane: None });
            w.s
        }),
        scene("browser", "Browser — an agent's browser, a pane beside it in the grid", || {
            let mut w = browsing();
            w.call(Call::Resize { side: Side::Workspace, rows: ROWS, cols: COLS / 2 - 1 })
                .call(Call::Resize { side: Side::Beside(0), rows: ROWS, cols: COLS / 2 - 1 });
            w.s
        }),
        scene("browsers-tabless", "Browsers — an agent's browser whose last tab closed has no picture", || {
            let mut w = browsing_tabless();
            w.call(Call::ShowBrowsers { pane: None });
            w.s
        }),
        scene("browser-tabless", "Browser — a pane with no tab open says so and offers a new one", || {
            let mut w = browsing_tabless();
            w.call(Call::Resize { side: Side::Workspace, rows: ROWS, cols: COLS / 2 - 1 })
                .call(Call::Resize { side: Side::Beside(0), rows: ROWS, cols: COLS / 2 - 1 });
            w.s
        }),
        scene("browser-held", "Browser — the person holds it: the agent's input is refused", || {
            let mut w = browsing_beside(P_FAULTS);
            w.call(Call::Resize { side: Side::Workspace, rows: ROWS, cols: COLS / 2 - 1 })
                .call(Call::Resize { side: Side::Beside(0), rows: ROWS, cols: COLS / 2 - 1 });
            w.s
        }),
        scene("browsers-none", "Browsers — no agent has one open", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowBrowsers { pane: None });
            w.s
        }),
        scene("checkouts-delete", "Checkouts — removing a worktree asks first", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::RemoveCheckout { checkout: FAULTS });
            w.s
        }),
        scene("code", "Code — a file with its tree and outline", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowCode { open: true })
                .call(Call::Code { gesture: CodeGesture::Click { target: CodeClick::Tree { path: "crates/argus-desktop-core/src/chrome/status.rs".into() } } })
                .call(Call::Code { gesture: CodeGesture::Click { target: CodeClick::Line { line: 52, extend: false } } });
            w.s
        }),
        scene("code-edit", "Code — the file open in the editor", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowCode { open: true })
                .call(Call::Code { gesture: CodeGesture::Click { target: CodeClick::Tree { path: "crates/argus-desktop-core/src/chrome/status.rs".into() } } })
                .call(Call::Code { gesture: CodeGesture::Click { target: CodeClick::Line { line: 52, extend: false } } });
            w.s
        }),
        scene("code-start", "Code — nothing open yet: what changed, and the one field", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowCode { open: true });
            w.s
        }),
        scene("code-search","Code — searching the checkout", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::ShowCode { open: true })
                .call(Call::Code { gesture: CodeGesture::Action { action: Action::Search } })
                .call(Call::Code { gesture: CodeGesture::Search { gesture: ReviewJumpGesture::Query { text: "#DraggedWidth".into() } } })
                .call(Call::Code { gesture: CodeGesture::Search { gesture: ReviewJumpGesture::Choose } });
            w.s
        }),
        scene("review", "Review — the working changes of a checkout", || reviewing().s),
        scene("review-comment", "Review — a comment to the agent on picked lines", || {
            let mut w = reviewing();
            w.call(Call::ReviewAction { action: Action::Pick })
                .call(Call::ReviewAction { action: Action::Next })
                .call(Call::ReviewAction { action: Action::Comment });
            w.s
        }),
        scene("history", "History — a commit unfolded to its diff", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenHistory { checkout: MAIN })
                .call(Call::HistoryAction { action: Action::Next, row: None })
                .call(Call::HistoryAction { action: Action::Unfold, row: None });
            w.s
        }),
        scene("sessions", "Sessions — a repository's agent sessions and one's transcript", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenSessions { repository: ARGUS });
            w.s
        }),
        scene("sessions-ended", "Sessions — the Ended group opened out", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenSessions { repository: ARGUS })
                .call(Call::SessionsAction { action: Action::ToggleEnded, row: None })
                .call(Call::SessionsAction { action: Action::Next, row: None });
            w.s
        }),
        scene("transcript", "Transcript — a session read whole, on one of its turns, with a find", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenSessions { repository: ARGUS })
                .call(Call::SessionsAction { action: Action::Transcript, row: None })
                .call(Call::SessionsAction { action: Action::Next, row: None })
                .call(Call::FindInTranscript { find: "brief".into() });
            w.s
        }),
        scene("add-project", "Add a project — browsing for a folder", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenAddProject);
            w.s
        }),
        // The leader then Ctrl-K asks the page to open the palette; what it
        // lists beyond the commands comes once the page says it is open.
        scene("palette", "Palette — searching commands, panes and features", || {
            let mut w = Window::showing(P_TRIM);
            w.call(key(" ", "Space", true)).call(key("k", "KeyK", true));
            w.s
        }),
        scene("palette-features", "Palette — over the Feature stage, whose board has arrived", || {
            let mut w = Window::showing(P_TRIM);
            on_feature(&mut w, "trim");
            w.call(key(" ", "Space", true)).call(key("k", "KeyK", true));
            w.s
        }),
        scene("shortcuts", "Shortcuts — the keyboard sheet", || {
            let mut w = Window::showing(P_TRIM);
            w.call(key(" ", "Space", true)).call(key("?", "Slash", false));
            w.s
        }),
        scene("settings", "Settings — the window's choices", || {
            let mut w = Window::showing(P_TRIM);
            let font = |name: &str, mono| FontFamily { name: name.into(), mono };
            w.s.found_fonts(vec![
                font("Cascadia Code", true),
                font("Cascadia Mono", true),
                font("Consolas", true),
                font("JetBrains Mono", true),
                font("Inter", false),
                font("Segoe UI", false),
                font("Segoe UI Variable", false),
            ]);
            w.call(Call::Command { action: Action::Settings });
            w.s
        }),
        scene("notice", "Notice — a refusal said under the stage", || {
            let mut w = Window::showing(P_TRIM);
            let sent = w.s.call(Call::SpawnAgent { checkout: RATATUI, template: "gemini".into() });
            let request_id = sent
                .iter()
                .find_map(|m| match m {
                    ClientMsg::SpawnAgent { request_id, .. } => Some(*request_id),
                    _ => None,
                })
                .expect("an agent is asked for");
            w.receive(ServerMsg::Refused(RequestRefusal {
                request_id,
                reason: "gemini is not on argusd's PATH; install it or point the template at it".into(),
                read: false,
            }));
            w.s
        }),
        scene("agent-views", "Views — what an agent showed, beside its pane", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenView { pane: P_TRIM, id: "trim-progress".into() });
            w.s
        }),
        scene("agent-view-page", "Views — an agent's view open as the page", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::PopOutView { pane: P_TRIM, id: "trim-progress".into() });
            w.s
        }),
        scene("extension-stage", "Extension — a docker extension's containers as the page", || {
            let panel = || Panel::extension("docker", "containers");
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() })
                .call(Call::ExtensionMode { panel: panel(), page: true })
                .call(Call::Extension { panel: panel(), gesture: ExtensionGesture::Select { key: "api-worker".into() } });
            w.s
        }),
        scene("extension-dialog", "Extension — a question an extension asks", || {
            let mut w = Window::showing(P_TRIM);
            w.receive(ServerMsg::ExtensionDialog {
                extension: "docker".into(),
                id: 9,
                dialog: ExtDialog {
                    title: "Stop api-web".into(),
                    kind: ExtDialogKind::Confirm { message: "Stop api-web? api-worker depends on it and stops with it.".into(), yes: Some("Stop both".into()), danger: true },
                    answers: None,
                },
                prompted: false,
            });
            w.s
        }),
        scene("saved-view", "Saved view — docked beside the pane", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenSavedView { id: "release-0-34".into(), from: None });
            w.s
        }),
        scene("docks", "Docks — the console below, an extension on the right", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::Console { gesture: ConsoleGesture::Toggle })
                .call(Call::Resize { side: Side::Workspace, rows: 26, cols: 96 })
                .call(Call::Resize { side: Side::First, rows: 12, cols: 96 })
                .call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
            w.s
        }),
        scene("float", "Float — a saved view floating over the stage", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenSavedView { id: "architecture".into(), from: None })
                .call(Call::Docks { gesture: DockGesture::Float { panel: Panel::Saved("architecture".into()) } });
            w.s
        }),
        Scene {
            then: vec![Step::Click("#heading .projbtn")],
            ..scene("switcher", "Switcher — projects, workspaces and hosts", || Window::showing(P_TRIM).s)
        },
    ]
}

#[test]
#[ignore = "writes the screenshot harness's scenes; run it by name"]
fn write_screens() {
    let mut names = std::collections::HashSet::new();
    let mut lines = Vec::new();
    for scene in scenes() {
        assert!(names.insert(scene.name), "{} is named twice", scene.name);
        let mut s = (scene.build)();
        let mut events = s.everything(NOW);
        events.extend(s.take_frames());
        assert!(!events.is_empty(), "{} hears nothing", scene.name);
        let mut json = json!({"name": scene.name, "title": scene.title, "events": events});
        if !scene.then.is_empty() {
            json["then"] = serde_json::to_value(&scene.then).unwrap();
        }
        lines.push(serde_json::to_string(&json).unwrap());
    }
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../argus-desktop/page/test/screens");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("scenes.json"), format!("[\n{}\n]\n", lines.join(",\n"))).unwrap();
}

#[test]
fn the_browser_scenes_cover_a_tab_less_browser_and_a_held_one() {
    use crate::browser::status::{status, Status};
    use crate::connection::page::Event;
    let heard = |name: &str| {
        let scene = scenes().into_iter().find(|scene| scene.name == name).expect(name);
        let events = (scene.build)().everything(NOW);
        let page = events.iter().find_map(|e| if let Event::Browsers(page) = e { Some((**page).clone()) } else { None }).expect("the stage is sent");
        let shots: Vec<u64> = events.iter().filter_map(|e| if let Event::BrowserShot(shot) = e { Some(shot.pane) } else { None }).collect();
        (page, shots)
    };
    let (page, shots) = heard("browsers-tabless");
    let tabless = page.browsers.iter().find(|b| b.pane == B_TRIM).unwrap();
    assert!(tabless.shown.is_none() && tabless.tabs.is_empty());
    assert!(!shots.contains(&B_TRIM), "a tab-less browser has no picture");
    assert!(shots.contains(&B_SCREENS) && shots.contains(&B_FAULTS), "the others keep theirs");
    let (page, _) = heard("browser-held");
    let held = page.browsers.iter().find(|b| b.pane == B_FAULTS).unwrap();
    assert_eq!(status(held), Some(Status::Held));
}
