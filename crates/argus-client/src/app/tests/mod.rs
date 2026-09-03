//! The client model's tests, one file per thing the operator does, over
//! the fixtures they all build an app from.

mod branch_rows;
mod columns;
mod editors;
mod mouse_input;
mod navigation;
mod note_editing;
mod picking;
mod preferences;
mod prompts;
mod reviewing;
mod scrollback;
mod spawning;
mod tree_updates;
mod typing;
mod windows;
mod workspaces;

use super::*;
use argus_protocol::{
    Cell, CellSpan, CheckoutId, GitStatus, NoteCounts, PaneKind, PaneStatus, ProjectId,
    RepositoryId, RepositoryInfo, TodoState,
};
use argus_protocol::{DirEntry, DirListing};
use crossterm::event::KeyModifiers;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

fn pane(id: u64, title: &str) -> PaneInfo {
    PaneInfo {
        id: PaneId(id),
        kind: PaneKind::Shell,
        title: title.to_string(),
        status: PaneStatus::Idle,
        note: None,
        template: None,
        children: Vec::new(),
    }
}

/// Two projects; the first has a primary checkout with two panes and a
/// linked worktree with none, the second has a single empty checkout.
fn tree() -> Vec<ProjectInfo> {
    vec![
        ProjectInfo {
            id: ProjectId(1),
            name: "argus".to_string(),
            repositories: vec![repository(
                5,
                "orion",
                vec![
                    checkout(
                        10,
                        "master",
                        true,
                        vec![pane(100, "shell"), pane(101, "claude")],
                    ),
                    checkout(11, "feat", false, vec![]),
                ],
            )],
            notes: Default::default(),
            has_note: false,
        },
        ProjectInfo {
            id: ProjectId(2),
            name: "other".to_string(),
            repositories: vec![repository(
                6,
                "other-repo",
                vec![checkout(20, "main", true, vec![])],
            )],
            notes: Default::default(),
            has_note: false,
        },
    ]
}

pub(super) struct Harness {
    app: App,
    rx: UnboundedReceiver<ClientMsg>,
}

impl Harness {
    /// An app that has already received the fixture tree, with the
    /// resulting Subscribe traffic drained so tests assert only on what
    /// they themselves trigger.
    pub(super) fn new() -> Self {
        let (tx, rx) = unbounded_channel();
        let mut app = App::new(tx);
        app.on_server_msg(ServerMsg::Tree(tree()));
        app.templates = vec!["claude".to_string(), "codex".to_string()];
        let mut h = Harness { app, rx };
        h.sent();
        h
    }

    pub(super) fn key(&mut self, code: KeyCode) {
        self.app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    pub(super) fn leader(&mut self) {
        self.key(KeyCode::Null);
    }

    pub(super) fn keys(&mut self, s: &str) {
        for c in s.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    /// Answers the browser's outstanding listing request, the way the
    /// daemon would.
    pub(super) fn browse(&mut self, path: &str, parent: Option<&str>, entries: &[(&str, bool)]) {
        let request_id = self
            .app
            .dir_picker
            .as_ref()
            .and_then(|p| p.pending)
            .expect("the browser is waiting for a listing");
        self.app.on_server_msg(ServerMsg::Directories(DirListing {
            request_id,
            path: path.to_string(),
            parent: parent.map(str::to_string),
            entries: entries
                .iter()
                .map(|(name, is_repo)| DirEntry {
                    name: name.to_string(),
                    is_repo: *is_repo,
                })
                .collect(),
            error: None,
        }));
    }

    pub(super) fn sent(&mut self) -> Vec<ClientMsg> {
        let mut out = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            out.push(msg);
        }
        out
    }
}

pub(super) fn drag(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

pub(super) fn drawn_mark(h: &Harness, pane: PaneId) -> String {
    h.app.grids[&pane].view()[0][0].ch.to_string()
}

pub(super) fn only_default() -> Vec<argus_protocol::WorkspaceInfo> {
    vec![argus_protocol::WorkspaceInfo {
        id: argus_protocol::WorkspaceId(1),
        name: "default".to_string(),
        projects: 1,
        panes: 0,
        open: true,
    }]
}

pub(super) fn review_with_agent() -> Harness {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Tree(tree_with_agent()));
    h.sent();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));
    h
}

pub(super) fn touched(path: &str) -> argus_protocol::CommitFile {
    argus_protocol::CommitFile {
        path: path.to_string(),
        old_path: None,
        kind: argus_protocol::ChangeKind::Modified,
        added: 1,
        removed: 1,
    }
}

pub(super) fn commit_files_arrive(h: &mut Harness, checkout: CheckoutId, oid: &str) {
    h.app.on_server_msg(ServerMsg::CommitFiles {
        checkout,
        commit: oid.to_string(),
        files: vec![touched("src/a.rs")],
    });
}

pub(super) fn open_editor_from_review(h: &mut Harness) {
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(h, diff_of(checkout));
    h.key(KeyCode::Char('e'));
    h.sent();
    // The daemon answers with a tree carrying the new editor pane.
    let mut t = tree();
    t[0].repositories[0].checkouts[0].panes.push(PaneInfo {
        id: PaneId(700),
        kind: PaneKind::Editor,
        title: "a.rs".to_string(),
        status: PaneStatus::Idle,
        note: None,
        template: None,
        children: Vec::new(),
    });
    h.app.on_server_msg(ServerMsg::Tree(t));
}

pub(super) fn checkout(id: u64, name: &str, primary: bool, panes: Vec<PaneInfo>) -> CheckoutInfo {
    CheckoutInfo {
        id: CheckoutId(id),
        name: name.to_string(),
        path: format!("/repo/{name}"),
        primary,
        git: None,
        panes,
        notes: Default::default(),
        has_note: false,
    }
}

pub(super) fn repository(id: u64, name: &str, checkouts: Vec<CheckoutInfo>) -> RepositoryInfo {
    RepositoryInfo {
        id: RepositoryId(id),
        name: name.to_string(),
        checkouts,
        branches: Vec::new(),
        default_branch: None,
        remote_branches: Vec::new(),
    }
}

// --- branches without a checkout ----------------------------------------

/// The fixture tree with two branches nothing is sitting on, the
/// column expanded to show them, and the selection parked on the first.
pub(super) fn harness_on_a_branch_row() -> Harness {
    let mut h = Harness::new();
    h.app.tree[0].repositories[0].branches =
        vec!["hotfix/tls".to_string(), "spike".to_string()];
    h.keys("ll"); // into the checkouts column
    h.key(KeyCode::Char('B')); // and show the branches at all
    h.keys("jj"); // past both checkouts, onto the first branch
    h.sent();
    h
}

/// The fixture tree with one branch that exists only on the remote,
/// the column expanded, and the selection parked on it.
pub(super) fn harness_on_a_remote_branch_row() -> Harness {
    let mut h = Harness::new();
    h.app.tree[0].repositories[0].remote_branches = vec!["origin/from-elsewhere".to_string()];
    h.keys("ll");
    h.key(KeyCode::Char('B'));
    h.keys("jj"); // past both checkouts
    h.sent();
    h
}

// --- where an editor opens ----------------------------------------------

/// Opens an editor without disturbing where the columns are pointed —
/// `open_review` drives keys from the projects column, which would
/// move the selection this is trying to observe.
pub(super) fn editor_arrives(h: &mut Harness) {
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    h.app.review_for_test(checkout);
    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
    h.key(KeyCode::Char('e'));
    h.sent();
    let mut t = tree();
    t[0].repositories[0].checkouts[0].panes.push(PaneInfo {
        id: PaneId(700),
        kind: PaneKind::Editor,
        title: "a.rs".to_string(),
        status: PaneStatus::Idle,
        note: None,
        template: None,
        children: Vec::new(),
    });
    h.app.on_server_msg(ServerMsg::Tree(t));
}

// --- choosing the editor ------------------------------------------------

/// Moves the settings cursor onto `want`.
pub(super) fn settings_row(h: &mut Harness, want: crate::app::Setting) {
    h.app.open_settings();
    let target = crate::app::Setting::ALL
        .iter()
        .position(|s| *s == want)
        .unwrap();
    for _ in 0..target {
        h.key(KeyCode::Char('j'));
    }
}

// --- editors are not panes ----------------------------------------------

/// A tree whose first checkout has a shell, an agent, and an editor.
pub(super) fn tree_with_editor() -> Vec<ProjectInfo> {
    let mut t = tree();
    t[0].repositories[0].checkouts[0].panes.push(PaneInfo {
        id: PaneId(700),
        kind: PaneKind::Editor,
        title: "a.rs".to_string(),
        status: PaneStatus::Idle,
        note: None,
        template: None,
        children: Vec::new(),
    });
    t
}

// --- mouse --------------------------------------------------------------

pub(super) fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

/// Five cards side by side, each with a one-cell frame around its rows,
/// so tests can click both a row and the chrome around it.
pub(super) fn laid_out(h: &mut Harness) {
    let panel = |x: u16, w: u16| Panel {
        outer: Rect::new(x, 0, w, 8),
        inner: Rect::new(x + 1, 1, w - 2, 6),
        first: 0,
    };
    h.app.layout = Layout {
        width: 100,
        row_height: crate::ui::ROW_HEIGHT,
        projects: panel(0, 12),
        repositories: panel(12, 12),
        checkouts: panel(24, 12),
        panes: panel(36, 12),
        content: panel(48, 20),
        overlay: Panel::default(),
        cursor: None,
    };
}

/// Say that `pane`'s child has asked for SGR mouse reporting. Nothing is
/// forwarded to a child that hasn't, so any test about forwarding has to
/// establish this first.
pub(super) fn wants_mouse(h: &mut Harness, pane: PaneId) {
    let grid = h
        .app
        .grids
        .entry(pane)
        .or_insert_with(|| crate::grid::Grid::new(Vec::new()));
    grid.mouse = argus_protocol::MouseTracking {
        mode: argus_protocol::MouseMode::ButtonMotion,
        encoding: argus_protocol::MouseEncoding::Sgr,
    };
}

/// Say that `pane`'s child is drawing on the alternate screen without
/// mouse reporting — Claude, Codex, and Cursor Agent's usual mode.
pub(super) fn on_alt_screen(h: &mut Harness, pane: PaneId) {
    h.app
        .grids
        .entry(pane)
        .or_insert_with(|| crate::grid::Grid::new(Vec::new()))
        .alternate_screen = true;
}

// --- notes ---------------------------------------------------------------

/// A harness with the note window open on the first checkout, holding
/// `body`, with the opening traffic drained.
pub(super) fn harness_with_a_note(body: &str) -> Harness {
    let mut h = Harness::new();
    h.keys("ll"); // into the checkouts column
    h.key(KeyCode::Char('m'));
    let target = h.app.notes.as_ref().expect("the note window is open").target;
    h.app
        .on_server_msg(ServerMsg::Note(Box::new(argus_protocol::Note::new(
            target,
            body.to_string(),
        ))));
    h.sent();
    h
}

// --- fuzzy pickers ------------------------------------------------------

pub(super) fn branches_arrive(h: &mut Harness, list: &[&str]) {
    h.key(KeyCode::Char('l')); // into the checkouts column
    h.key(KeyCode::Char('b'));
    let checkout = match h.sent().into_iter().next() {
        Some(ClientMsg::ListBranches { checkout }) => checkout,
        other => panic!("unexpected {other:?}"),
    };
    h.app.on_server_msg(ServerMsg::Branches {
        checkout,
        branches: list.iter().map(|s| s.to_string()).collect(),
    });
}

// --- files --------------------------------------------------------------

pub(super) fn files_arrive(h: &mut Harness, list: &[&str]) {
    h.key(KeyCode::Char('l'));
    h.key(KeyCode::Char('f'));
    let checkout = match h.sent().into_iter().next() {
        Some(ClientMsg::ListFiles { checkout }) => checkout,
        other => panic!("unexpected {other:?}"),
    };
    h.app.on_server_msg(ServerMsg::Files {
        checkout,
        files: list.iter().map(|s| s.to_string()).collect(),
    });
}

// --- review -------------------------------------------------------------

pub(super) fn diff_of(checkout: CheckoutId) -> argus_protocol::Review {
    argus_protocol::Review {
        request_id: 1,
        checkout,
        base: argus_protocol::ReviewBase::Unstaged,
        files: vec![argus_protocol::FileDiff {
            path: "src/a.rs".to_string(),
            old_path: None,
            kind: argus_protocol::ChangeKind::Modified,
            hunks: vec![argus_protocol::Hunk {
                header: "@@ -1,2 +1,2 @@".to_string(),
                lines: vec![
                    argus_protocol::DiffLine {
                        kind: argus_protocol::LineKind::Context,
                        old_lineno: Some(1),
                        new_lineno: Some(1),
                        text: "keep".to_string(),
                        spans: Vec::new(),
                    },
                    argus_protocol::DiffLine {
                        kind: argus_protocol::LineKind::Added,
                        old_lineno: None,
                        new_lineno: Some(2),
                        text: "new".to_string(),
                        spans: Vec::new(),
                    },
                ],
            }],
            note: None,
        }],
        commit: None,
    }
}

/// Presses `R` on the first checkout and answers with `review`.
pub(super) fn open_review(h: &mut Harness, review: argus_protocol::Review) {
    h.key(KeyCode::Char('l'));
    h.key(KeyCode::Char('R'));
    h.sent();
    h.app.on_server_msg(ServerMsg::Review(review));
}

/// A tree whose first checkout has an agent pane running in it.
pub(super) fn tree_with_agent() -> Vec<ProjectInfo> {
    let mut t = tree();
    t[0].repositories[0].checkouts[0].panes = vec![
        PaneInfo {
            id: PaneId(50),
            kind: PaneKind::Shell,
            title: "sh".to_string(),
            status: PaneStatus::Idle,
            note: None,
            template: None,
            children: Vec::new(),
        },
        PaneInfo {
            id: PaneId(51),
            kind: PaneKind::Agent,
            title: "claude".to_string(),
            status: PaneStatus::Idle,
            note: None,
            template: None,
            children: Vec::new(),
        },
    ];
    t
}

// --- history -------------------------------------------------------------

pub(super) fn commit(oid: &str, summary: &str) -> argus_protocol::CommitInfo {
    argus_protocol::CommitInfo {
        oid: oid.to_string(),
        short: oid.chars().take(7).collect(),
        summary: summary.to_string(),
        author: "hunt".to_string(),
        time: 0,
    }
}

/// Presses `H` on the first checkout and answers with two commits.
pub(super) fn open_history(h: &mut Harness) -> CheckoutId {
    h.key(KeyCode::Char('l'));
    let checkout = h.app.current_checkout().unwrap().id;
    h.key(KeyCode::Char('H'));
    let request_id = match &h.sent()[0] {
        ClientMsg::ListCommits { request_id, .. } => *request_id,
        other => panic!("unexpected {other:?}"),
    };
    h.app.on_server_msg(ServerMsg::Commits {
        request_id,
        checkout,
        commits: vec![commit("aaaa111", "newest"), commit("bbbb222", "older")],
    });
    checkout
}

// --- Pane scrollback ---------------------------------------------------

pub(super) fn wheel(kind: MouseEventKind) -> MouseEvent {
    MouseEvent {
        kind,
        column: 54,
        row: 3,
        modifiers: KeyModifiers::NONE,
    }
}

/// A pane showing three live rows, entered, subscribed and drained.
pub(super) fn live_pane(h: &mut Harness) -> PaneId {
    laid_out(h);
    h.keys("llll");
    assert_eq!(h.app.focus, Focus::PaneContent);
    let pane = h.app.column_pane().unwrap();
    h.app.grids.insert(
        pane,
        crate::grid::Grid::new(vec![vec![Cell::default(); 4]; 3]),
    );
    h.sent();
    pane
}

/// Answer an outstanding Scrollback request the way the daemon would,
/// with rows whose first cell carries `mark` so a test can tell which
/// depth it is looking at.
pub(super) fn answer_scrollback(h: &mut Harness, pane: PaneId, offset: u32, depth: u32, mark: char) {
    let mut row = vec![Cell::default(); 4];
    row[0].ch = mark.to_string().into();
    h.app.on_server_msg(ServerMsg::ScrollbackRows {
        pane,
        offset,
        depth,
        cells: vec![row; 3],
    });
}

/// The offsets asked for since the last drain. `ClientMsg` carries no
/// `PartialEq`, so the traffic is compared by what it requested.
pub(super) fn scrollback_asks(h: &mut Harness) -> Vec<u32> {
    h.sent()
        .into_iter()
        .filter_map(|m| match m {
            ClientMsg::Scrollback { offset, .. } => Some(offset),
            _ => None,
        })
        .collect()
}

// --- workspaces ---------------------------------------------------------

pub(super) fn workspaces(open: &str) -> Vec<argus_protocol::WorkspaceInfo> {
    ["default", "work", "weekend"]
        .iter()
        .enumerate()
        .map(|(i, name)| argus_protocol::WorkspaceInfo {
            id: argus_protocol::WorkspaceId(i as u64 + 1),
            name: name.to_string(),
            projects: i + 1,
            panes: i,
            open: *name == open,
        })
        .collect()
}
