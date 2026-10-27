//! Render tests, driven through a `TestBackend` so a frame can be
//! asserted on as text. One file per thing being drawn, over the
//! fixtures they all build an app from.

mod agent_views;
mod brief;
mod browser;
mod command_center;
mod diagram;
mod diff;
mod frame;
mod geometry;
mod help;
mod keymap;
mod panes;
mod picker;
mod saved_views;
mod status;

use super::*;
use crate::app::layers::dirpicker;
use crate::fixtures::*;
use argus_protocol::{
    CheckoutId, CheckoutInfo, GitStatus, PaneId, PaneKind, ProjectId, ProjectInfo,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

// --- what a pane row says -----------------------------------------------

// --- rendering the whole frame -----------------------------------------

pub(super) fn tree() -> Vec<ProjectInfo> {
    vec![project(
        1,
        "argus",
        vec![repository(
            2,
            "orion",
            vec![
                CheckoutInfo {
                    path: "/repo".to_string(),
                    git: Some(git(Some("master"), true, 2, 0, 0)),
                    ..checkout(
                        10,
                        "master",
                        true,
                        vec![
                            pane_info(100, PaneKind::Agent, "claude", PaneStatus::Working),
                            pane_info(101, PaneKind::Shell, "shell", PaneStatus::Idle),
                        ],
                    )
                },
                CheckoutInfo {
                    path: "/repo/wt".to_string(),
                    ..checkout(11, "feat", false, vec![])
                },
            ],
        )],
    )]
}

// --- a feature with everything in it -------------------------------------

/// The board the Feature stage's tests read: three features, the middle one
/// with a task tree, a decision tree hung off it, and one flow.
pub(super) fn feature_board(project: Option<ProjectId>) -> argus_protocol::DecisionBoard {
    use argus_protocol::{Consequence, Decision, DecisionState, Feature, FeatureState, TaskCounts};
    let feature = |number: i64, slug: &str, title: &str| Feature {
        number,
        slug: slug.into(),
        title: title.into(),
        origin_branch: Some("master".into()),
        state: FeatureState::Open,
        ..Feature::default()
    };
    let decision = |id: i64, parent: Option<i64>, task: Option<i64>, chose: &str| Decision {
        id,
        parent,
        task,
        at: id,
        feature: Some("trim".into()),
        chose: chose.into(),
        ..Decision::default()
    };
    argus_protocol::DecisionBoard {
        repositories: Vec::new(),
        project,
        tracker_map: Default::default(),
        name: "argus".into(),
        features: vec![
            Feature {
                tasks: TaskCounts { todo: 0, doing: 0, done: 4 },
                ..feature(6, "ctrl-c", "Hand a pane's child Ctrl-C handling on Windows")
            },
            Feature {
                body: "The session-start hook prints the whole brief into every pane. Name the feature there instead.".into(),
                checkouts: vec!["/repo".into()],
                tasks: TaskCounts { todo: 4, doing: 2, done: 4 },
                ..feature(7, "trim", "Trim Argus context injection")
            },
            Feature {
                waits_on: vec!["trim".into()],
                tasks: TaskCounts { todo: 5, doing: 0, done: 0 },
                ..feature(8, "ratatui", "The ratatui 0.30 bump and visual pass")
            },
        ],
        decisions: vec![
            decision(1, None, Some(1), "Name the feature, don't print the brief"),
            decision(2, Some(1), None, "The name line carries the feature id"),
            Decision {
                context: Some("Session start now only names the feature.".into()),
                detail: Some("argus-hook asks the daemon for its pane's brief over the pane API.".into()),
                because: Some("A hook that opens the file is a second contract.".into()),
                consequences: vec![
                    Consequence { gain: true, text: "One contract for the brief".into() },
                    Consequence { gain: false, text: "A pane with no feature needs a refusal".into() },
                ],
                ..decision(3, None, Some(2), "The brief travels through the daemon")
            },
            decision(4, Some(3), None, "The route lives in the pane API URL grammar"),
            Decision {
                state: DecisionState::Rejected,
                because: Some("the second contract this decision exists to avoid".into()),
                ..decision(5, Some(3), None, "Hook reads the brief file directly")
            },
            Decision {
                state: DecisionState::Open,
                options: vec!["All of it".into(), "The last ten".into()],
                ..decision(6, None, Some(3), "How much decision history does a pane get?")
            },
        ],
    }
}

pub(super) fn feature_tasks() -> argus_protocol::TaskList {
    use argus_protocol::{Criterion, Task, TaskNote, TaskState};
    let task = |id: i64, parent: Option<i64>, state: TaskState, title: &str| Task {
        id,
        parent,
        state,
        position: id,
        feature: "trim".into(),
        title: title.into(),
        ..Task::default()
    };
    use TaskState::{Doing, Done, Todo};
    argus_protocol::TaskList {
        repositories: Vec::new(),
        project_name: "argus".into(),
        feature: Some("trim".into()),
        tasks: vec![
            task(1, None, Done, "Name the feature at session start"),
            task(5, Some(1), Done, "Print the name line"),
            task(6, Some(1), Done, "Drop the brief from the hook"),
            task(2, None, Doing, "Serve the brief on demand"),
            task(7, Some(2), Done, "Add a brief route to the pane API URL grammar"),
            Task {
                body: Some("The hook asks the daemon for its pane's brief instead of opening the file.".into()),
                claimed_by: Some("session-a".into()),
                claimed_pane: Some(PaneId(100)),
                criteria: vec![
                    Criterion { text: "The hook's request uses the route from 2.1".into(), done: true },
                    Criterion { text: "No path to the brief file is left in argus-hook".into(), done: false },
                ],
                notes: vec![
                    TaskNote { at: 1, session: Some("session-a".into()), pane: Some(PaneId(100)), text: "route answers; wiring the hook to it now".into() },
                    TaskNote { at: 2, session: None, pane: None, text: "keep the refusal case for 2.3".into() },
                ],
                ..task(8, Some(2), Doing, "argus-hook reads the brief through the daemon")
            },
            task(9, Some(2), Todo, "Refuse when the pane has no feature"),
            task(3, None, Todo, "Cap the decision history a pane receives"),
            task(10, Some(3), Todo, "Implement the cap"),
            task(4, None, Todo, "Update the DESIGN.md responsibility table"),
        ],
    }
}

pub(super) const FLOW: &str = "sequenceDiagram
    participant agent
    participant hook
    participant argusd
    agent->>hook: ask for the brief
    hook->>argusd: brief request %% decision #4
    alt the pane has a feature
        argusd-->>hook: the brief
    else the pane has no feature
        argusd-->>hook: refused
        %% task #9
    end";

/// The Feature stage on its index with the board above pushed, the cursor
/// on feature #7, and every message it sends readable.
pub(super) fn feature_stage() -> (
    App,
    tokio::sync::mpsc::UnboundedReceiver<argus_protocol::ClientMsg>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(tx);
    // Never the desk's browser: `t` here only says what it would open.
    app.open_url = |_| crate::system::browser::Opened::Copied;
    app.on_server_msg(argus_protocol::ServerMsg::Tree(tree()));
    let project = app.current_project().unwrap().id;
    app.open_view(View::Feature);
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(feature_board(Some(project)))));
    let at = app
        .feature_rows()
        .iter()
        .position(|row| row.slug == "trim")
        .expect("the fixture's active feature");
    app.feature_sel = at;
    app.rescope_feature();
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(feature_board(Some(project)))));
    app.on_server_msg(argus_protocol::ServerMsg::Tasks(Box::new(feature_tasks())));
    app.on_server_msg(argus_protocol::ServerMsg::SequenceDiagrams(Box::new(
        argus_protocol::DiagramList {
            repositories: Vec::new(),
            project_name: "argus".into(),
            feature: Some("trim".into()),
            diagrams: vec![argus_protocol::SequenceDiagram {
                id: 1,
                feature: "trim".into(),
                title: "Brief on demand".into(),
                body: FLOW.into(),
                at: 0,
                session: None,
            }],
        },
    )));
    (app, rx)
}

/// Pushes the board on screen again, changed, as an agent's write would.
pub(super) fn push_board(app: &mut App, change: impl FnOnce(&mut argus_protocol::DecisionBoard)) {
    let mut board = app.board_follower.board().expect("a board on screen").clone();
    change(&mut board);
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(board)));
}

/// Feature #7 open on its work lens.
pub(super) fn app_reading_a_feature() -> App {
    let (mut app, rx) = feature_stage();
    std::mem::forget(rx);
    app.feature_open = true;
    app
}

/// Renders a real frame through ratatui's test backend and hands back
/// the buffer, so the UI can be asserted on without a terminal.
pub(super) fn draw(app: &mut App) -> ratatui::buffer::Buffer {
    draw_at(app, 120, 20)
}

pub(super) fn draw_at(app: &mut App, w: u16, h: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| render(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

pub(super) fn lines(buf: &ratatui::buffer::Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// The status bar's row: the one carrying the nav keymap.
pub(super) fn bar_row(buf: &ratatui::buffer::Buffer) -> u16 {
    lines(buf)
        .iter()
        .rposition(|r| !r.trim().is_empty())
        .expect("the status bar") as u16
}

pub(super) fn bar(buf: &ratatui::buffer::Buffer) -> String {
    lines(buf)[bar_row(buf) as usize].clone()
}

pub(super) fn app_with_tree() -> App {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    // Keep the receiver alive so sends don't fail during render setup.
    std::mem::forget(rx);
    let mut app = App::new(tx);
    app.on_server_msg(argus_protocol::ServerMsg::Tree(tree()));
    app
}

pub(super) fn cell(ch: char) -> argus_protocol::Cell {
    argus_protocol::Cell {
        ch: ch.to_string().into(),
        ..Default::default()
    }
}

// --- a table taller than its stage ---------------------------------------

/// The Checkouts stage over eight checkouts in one repository, so a short
/// terminal has to scroll the table.
pub(super) fn app_with_a_long_checkout_table() -> App {
    let mut app = app_with_tree();
    let r = &mut app.tree[0].repositories[0];
    r.checkouts = (0..8)
        .map(|i| argus_protocol::CheckoutInfo {
            id: CheckoutId(20 + i),
            name: format!("wt-{i}"),
            path: format!("/repo/wt-{i}"),
            primary: i == 0,
            git: None,
            panes: Vec::new(),
        })
        .collect();
    app.view = View::Checkouts;
    app.focus = Focus::View;
    app
}

/// Clicks the checkouts table's `drawn_row`th row.
pub(super) fn click_checkout(app: &mut App, drawn_row: u16) {
    let inner = app.layout.checkouts.inner;
    app.on_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: inner.x + 1,
        row: inner.y + drawn_row * 3,
        modifiers: KeyModifiers::NONE,
    });
}

pub(super) fn comment_prompt(input: &str) -> App {
    let mut app = app_with_tree();
    app.prompt = Some(Prompt::Comment {
        anchor: argus_protocol::ReviewAnchor {
            base: argus_protocol::ReviewBase::Unstaged,
            commit: None,
            path: "crates/argus-client/src/ui.rs".to_string(),
            old_path: None,
            old_start: Some(1013),
            old_end: Some(1013),
            new_start: Some(1013),
            new_end: Some(1013),
            text: vec!["        Prompt::Comment { anchor, input } => (".to_string()],
        },
        input: input.to_string(),
    });
    app
}

/// What is inside the prompt box, borders and the columns it floats
/// over excluded. The box is found rather than assumed: it is centered
/// and sized to its own content.
pub(super) fn box_rows(buf: &ratatui::buffer::Buffer) -> Vec<String> {
    let sym = |x: u16, y: u16| {
        buf.cell((x, y))
            .map(|c| c.symbol().to_string())
            .unwrap_or_default()
    };
    // The panels are drawn first and start higher up, so the last
    // top-left corner on the screen belongs to the modal over them.
    let (x0, y0) = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .rfind(|(x, y)| sym(*x, *y) == "\u{256d}")
        .expect("a prompt box should be drawn");
    let x1 = (x0 + 1..buf.area.width)
        .find(|x| sym(*x, y0) == "\u{256e}")
        .expect("closed on the right");

    (y0 + 1..buf.area.height)
        .take_while(|y| sym(x0, *y) != "\u{2570}")
        .map(|y| {
            (x0 + 1..x1)
                .map(|x| sym(x, y))
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

// --- a pane parked in its scrollback -------------------------------------

/// An app inside a pane whose view is parked `offset` lines back, with
/// `mark` filling the rows the daemon answered with.
pub(super) fn app_scrolled_back(offset: u32, depth: u32, mark: char) -> App {
    let mut app = app_with_tree();
    app.focus = Focus::PaneContent;
    let pane = app.column_pane().expect("the fixture pane");
    let live = |c: char| vec![vec![cell(c); 30]; 5];
    let mut view = crate::fixtures::pane_view(pane, live('L'));
    let _ = view.scroll(-(offset as i32));
    let _ = view.receive(&argus_protocol::ServerMsg::ScrollbackRows { pane, offset, depth, cells: live(mark) });
    app.grids.insert(pane, view);
    app
}

// --- briefs --------------------------------------------------------------

pub(super) fn app_with_a_brief(body: &str) -> App {
    let mut app = app_with_tree();
    app.brief = Some(crate::app::layers::brief::BriefView::new(
        crate::app::layers::brief::BriefTarget::Feature {
            project: ProjectId(1),
            slug: "pty".to_string(),
        },
        "The pty".to_string(),
        body,
    ));
    app.overlay = Some(Overlay::Brief);
    app
}

// --- the directory browser ----------------------------------------------

pub(super) fn app_browsing() -> App {
    let mut app = app_with_tree();
    let mut picker = dirpicker::DirPicker::new(dirpicker::DirTarget::Project, 1);
    picker.show(argus_protocol::DirListing {
        request_id: 1,
        path: "/home/u/Source/github.com".to_string(),
        parent: Some("/home/u/Source".to_string()),
        entries: [("argus", true), ("notes", false), ("orion", true)]
            .iter()
            .map(|(name, is_repo)| argus_protocol::DirEntry {
                name: name.to_string(),
                is_repo: *is_repo,
            })
            .collect(),
        error: None,
    });
    app.dir_picker = Some(picker);
    app
}

// --- review viewer ------------------------------------------------------

pub(super) fn app_with_review() -> App {
    app_with_review_split(false)
}

pub(super) fn app_with_review_split(split: bool) -> App {
    app_with_diff(
        split,
        vec![
            diff_line(
                argus_protocol::LineKind::Context,
                Some(10),
                Some(10),
                "unchanged",
            ),
            diff_line(argus_protocol::LineKind::Removed, Some(11), None, "gone"),
            diff_line(argus_protocol::LineKind::Added, None, Some(11), "arrived"),
        ],
    )
}

pub(super) fn diff_line(
    kind: argus_protocol::LineKind,
    old_lineno: Option<u32>,
    new_lineno: Option<u32>,
    text: &str,
) -> argus_protocol::DiffLine {
    argus_protocol::DiffLine {
        kind,
        old_lineno,
        new_lineno,
        text: text.to_string(),
        spans: Vec::new(),
    }
}

pub(super) fn app_with_diff(split: bool, lines: Vec<argus_protocol::DiffLine>) -> App {
    showing(split, None, lines)
}

/// A commit's review, opened from the history overlay still behind it.
pub(super) fn app_with_commit_review() -> App {
    let commit = argus_protocol::CommitInfo {
        oid: "a".repeat(40),
        short: "aaaaaaa".to_string(),
        summary: "Wake a pane's pump on the byte".to_string(),
        author: "hunt".to_string(),
        time: 0,
    };
    let lines = vec![diff_line(argus_protocol::LineKind::Added, None, Some(11), "arrived")];
    let mut app = showing(false, Some(commit), lines);
    app.history = app_with_history().history;
    app
}

/// `lines` as the review on screen, answered the way the daemon would.
fn showing(
    split: bool,
    commit: Option<argus_protocol::CommitInfo>,
    lines: Vec<argus_protocol::DiffLine>,
) -> App {
    showing_file(split, commit, lines, None)
}

/// A review whose file is forty lines long, read from a blob, so the lines
/// above and below its one hunk are folds — the cursor on the one below.
pub(super) fn app_with_folded_review() -> App {
    let lines = vec![
        diff_line(argus_protocol::LineKind::Context, Some(10), Some(10), "unchanged"),
        diff_line(argus_protocol::LineKind::Removed, Some(11), None, "gone"),
        diff_line(argus_protocol::LineKind::Added, None, Some(11), "arrived"),
        diff_line(argus_protocol::LineKind::Context, Some(12), Some(12), "after"),
    ];
    let mut app = showing_file(false, None, lines, Some(40));
    let view = app.review.as_mut().unwrap().view_mut().unwrap();
    view.bottom_of_diff();
    view.move_by(1);
    app
}

/// As [`showing`], the file `new_lines` long when that is given, with a
/// blob to read the lines between hunks from.
fn showing_file(
    split: bool,
    commit: Option<argus_protocol::CommitInfo>,
    lines: Vec<argus_protocol::DiffLine>,
    new_lines: Option<u32>,
) -> App {
    use argus_protocol::review::session::ReviewSession;
    let mut app = app_with_tree();
    app.review_split = split;
    let (mut session, base) = match &commit {
        Some(c) => (
            ReviewSession::of_commit(CheckoutId(1), c.oid.clone(), None, split, 1).0,
            argus_protocol::ReviewBase::Commit,
        ),
        None => (
            ReviewSession::working(CheckoutId(1), argus_protocol::ReviewBase::Unstaged, split, false, || 1).0,
            argus_protocol::ReviewBase::Unstaged,
        ),
    };
    session.receive(argus_protocol::Review {
        request_id: 1,
        checkout: CheckoutId(1),
        base,
        files: vec![argus_protocol::FileDiff {
            path: "src/thing.rs".to_string(),
            old_path: None,
            kind: argus_protocol::ChangeKind::Modified,
            hunks: vec![argus_protocol::Hunk {
                header: "@@ -10,3 +10,3 @@ fn f()".to_string(),
                lines,
            }],
            note: None,
            new_blob: new_lines.map(|_| "b10b".to_string()),
            new_lines,
            pending: false,
            counted: None,
        }],
        commit,
    });
    app.review = Some(session);
    app.overlay = Some(Overlay::Review);
    app.focus = Focus::Review;
    app
}

/// As the overlay first opens: headers, and no commit summarized yet.
pub(super) fn app_with_history() -> App {
    let mut app = app_with_tree();
    app.history = Some(argus_protocol::review::history_view::HistoryView::new(
        CheckoutId(1),
        vec![argus_protocol::CommitInfo {
            oid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            short: "aaaaaaa".into(),
            summary: "Wake a pane's pump on the byte".into(),
            author: "hunt".into(),
            time: 0,
        }],
    ));
    app.overlay = Some(Overlay::History);
    app.focus = Focus::Review;
    app
}

/// The same overlay after the cursor has drilled into that commit and
/// the daemon has answered with what it touched.
pub(super) fn app_with_drilled_history() -> App {
    let mut app = app_with_history();
    let view = app.history.as_mut().unwrap();
    view.drill();
    view.receive_files(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        vec![
            argus_protocol::CommitFile {
                path: "crates/argusd/src/pty.rs".into(),
                old_path: None,
                kind: argus_protocol::ChangeKind::Modified,
                added: 12,
                removed: 3,
            },
            argus_protocol::CommitFile {
                path: "DESIGN.md".into(),
                old_path: None,
                kind: argus_protocol::ChangeKind::Added,
                added: 40,
                removed: 0,
            },
        ],
    );
    app
}

pub(super) fn fg_of(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<Color> {
    cell_at(buf, needle).map(|c| c.fg)
}

pub(super) fn bg_of(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<Color> {
    cell_at(buf, needle).map(|c| c.bg)
}

pub(super) fn cell_at<'a>(
    buf: &'a ratatui::buffer::Buffer,
    needle: &str,
) -> Option<&'a ratatui::buffer::Cell> {
    // Cell-wise: multi-byte glyphs make byte offsets lie.
    let needle: Vec<&str> = needle.split("").filter(|s| !s.is_empty()).collect();
    for y in 0..buf.area.height {
        let row: Vec<&str> = (0..buf.area.width)
            .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();
        if let Some(x) = row.windows(needle.len()).position(|w| w == needle) {
            return buf.cell((x as u16, y));
        }
    }
    None
}

pub(super) fn row_text(buf: &ratatui::buffer::Buffer, y: u16, area: Rect) -> String {
    (area.x..area.x + area.width)
        .filter_map(|x| buf.cell((x, y)).map(|c| c.symbol()))
        .collect()
}

pub(super) fn row_of(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<u16> {
    let needle: Vec<&str> = needle.split("").filter(|s| !s.is_empty()).collect();
    (0..buf.area.height).find(|&y| {
        let row: Vec<&str> = (0..buf.area.width)
            .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();
        row.windows(needle.len()).any(|w| w == needle)
    })
}

// --- the fuzzy picker ---------------------------------------------------

pub(super) fn app_with_branch_picker(query: &str) -> App {
    let mut app = app_with_tree();
    let mut p = crate::app::Picker::new(
        PickerKind::Branch {
            checkout: CheckoutId(10),
        },
        "switch branch",
        ["feature/login", "feature/logout", "hotfix"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        0,
    );
    p.type_query(query);
    app.picker = Some(p);
    app
}

pub(super) fn git(
    branch: Option<&str>,
    dirty: bool,
    changed: usize,
    ahead: usize,
    behind: usize,
) -> GitStatus {
    GitStatus {
        branch: branch.map(str::to_string),
        dirty,
        changed_files: changed,
        staged: 0,
        unstaged: changed,
        ahead,
        behind,
        upstream: None,
        last_commit: None,
        changes: 0,
    }
}

