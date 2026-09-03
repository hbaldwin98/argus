//! Render tests, driven through a `TestBackend` so a frame can be
//! asserted on as text. One file per thing being drawn, over the
//! fixtures they all build an app from.

mod browser;
mod diff;
mod frame;
mod geometry;
mod help;
mod narrow;
mod notes;
mod panes;
mod picker;
mod rows;
mod views;

use super::*;
use argus_protocol::{
    CheckoutId, CheckoutInfo, PaneId, PaneInfo, PaneKind, ProjectId, ProjectInfo, RepositoryId,
    RepositoryInfo,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;



// --- what a pane row says -----------------------------------------------

pub(super) fn pane(status: PaneStatus, note: Option<&str>) -> argus_protocol::PaneInfo {
    argus_protocol::PaneInfo {
        id: PaneId(1),
        kind: PaneKind::Agent,
        title: "claude".to_string(),
        status,
        note: note.map(str::to_string),
        template: None,
        children: Vec::new(),
    }
}

// --- rendering the whole frame -----------------------------------------

pub(super) fn tree() -> Vec<ProjectInfo> {
    vec![ProjectInfo {
        id: ProjectId(1),
        name: "argus".to_string(),
        repositories: vec![RepositoryInfo {
            id: RepositoryId(2),
            name: "orion".to_string(),
            branches: Vec::new(),
            default_branch: None,
            remote_branches: Vec::new(),
            checkouts: vec![
                CheckoutInfo {
                    id: CheckoutId(10),
                    name: "master".to_string(),
                    path: "/repo".to_string(),
                    primary: true,
                    git: Some(git(Some("master"), true, 2, 0, 0)),
                    panes: vec![
                        PaneInfo {
                            id: PaneId(100),
                            kind: PaneKind::Agent,
                            title: "claude".to_string(),
                            status: PaneStatus::Working,
                            note: None,
                            template: None,
                            children: Vec::new(),
                        },
                        PaneInfo {
                            id: PaneId(101),
                            kind: PaneKind::Shell,
                            title: "shell".to_string(),
                            status: PaneStatus::Idle,
                            note: None,
                            template: None,
                            children: Vec::new(),
                        },
                    ],
                    notes: Default::default(),
                    has_note: false,
                },
                CheckoutInfo {
                    id: CheckoutId(11),
                    name: "feat".to_string(),
                    path: "/repo/wt".to_string(),
                    primary: false,
                    git: None,
                    panes: vec![],
                    notes: Default::default(),
                    has_note: false,
                },
            ],
        }],
        notes: Default::default(),
        has_note: false,
    }]
}

/// Renders a real frame through ratatui's test backend and hands back
/// the buffer, so the UI can be asserted on without a terminal.
pub(super) fn draw(app: &mut App) -> ratatui::buffer::Buffer {
    // Wide enough for the whole spine: five cards at their floors plus a
    // live view at its own is exactly what `spine_min_width` says, and a
    // narrower default would fold a column away under every test that was
    // not about folding.
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

// --- a column taller than its card ---------------------------------------

/// Eight checkouts in one repository, so a short column has to scroll.
pub(super) fn app_with_a_long_checkout_column() -> App {
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
            notes: Default::default(),
            has_note: false,
        })
        .collect();
    app.focus = Focus::Checkouts;
    app
}

/// The row of the checkouts column that a given screen row sits on.
pub(super) fn click_checkout(app: &mut App, drawn_row: u16) {
    let inner = app.layout.checkouts.inner;
    app.on_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: inner.x + 1,
        row: inner.y + drawn_row * app.layout.row_height,
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
    let mut grid = crate::grid::Grid::new(live('L'));
    grid.scrollback = Some(crate::grid::Scrollback {
        offset,
        depth,
        cells: live(mark),
    });
    app.grids.insert(pane, grid);
    app
}

// --- notes ---------------------------------------------------------------

pub(super) fn app_with_a_note(body: &str) -> App {
    let mut app = app_with_tree();
    let target = argus_protocol::NoteTarget::Checkout(CheckoutId(10));
    let note = argus_protocol::Note::new(target, body.to_string());
    app.notes = Some(crate::notes::NoteView::new(&note, "master".to_string()));
    app.overlay = Some(Overlay::Notes);
    app
}

// --- the directory browser ----------------------------------------------

pub(super) fn app_browsing() -> App {
    let mut app = app_with_tree();
    let mut picker = crate::dirpicker::DirPicker::new(crate::dirpicker::DirTarget::Project, 1);
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
    let mut app = app_with_tree();
    app.review_split = split;
    app.review = Some(crate::review::ReviewView::new(
        argus_protocol::Review {
            request_id: 1,
            checkout: CheckoutId(1),
            base: argus_protocol::ReviewBase::Unstaged,
            files: vec![argus_protocol::FileDiff {
                path: "src/thing.rs".to_string(),
                old_path: None,
                kind: argus_protocol::ChangeKind::Modified,
                hunks: vec![argus_protocol::Hunk {
                    header: "@@ -10,3 +10,3 @@ fn f()".to_string(),
                    lines,
                }],
                note: None,
            }],
            commit: None,
        },
        split,
    ));
    app.overlay = Some(Overlay::Review);
    app.focus = Focus::Review;
    app
}

/// As the overlay first opens: headers, and no commit summarized yet.
pub(super) fn app_with_history() -> App {
    let mut app = app_with_tree();
    app.history = Some(crate::history::HistoryView::new(
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
        ahead,
        behind,
    }
}

pub(super) fn checkout_with(statuses: &[PaneStatus]) -> CheckoutInfo {
    CheckoutInfo {
        id: CheckoutId(1),
        name: "c".to_string(),
        path: "/c".to_string(),
        primary: true,
        git: None,
        panes: statuses
            .iter()
            .enumerate()
            .map(|(i, s)| PaneInfo {
                id: PaneId(i as u64),
                kind: PaneKind::Agent,
                title: "t".to_string(),
                status: *s,
                note: None,
                template: None,
                children: Vec::new(),
            })
            .collect(),
        notes: Default::default(),
        has_note: false,
    }
}

pub(super) fn text_of(spans: &[Span]) -> String {
    spans.iter().map(|s| s.content.as_ref()).collect()
}
