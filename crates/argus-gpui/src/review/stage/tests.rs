//! The Git stage in the whole window: the sidebar's Git, the stage it lights, the
//! Changes tab it holds, and what each press sends. Rules cited are
//! `gpui/parity/review-agents.md`'s.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::chrome::shell::Shell;
use argus_desktop_core::connection::page::{
    Checkout, Connection, DiffRow, DiffSide, Event, HistoryPage, HistoryRowView, Pane, Project, Rail, Repository, ReviewFile, ReviewJump, ReviewPage,
    Shown, Stage, Status,
};
use argus_desktop_core::review::comment_box::{CommentPage, Recipient};
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{point, px, size, AnyWindowHandle, Bounds, ElementId, Modifiers, MouseButton, Pixels, Point, TestAppContext, WindowBounds, WindowOptions};

type Calls = Rc<RefCell<Vec<Call>>>;

struct Opened {
    handle: AnyWindowHandle,
    heard: Entity<Heard>,
    calls: Calls,
}

fn pane(id: u64) -> Pane {
    Pane { id, agent: false, title: "zsh".into(), status: "idle", note: None, on: false, views: 0 }
}

fn checkout(id: u64, name: &str, panes: Vec<Pane>) -> Checkout {
    Checkout {
        id,
        name: name.into(),
        branch: name.into(),
        kind: "main",
        git: None,
        clean: true,
        shared: None,
        open: true,
        count: panes.len(),
        status: None,
        panes,
    }
}

/// One repository with two checkouts: `main` (100) holding pane 1, and `side` (110) with none.
fn rail(shown: Option<u64>) -> Rail {
    Rail {
        project: Some(Project {
            id: 1,
            name: "orion".into(),
            needs_you: 0,
            repositories: vec![Repository {
                id: 10,
                name: "argus".into(),
                quiet: false,
                divider: false,
                open: true,
                panes: 1,
                status: None,
                used: None,
                checkouts: vec![checkout(100, "main", vec![pane(1)]), checkout(110, "side", Vec::new())],
            }],
            running: Vec::new(),
        }),
        shown: shown.map(|pane| Shown {
            pane,
            repository: "argus".into(),
            checkout: "main".into(),
            branch: "main".into(),
            agent: false,
            browser: false,
            title: "zsh".into(),
            status: "idle",
            resumed: None,
        }),
        loaded: true,
        stage: Stage::Workspace,
        ..Rail::default()
    }
}

fn status(connection: Connection) -> Event {
    Event::Status(Status { connection, detail: None, alarm: None, chime: true, host: None })
}

fn side(old: Option<u32>, new: Option<u32>, kind: &'static str, marker: &'static str, text: &str) -> DiffSide {
    DiffSide { old, new, marker, kind, parts: vec![(text.to_owned(), "")] }
}

fn row(row: usize, kind: &'static str) -> DiffRow {
    DiffRow { row, kind, text: String::new(), hidden: None, step: None, left: None, right: None, cursor: false, picked: false }
}

fn hunk(at: usize) -> DiffRow {
    DiffRow { text: "@@ -1,3 +1,3 @@ fn main".into(), ..row(at, "hunk") }
}

fn line(at: usize, kind: &'static str, text: &str) -> DiffRow {
    let (old, new, marker) = match kind {
        "added" => (None, Some(at as u32), "+"),
        "removed" => (Some(at as u32), None, "\u{2212}"),
        _ => (Some(at as u32), Some(at as u32), ""),
    };
    DiffRow { left: Some(side(old, new, kind, marker, text)), ..row(at, "line") }
}

fn fold(at: usize, step: Option<u32>) -> DiffRow {
    DiffRow { text: "30 lines not in the diff".into(), hidden: Some(30), step, ..row(at, "fold") }
}

fn file(path: &str, added: usize, removed: usize) -> ReviewFile {
    let (dir, name) = path.rsplit_once('/').map_or(("", path), |(dir, name)| (dir, name));
    ReviewFile {
        path: path.into(),
        dir: if dir.is_empty() { String::new() } else { format!("{dir}/") },
        name: name.into(),
        added,
        removed,
        kind: "modified",
    }
}

/// An open review of `main` with two files and a short diff of the first.
fn review() -> ReviewPage {
    let mut rows = vec![
        hunk(0),
        line(1, "context", "fn main() {"),
        line(2, "removed", "    old();"),
        line(3, "added", "    new();"),
        fold(4, Some(20)),
        line(5, "context", "}"),
    ];
    rows[2].cursor = true;
    ReviewPage {
        open: true,
        checkout: 100,
        name: "main".into(),
        branch: "main".into(),
        base: "unstaged",
        unstaged: Some(2),
        staged: Some(1),
        added: 3,
        removed: 1,
        files: vec![file("src/lib.rs", 2, 1), file("README.md", 1, 0)],
        file: Some(0),
        rows,
        ..ReviewPage::default()
    }
}

fn reviewing(page: ReviewPage) -> Event {
    Event::Review(Box::new(page))
}

fn closed() -> Event {
    Event::Review(Box::new(ReviewPage { open: false, ..ReviewPage::default() }))
}

fn open(cx: &mut TestAppContext, events: Vec<Event>) -> Opened {
    cx.update(crate::window::init);
    let calls: Calls = Rc::default();
    let sink = calls.clone();
    let caller = Caller::new(move |call| sink.borrow_mut().push(call));
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(1100.0), px(720.0))))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let heard = cx.new(|_| Heard::new());
                let shell = cx.new(|cx| Shell::new(heard.clone(), caller, window, cx));
                built = Some(heard);
                shell
            },
        )
        .expect("open test window")
    });
    let opened = Opened { handle, heard: built.unwrap(), calls };
    // A test window starts inactive, and an inactive one is told of no focus change.
    cx.update_window(handle, |_, window, _| window.activate_window()).unwrap();
    cx.run_until_parked();
    hear(cx, &opened, events);
    // What the window said of its own focus on opening is not what a test is about.
    opened.calls.borrow_mut().clear();
    opened
}

fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    cx.update(|cx| {
        opened.heard.update(cx, |heard, cx| {
            for event in events {
                heard.hear(event);
            }
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// A connected window on pane 1 of `main`, with nothing of Git open.
fn on_main(cx: &mut TestAppContext) -> Opened {
    open(cx, vec![status(Connection::Connected), Event::Rail(Box::new(rail(Some(1))))])
}

/// The same with the review open, as the session says.
fn reviewed(cx: &mut TestAppContext) -> Opened {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(review())]);
    opened
}

fn taken(opened: &Opened) -> Vec<String> {
    opened.calls.borrow().iter().map(|call| format!("{call:?}")).collect()
}

fn forget(opened: &Opened) {
    opened.calls.borrow_mut().clear();
}

fn present(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.try_find(id).is_some()).unwrap()
}

fn label(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> Option<String> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.try_find(id).and_then(|f| f.label().map(str::to_owned))).unwrap()
}

fn selected(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> Option<bool> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.find(id).selected()).unwrap()
}

fn click(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, cx| window.click(id, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn shift_click(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, cx| window.click_with_modifiers(id, Modifiers::shift(), cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn press(cx: &mut TestAppContext, opened: &Opened, key: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.press(key, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn type_text(cx: &mut TestAppContext, opened: &Opened, text: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.input(text, cx)).unwrap();
    cx.run_until_parked();
}

/// How many items the review's virtual list has built, which is what a long diff must keep small:
/// the elements of a window are only what the frame drew, and say nothing of what was built to get there.
fn rows_built() -> usize {
    crate::kit::diff::built_by("review-diff")
}

fn keys_typed(opened: &Opened) -> usize {
    opened.calls.borrow().iter().filter(|call| matches!(call, Call::Key { .. } | Call::Text { .. })).count()
}

// ---- Entering Git ----------------------------------------------------------------

#[gpui_kit::test]
fn git_asks_for_the_review_of_the_pane_on_screens_checkout_and_draws_none_of_it_until_the_event(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    assert!(!present(cx, &opened, "git"), "no stage before Git is pressed");
    click(cx, &opened, "nav-git");
    assert_eq!(taken(&opened), ["OpenReview { checkout: 100 }"]);
    assert!(!present(cx, &opened, "git"), "nothing of the review is drawn before the Review event");
    assert_eq!(selected(cx, &opened, "nav-git"), Some(false), "Git is lit when the session says the review is open");
    hear(cx, &opened, vec![reviewing(review())]);
    assert!(present(cx, &opened, "git") && present(cx, &opened, "review"));
    assert_eq!(selected(cx, &opened, "nav-git"), Some(true));
    assert_eq!(selected(cx, &opened, "nav-workspace"), Some(false), "the stage the session is on is no longer the lit one");
}

#[gpui_kit::test]
fn git_with_no_pane_shown_asks_for_the_checkout_picked_in_the_rail(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connected), Event::Rail(Box::new(rail(None)))]);
    click(cx, &opened, "checkout-110");
    click(cx, &opened, "nav-git");
    assert_eq!(taken(&opened), ["OpenReview { checkout: 110 }"]);
}

#[gpui_kit::test]
fn git_with_no_checkout_says_to_pick_one_and_asks_for_nothing(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connected), Event::Rail(Box::new(rail(None)))]);
    click(cx, &opened, "nav-git");
    assert!(taken(&opened).is_empty(), "nothing to open it on: {:?}", taken(&opened));
    assert_eq!(label(cx, &opened, "git-pick").as_deref(), Some("Pick a checkout in the sidebar, then press Git."));
    assert_eq!(selected(cx, &opened, "nav-git"), Some(true), "the stage is up on the window's word");
    // A checkout picked and Git pressed again: now it opens.
    click(cx, &opened, "checkout-110");
    click(cx, &opened, "nav-git");
    assert_eq!(taken(&opened), ["OpenReview { checkout: 110 }"]);
}

#[gpui_kit::test]
fn a_held_window_does_not_ask_and_puts_no_stage_up(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    click(cx, &opened, "nav-git");
    assert!(taken(&opened).is_empty());
    assert!(!present(cx, &opened, "git") && !present(cx, &opened, "git-pick"));
}

// ---- What a Review snapshot draws -------------------------------------------------

#[gpui_kit::test]
fn a_review_snapshot_draws_its_files_hunks_and_every_field_of_the_page(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    // The header: the branch, the tabs, the count, and the two buttons.
    assert_eq!(label(cx, &opened, "git-branch").as_deref(), Some("main"));
    assert_eq!(selected(cx, &opened, "git-tab-changes"), Some(true));
    assert_eq!(selected(cx, &opened, "git-tab-history"), Some(false));
    assert_eq!(label(cx, &opened, "review-count").as_deref(), Some("2 files \u{b7} +3 \u{2212}1"));
    assert!(present(cx, &opened, "review-refresh") && present(cx, &opened, "git-close"));
    // The files: each its path, the cursor's file lit.
    assert_eq!(label(cx, &opened, ("review-file", 0usize)).as_deref(), Some("src/lib.rs"));
    assert_eq!(label(cx, &opened, ("review-file", 1usize)).as_deref(), Some("README.md"));
    assert_eq!(selected(cx, &opened, ("review-file", 0usize)), Some(true));
    assert_eq!(selected(cx, &opened, ("review-file", 1usize)), Some(false));
    // The side and the layout.
    assert_eq!(selected(cx, &opened, "review-side-unstaged"), Some(true));
    assert_eq!(selected(cx, &opened, "review-side-staged"), Some(false));
    assert_eq!(selected(cx, &opened, "review-view-unified"), Some(true));
    // The rows: the hunk, the lines, and the fold with its two buttons.
    for at in 0..6usize {
        assert!(present(cx, &opened, ("diff-row", at)), "row {at}");
    }
    assert_eq!(label(cx, &opened, ("diff-fold", 4usize)).as_deref(), Some("Show 20"));
    assert_eq!(label(cx, &opened, ("diff-fold-all", 4usize)).as_deref(), Some("Show all"));
    assert!(label(cx, &opened, "review-hints").unwrap().contains("Ctrl P jump"));
    assert!(!present(cx, &opened, "review-pick") && !present(cx, &opened, "review-loading"));
}

#[gpui_kit::test]
fn the_pick_a_commit_and_the_loading_and_empty_states_are_drawn_in_the_pages_order(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    let mut page = review();
    page.pick = Some("old lines 2\u{2013}3".into());
    hear(cx, &opened, vec![reviewing(page.clone())]);
    assert_eq!(label(cx, &opened, "review-pick").as_deref(), Some("old lines 2\u{2013}3 picked"));
    // Loading wins over a reason, which wins over rows.
    page.loading = true;
    page.empty = Some("Nothing unstaged in main.".into());
    hear(cx, &opened, vec![reviewing(page.clone())]);
    assert!(present(cx, &opened, "review-loading") && !present(cx, &opened, "review-empty"));
    page.loading = false;
    hear(cx, &opened, vec![reviewing(page.clone())]);
    assert_eq!(label(cx, &opened, "review-empty").as_deref(), Some("Nothing unstaged in main."));
    assert!(!present(cx, &opened, ("diff-row", 0usize)));
    // A commit's review says what it is of in place of the side radio.
    page.empty = None;
    page.base = "commit";
    page.commit = Some("a1b2c3d  Trim context".into());
    hear(cx, &opened, vec![reviewing(page)]);
    assert!(!present(cx, &opened, "review-side"));
    assert_eq!(label(cx, &opened, "review-commit"), None);
    assert!(present(cx, &opened, "review-commit"));
}

#[gpui_kit::test]
fn a_split_review_draws_its_pairs_and_marks_the_layout(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    let mut page = review();
    page.split = true;
    page.rows = vec![
        hunk(0),
        DiffRow {
            left: Some(side(Some(2), None, "removed", "\u{2212}", "    old();")),
            right: Some(side(None, Some(2), "added", "+", "    new();")),
            cursor: true,
            ..row(1, "pair")
        },
    ];
    hear(cx, &opened, vec![reviewing(page)]);
    assert_eq!(selected(cx, &opened, "review-view-split"), Some(true));
    assert!(present(cx, &opened, ("diff-row", 1usize)));
}

#[gpui_kit::test]
fn a_large_diff_builds_a_bounded_number_of_rows(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    let mut page = review();
    page.rows = (0..20_000usize).map(|at| if at % 50 == 0 { hunk(at) } else { line(at, "context", "let x = 1;") }).collect();
    page.rows[10].cursor = true;
    hear(cx, &opened, vec![reviewing(page)]);
    let drawn = rows_built();
    assert!(drawn > 5, "the rows on screen were not drawn: {drawn}");
    assert!(drawn < 80, "a 20000-row diff built {drawn} rows");
    assert!(present(cx, &opened, ("diff-row", 10usize)));
    assert!(!present(cx, &opened, ("diff-row", 19_000usize)), "a row far below the window was built");
}

// ---- Presses send the page's calls -----------------------------------------------

#[gpui_kit::test]
fn each_button_and_click_sends_the_call_the_page_sends_and_changes_nothing_itself(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    let before = (
        selected(cx, &opened, "review-side-unstaged"),
        selected(cx, &opened, "review-view-unified"),
        selected(cx, &opened, ("review-file", 0usize)),
    );
    let asked = |opened: &Opened| {
        let said = taken(opened);
        forget(opened);
        said
    };
    click(cx, &opened, "review-side-staged");
    assert_eq!(asked(&opened), ["ReviewClick { target: Side { staged: true } }"]);
    click(cx, &opened, "review-view-split");
    assert_eq!(asked(&opened), ["ReviewAction { action: Split }"]);
    click(cx, &opened, "review-view-unified");
    assert!(asked(&opened).is_empty(), "the layout already on sends nothing (REVIEW-3)");
    click(cx, &opened, ("review-file", 1usize));
    assert_eq!(asked(&opened), ["ReviewClick { target: File { file: 1 } }"]);
    click(cx, &opened, "review-refresh");
    assert_eq!(asked(&opened), ["ReviewRefresh"]);
    click(cx, &opened, ("diff-fold", 4usize));
    assert_eq!(asked(&opened), ["ReviewClick { target: Fold { row: 4, all: false } }"], "a fold's button is not also a click on its row");
    click(cx, &opened, ("diff-fold-all", 4usize));
    assert_eq!(asked(&opened), ["ReviewClick { target: Fold { row: 4, all: true } }"]);
    // Nothing the session owns changed on screen before an event said so.
    assert_eq!(
        before,
        (
            selected(cx, &opened, "review-side-unstaged"),
            selected(cx, &opened, "review-view-unified"),
            selected(cx, &opened, ("review-file", 0usize)),
        )
    );
}

#[gpui_kit::test]
fn picking_a_range_sends_the_clicks_the_page_sends_a_plain_one_then_one_with_shift(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    click(cx, &opened, ("diff-row", 2usize));
    shift_click(cx, &opened, ("diff-row", 3usize));
    assert_eq!(
        taken(&opened),
        ["ReviewClick { target: Row { row: 2, extend: false } }", "ReviewClick { target: Row { row: 3, extend: true } }"]
    );
    // `v` marks a range from the cursor too, and the hunk is not a line to pick.
    forget(&opened);
    press(cx, &opened, "v");
    assert_eq!(taken(&opened), ["ReviewAction { action: Pick }"]);
}

/// A drag from the start of the code of row `from` to the end of the code of row `to`.
fn drag_rows(cx: &mut TestAppContext, opened: &Opened, from: usize, to: usize) {
    cx.update_window(opened.handle, |_, window, cx| {
        let (a, b) = (window.find(("diff-row", from)).bounds(), window.find(("diff-row", to)).bounds());
        // Past the two number columns and the marker, one pixel into the code.
        let start = point(a.origin.x + px(106.0), a.center().y);
        window.drag(start, point(b.right() - px(4.0), b.center().y), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

#[gpui_kit::test]
fn dragging_across_rows_sends_the_click_on_the_first_and_the_shift_click_on_the_last_and_draws_no_pick(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    drag_rows(cx, &opened, 1, 3);
    assert_eq!(
        taken(&opened),
        ["ReviewClick { target: Row { row: 1, extend: false } }", "ReviewClick { target: Row { row: 3, extend: true } }"]
    );
    assert!(!present(cx, &opened, "review-pick"), "nothing is picked until the Review event says so");
    // The text it passed over is selected, and Ctrl+C copies it.
    press(cx, &opened, "ctrl-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()).as_deref(),
        Some("fn main() {
    old();
    new();")
    );
    assert_eq!(taken(&opened).len(), 2, "the copy sent nothing");
}

#[gpui_kit::test]
fn dragging_within_one_row_picks_nothing(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    drag_rows(cx, &opened, 2, 2);
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_hunk_header_and_a_note_take_no_click(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    click(cx, &opened, ("diff-row", 0usize));
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn the_reviews_keys_are_heard_by_the_stage_and_never_typed_into_the_pane(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    for (key, call) in [
        ("j", "ReviewAction { action: Next }"),
        ("down", "ReviewAction { action: Next }"),
        ("k", "ReviewAction { action: Previous }"),
        ("]", "ReviewAction { action: NextFile }"),
        ("c", "ReviewAction { action: Comment }"),
        ("b", "ReviewAction { action: Side }"),
        ("s", "ReviewAction { action: Split }"),
        ("enter", "ReviewAction { action: Unfold }"),
        ("z", "ReviewAction { action: UnfoldAll }"),
        ("ctrl-p", "ReviewAction { action: Filter }"),
    ] {
        forget(&opened);
        press(cx, &opened, key);
        assert_eq!(taken(&opened), [call], "{key}");
    }
    forget(&opened);
    press(cx, &opened, "x");
    press(cx, &opened, "1");
    assert!(taken(&opened).is_empty(), "a key the review has no action for is held, not sent: {:?}", taken(&opened));
    assert_eq!(keys_typed(&opened), 0, "the pane under the stage heard a key");
}

// ---- The comment box --------------------------------------------------------------

fn commenting() -> ReviewPage {
    let mut page = review();
    page.rows[2].picked = true;
    page.rows[3].picked = true;
    page.pick = Some("old line 2".into());
    page.comment = Some(CommentPage {
        about: "new lines 2\u{2013}3 \u{b7} unstaged".into(),
        recipients: vec![Recipient { pane: 7, title: "claude \u{b7} trim context".into() }],
        to: Some(7),
    });
    page
}

#[gpui_kit::test]
fn an_open_comment_box_sits_under_the_last_picked_row_and_takes_the_keys(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    hear(cx, &opened, vec![reviewing(commenting())]);
    assert!(present(cx, &opened, "review-comment"));
    assert_eq!(label(cx, &opened, "comment-recipient").as_deref(), Some("claude \u{b7} trim context"), "one agent, lit, is said and not offered");
    let (row, slot, next) = cx
        .update_window(opened.handle, |_, window, _| {
            (
                window.find(("diff-row", 3usize)).bounds(),
                window.find(("diff-slot", 0usize)).bounds(),
                window.find(("diff-row", 4usize)).bounds(),
            )
        })
        .unwrap();
    assert!(slot.top() >= row.bottom() && slot.bottom() <= next.top(), "the box is between the last picked row and the next");
    // The box has the keys: j is typed into it, not heard as the review's next line.
    forget(&opened);
    type_text(cx, &opened, "j");
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
    cx.update(|cx| {
        let stage = opened.heard.read(cx).review.is_some();
        assert!(stage);
    });
}

#[gpui_kit::test]
fn ctrl_enter_sends_the_words_without_a_newline_and_a_plain_enter_is_a_newline(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    hear(cx, &opened, vec![reviewing(commenting())]);
    forget(&opened);
    type_text(cx, &opened, "tidy this");
    press(cx, &opened, "enter");
    type_text(cx, &opened, "up");
    assert!(taken(&opened).is_empty(), "a plain Enter sends nothing");
    press(cx, &opened, "secondary-enter");
    assert_eq!(taken(&opened), ["ReviewComment { body: Some(\"tidy this\\nup\") }"]);
    // No newline was left behind in the box: more typed after it joins the same line.
    forget(&opened);
    type_text(cx, &opened, "!");
    press(cx, &opened, "secondary-enter");
    assert_eq!(taken(&opened), ["ReviewComment { body: Some(\"tidy this\\nup!\") }"]);
}

#[gpui_kit::test]
fn both_ctrl_enter_and_meta_enter_send_once_each(cx: &mut TestAppContext) {
    // REVIEW-AGENTS-COMMENT-6: Ctrl Enter or Meta Enter sends, on every platform.
    let opened = reviewed(cx);
    hear(cx, &opened, vec![reviewing(commenting())]);
    forget(&opened);
    type_text(cx, &opened, "hi");
    press(cx, &opened, "ctrl-enter");
    assert_eq!(taken(&opened), ["ReviewComment { body: Some(\"hi\") }"]);
    forget(&opened);
    press(cx, &opened, "cmd-enter");
    assert_eq!(taken(&opened), ["ReviewComment { body: Some(\"hi\") }"]);
}

#[gpui_kit::test]
fn the_comment_buttons_send_and_cancel_and_a_chip_chooses_the_agent(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    let mut page = commenting();
    let comment = page.comment.as_mut().unwrap();
    comment.recipients[0].title = "claude".into();
    comment.recipients.push(Recipient { pane: 8, title: "codex".into() });
    hear(cx, &opened, vec![reviewing(page)]);
    assert!(!present(cx, &opened, "comment-recipient"), "with several, each is a button");
    click(cx, &opened, "comment-recipient-8");
    click(cx, &opened, "comment-cancel");
    click(cx, &opened, "comment-send");
    assert_eq!(
        taken(&opened),
        [
            "ReviewClick { target: Recipient { pane: 8 } }",
            "ReviewComment { body: None }",
            "ReviewComment { body: Some(\"\") }"
        ]
    );
}

#[gpui_kit::test]
fn escape_in_the_comment_cancels_it_alone_and_the_keys_come_back_once_the_box_is_gone(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    hear(cx, &opened, vec![reviewing(commenting())]);
    forget(&opened);
    type_text(cx, &opened, "half a thought");
    press(cx, &opened, "escape");
    assert_eq!(taken(&opened), ["ReviewComment { body: None }"], "Escape reaches the box, not the review under it");
    // The session says the box is gone: its words go with it and the stage has the keys again.
    forget(&opened);
    hear(cx, &opened, vec![reviewing(review())]);
    assert!(!present(cx, &opened, "review-comment"));
    press(cx, &opened, "j");
    assert_eq!(taken(&opened), ["ReviewAction { action: Next }"], "the review's keys are heard again");
    // The next box starts empty.
    hear(cx, &opened, vec![reviewing(commenting())]);
    forget(&opened);
    press(cx, &opened, "secondary-enter");
    assert_eq!(taken(&opened), ["ReviewComment { body: Some(\"\") }"], "the words went when the box shut");
}

#[gpui_kit::test]
fn a_refused_send_keeps_the_box_and_its_words(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    hear(cx, &opened, vec![reviewing(commenting())]);
    type_text(cx, &opened, "keep me");
    press(cx, &opened, "secondary-enter");
    // The session answers with the same box (a refusal is a notice, not a closed box).
    hear(cx, &opened, vec![Event::Notice { text: "the comment is empty".into() }, reviewing(commenting())]);
    forget(&opened);
    press(cx, &opened, "secondary-enter");
    assert_eq!(taken(&opened), ["ReviewComment { body: Some(\"keep me\") }"], "the words stay across an update");
}

// ---- The jump field ----------------------------------------------------------------

fn jumping(query: &str, matches: Vec<usize>, at: Option<usize>) -> ReviewPage {
    ReviewPage { jump: Some(ReviewJump { query: query.into(), matches, at }), ..review() }
}

#[gpui_kit::test]
fn the_jump_field_lists_its_matches_and_its_keys_are_its_own(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    press(cx, &opened, "ctrl-p");
    assert_eq!(taken(&opened), ["ReviewAction { action: Filter }"]);
    hear(cx, &opened, vec![reviewing(jumping("", vec![0, 1], Some(0)))]);
    assert!(present(cx, &opened, "review-jump"));
    assert_eq!(label(cx, &opened, "review-jump").as_deref(), Some("Jump to a file"), "REVIEW-13");
    forget(&opened);
    for letter in ["r", "e", "a"] {
        type_text(cx, &opened, letter);
    }
    assert_eq!(
        taken(&opened),
        [
            "ReviewJump { gesture: Query { text: \"r\" } }",
            "ReviewJump { gesture: Query { text: \"re\" } }",
            "ReviewJump { gesture: Query { text: \"rea\" } }"
        ],
        "each letter is sent, with no debounce; nothing is heard as the review's key"
    );
    hear(cx, &opened, vec![reviewing(jumping("rea", vec![1], Some(1)))]);
    assert_eq!(selected(cx, &opened, ("review-file", 1usize)), Some(true), "the match Enter opens is lit");
    assert!(!present(cx, &opened, ("review-file", 0usize)), "only matches are listed");
    forget(&opened);
    press(cx, &opened, "down");
    press(cx, &opened, "up");
    press(cx, &opened, "ctrl-n");
    press(cx, &opened, "enter");
    press(cx, &opened, "escape");
    assert_eq!(
        taken(&opened),
        [
            "ReviewJump { gesture: Step { by: 1 } }",
            "ReviewJump { gesture: Step { by: -1 } }",
            "ReviewJump { gesture: Step { by: 1 } }",
            "ReviewJump { gesture: Choose }",
            "ReviewJump { gesture: Close }"
        ],
        "Escape closes the field and not the review"
    );
}

#[gpui_kit::test]
fn a_jump_with_no_match_says_so_and_the_text_is_not_overwritten_by_an_echo(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    hear(cx, &opened, vec![reviewing(jumping("", vec![0, 1], Some(0)))]);
    type_text(cx, &opened, "z");
    type_text(cx, &opened, "z");
    forget(&opened);
    // The session's echo of the first letter lands after the second was typed.
    hear(cx, &opened, vec![reviewing(jumping("z", Vec::new(), None))]);
    assert_eq!(label(cx, &opened, "review-jump-none"), None);
    assert!(present(cx, &opened, "review-jump-none"));
    type_text(cx, &opened, "z");
    assert_eq!(taken(&opened), ["ReviewJump { gesture: Query { text: \"zzz\" } }"], "the field kept what was typed");
}

// ---- Leaving Git -------------------------------------------------------------------

#[gpui_kit::test]
fn leaving_for_another_stage_closes_the_review_with_the_stage_call_and_the_stage_goes_on_the_event(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    click(cx, &opened, "nav-workspace");
    assert_eq!(taken(&opened), ["ReviewAction { action: CloseLayer }", "ShowFeatures { open: false }"]);
    assert!(present(cx, &opened, "git"), "the stage is the session's to say: it stays until the review's event");
    hear(cx, &opened, vec![closed()]);
    assert!(!present(cx, &opened, "git"));
    assert_eq!(selected(cx, &opened, "nav-workspace"), Some(true));
}

#[gpui_kit::test]
fn every_stage_item_closes_what_is_open_and_search_does_not(cx: &mut TestAppContext) {
    for (nav, call) in [
        ("nav-new", "ShowComposer { open: true }"),
        ("nav-feature", "ShowFeatures { open: true }"),
        ("nav-checkouts", "ShowCheckouts { open: true }"),
        ("nav-code", "ShowCode { open: true }"),
    ] {
        let opened = reviewed(cx);
        click(cx, &opened, nav);
        assert_eq!(taken(&opened), ["ReviewAction { action: CloseLayer }", call], "{nav}");
    }
    let opened = reviewed(cx);
    click(cx, &opened, "nav-search");
    assert_eq!(taken(&opened), ["Palette { open: true }"], "the palette is over the stage, not a way out of it");
}

#[gpui_kit::test]
fn a_pane_row_clicked_while_git_is_up_closes_git_first(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    click(cx, &opened, "pane-1");
    assert_eq!(taken(&opened), ["ReviewAction { action: CloseLayer }", "Select { pane: 1 }"]);
}

#[gpui_kit::test]
fn escape_and_the_close_button_close_the_review_and_the_session_says_when_it_is_gone(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    press(cx, &opened, "escape");
    assert_eq!(taken(&opened), ["ReviewAction { action: CloseLayer }"]);
    assert!(present(cx, &opened, "git"));
    forget(&opened);
    click(cx, &opened, "git-close");
    assert_eq!(taken(&opened), ["ReviewAction { action: CloseLayer }"]);
    hear(cx, &opened, vec![closed()]);
    assert!(!present(cx, &opened, "git"));
}

#[gpui_kit::test]
fn closing_the_stage_gives_the_keys_back_to_the_pane_that_was_shown(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    press(cx, &opened, "x");
    assert_eq!(keys_typed(&opened), 0);
    hear(cx, &opened, vec![closed()]);
    press(cx, &opened, "a");
    assert_eq!(keys_typed(&opened), 1, "the pane has the keys again once the stage is gone");
}

// ---- Tabs ----------------------------------------------------------------------------

#[gpui_kit::test]
fn the_history_tab_closes_the_review_and_opens_the_history_of_the_same_checkout(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    click(cx, &opened, "git-tab-history");
    assert_eq!(taken(&opened), ["ReviewAction { action: CloseLayer }", "OpenHistory { checkout: 100 }"]);
    assert_eq!(selected(cx, &opened, "git-tab-changes"), Some(true), "the tab is the session's to say");
    forget(&opened);
    let history = HistoryPage { open: true, name: "main".into(), branch: "main".into(), ..Default::default() };
    hear(cx, &opened, vec![closed(), Event::History(Box::new(history))]);
    assert_eq!(selected(cx, &opened, "git-tab-history"), Some(true));
    assert!(present(cx, &opened, "history"));
    assert!(!present(cx, &opened, "review"));
    click(cx, &opened, "git-tab-changes");
    assert_eq!(taken(&opened), ["HistoryAction { action: CloseLayer, row: None }", "OpenReview { checkout: 100 }"]);
}

#[gpui_kit::test]
fn a_history_the_session_closes_after_a_tab_switch_takes_the_stage_down_with_it(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    click(cx, &opened, "git-tab-history");
    // The session, not Escape or leave(), is what ends it: the history event is what clears the wait.
    hear(cx, &opened, vec![closed(), historic(history()), history_closed()]);
    assert!(!present(cx, &opened, "git"));
}

#[gpui_kit::test]
fn a_review_the_session_closes_after_a_tab_switch_takes_the_stage_down_with_it(cx: &mut TestAppContext) {
    let opened = historied(cx);
    click(cx, &opened, "git-tab-changes");
    hear(cx, &opened, vec![history_closed(), reviewing(review()), closed()]);
    assert!(!present(cx, &opened, "git"));
}

#[gpui_kit::test]
fn a_review_that_opens_after_git_pressed_with_no_checkout_does_not_leave_the_stage_up_once_it_closes(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connected), Event::Rail(Box::new(rail(None)))]);
    click(cx, &opened, "nav-git");
    assert!(present(cx, &opened, "git-pick"));
    hear(cx, &opened, vec![reviewing(review()), closed()]);
    assert!(!present(cx, &opened, "git"));
}

// ---- Held ------------------------------------------------------------------------------

#[gpui_kit::test]
fn a_stage_whose_daemon_is_gone_keeps_what_it_drew_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    hear(cx, &opened, vec![status(Connection::Connecting)]);
    assert!(present(cx, &opened, ("diff-row", 1usize)), "the last review stays on screen");
    forget(&opened);
    click(cx, &opened, ("diff-row", 2usize));
    click(cx, &opened, "review-refresh");
    click(cx, &opened, "git-close");
    press(cx, &opened, "j");
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}


// ---- Scroll ---------------------------------------------------------------------------

fn long(cursor: usize) -> ReviewPage {
    let mut page = review();
    page.rows = (0..400usize).map(|at| line(at, "context", "let x = 1;")).collect();
    page.rows[cursor].cursor = true;
    page
}

fn top_of(cx: &mut TestAppContext, opened: &Opened, at: usize) -> Option<f32> {
    cx.update_window(opened.handle, |_, window, _| window.try_find(("diff-row", at)).map(|row| f32::from(row.bounds().top()))).unwrap()
}

#[gpui_kit::test]
fn the_cursors_row_is_brought_into_view_when_the_rows_change_and_left_alone_while_visible(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    hear(cx, &opened, vec![reviewing(long(300))]);
    // One more frame: the scroll to an item lands when the list next lays out.
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let list = cx.update_window(opened.handle, |_, window, _| window.find("review-rows").bounds()).unwrap();
    let row = top_of(cx, &opened, 300).expect("the cursor's row is built, so it is in view");
    assert!(row >= f32::from(list.top()) && row < f32::from(list.bottom()), "row at {row} outside {list:?}");
    // The cursor moves to the row above, already on screen: nothing scrolls (nearest, SHARED-5).
    let before = top_of(cx, &opened, 299).expect("the row above is on screen too");
    hear(cx, &opened, vec![reviewing(long(299))]);
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert_eq!(top_of(cx, &opened, 299), Some(before), "a cursor already in view moved the list");
}

#[gpui_kit::test]
fn an_update_keeps_where_the_diff_was_scrolled(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    hear(cx, &opened, vec![reviewing(long(0))]);
    cx.update_window(opened.handle, |_, window, cx| {
        window.scroll("review-rows", gpui_kit::ScrollDelta::Pixels(point(px(0.0), px(-660.0))), cx)
    })
    .unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let scrolled = top_of(cx, &opened, 40).expect("row 40 is on screen after scrolling down 30 rows");
    // The same review again, as the session says it whenever anything about it changes.
    let mut again = long(0);
    again.pick = Some("new line 1".into());
    hear(cx, &opened, vec![reviewing(again)]);
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert_eq!(top_of(cx, &opened, 40), Some(scrolled), "an update moved the scrolled diff");
}

#[gpui_kit::test]
fn the_lit_file_is_brought_into_view_in_a_long_list(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    let mut page = review();
    page.files = (0..60).map(|i| file(&format!("src/file_{i:02}.rs"), 1, 0)).collect();
    page.file = Some(55);
    hear(cx, &opened, vec![reviewing(page)]);
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let list = cx.update_window(opened.handle, |_, window, _| window.find("review-files-list").bounds()).unwrap();
    let lit = cx.update_window(opened.handle, |_, window, _| window.find(("review-file", 55usize)).bounds()).unwrap();
    assert!(lit.top() >= list.top() && lit.bottom() <= list.bottom(), "the lit file {lit:?} is outside the list {list:?}");
}

#[gpui_kit::test]
fn the_reviews_keys_are_still_heard_after_a_stage_button_is_clicked(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    click(cx, &opened, "review-refresh");
    forget(&opened);
    press(cx, &opened, "j");
    assert_eq!(taken(&opened), ["ReviewAction { action: Next }"]);
    forget(&opened);
    press(cx, &opened, "escape");
    assert_eq!(taken(&opened), ["ReviewAction { action: CloseLayer }"]);
}


// ---- History ----------------------------------------------------------------------------

fn commit_row(row: usize, short: &str, text: &str, expanded: bool, files: Option<usize>) -> HistoryRowView {
    HistoryRowView {
        row,
        kind: "commit",
        short: short.into(),
        text: text.into(),
        ago: "15m".into(),
        files,
        loading: false,
        expanded,
        added: 0,
        removed: 0,
        on: false,
    }
}

fn file_row(row: usize, path: &str, on: bool) -> HistoryRowView {
    HistoryRowView {
        row,
        kind: "file",
        short: String::new(),
        text: path.into(),
        ago: String::new(),
        files: None,
        loading: false,
        expanded: false,
        added: 4,
        removed: 1,
        on,
    }
}

/// An open history of `main`: one commit unfolded to two files with the first under the
/// cursor and its diff held, then a folded commit.
fn history() -> HistoryPage {
    HistoryPage {
        open: true,
        name: "main".into(),
        branch: "main".into(),
        rows: vec![
            commit_row(0, "046eb4f", "Bump version to 0.35.0", true, Some(2)),
            file_row(1, "src/lib.rs", true),
            file_row(2, "README.md", false),
            commit_row(3, "d2b4e2d", "Add seven themes", false, None),
        ],
        commit: Some("046eb4f".into()),
        path: Some("src/lib.rs".into()),
        diff: vec![hunk(0), line(1, "context", "fn main() {"), line(2, "removed", "    old();"), line(3, "added", "    new();")],
        files: 2,
        ..HistoryPage::default()
    }
}

fn historic(page: HistoryPage) -> Event {
    Event::History(Box::new(page))
}

fn history_closed() -> Event {
    Event::History(Box::default())
}

/// The same window with the history open, as the session says.
fn historied(cx: &mut TestAppContext) -> Opened {
    let opened = on_main(cx);
    hear(cx, &opened, vec![historic(history())]);
    opened
}

/// What was asked since the last look, and forgets it.
fn asked(opened: &Opened) -> Vec<String> {
    let said = taken(opened);
    forget(opened);
    said
}

#[gpui_kit::test]
fn a_history_snapshot_draws_its_rows_its_header_and_the_diff_of_the_file_under_the_cursor(cx: &mut TestAppContext) {
    let opened = historied(cx);
    assert!(present(cx, &opened, "git") && present(cx, &opened, "history") && !present(cx, &opened, "review"));
    assert_eq!(selected(cx, &opened, "nav-git"), Some(true), "Git is lit while the session has a history open");
    assert_eq!(selected(cx, &opened, "git-tab-history"), Some(true));
    assert_eq!(selected(cx, &opened, "git-tab-changes"), Some(false));
    assert_eq!(label(cx, &opened, "git-branch").as_deref(), Some("main"));
    assert!(present(cx, &opened, "history-filter"), "the filter field is in the strip");
    assert!(!present(cx, &opened, "review-refresh") && !present(cx, &opened, "review-count"), "the review's buttons are not History's");
    // The commits: the hash and the subject; the cursor's file lit.
    assert_eq!(label(cx, &opened, ("history-row", 0usize)).as_deref(), Some("046eb4f Bump version to 0.35.0"));
    assert_eq!(label(cx, &opened, ("history-row", 1usize)).as_deref(), Some("src/lib.rs"));
    assert_eq!(label(cx, &opened, ("history-row", 3usize)).as_deref(), Some("d2b4e2d Add seven themes"));
    assert_eq!(selected(cx, &opened, ("history-row", 1usize)), Some(true));
    assert_eq!(selected(cx, &opened, ("history-row", 2usize)), Some(false));
    assert_eq!(selected(cx, &opened, ("history-row", 0usize)), Some(false));
    // The diff: the file's header and rows, with no cursor.
    assert_eq!(label(cx, &opened, "history-commit").as_deref(), Some("046eb4f"));
    assert_eq!(label(cx, &opened, "history-diff-head").as_deref(), Some("src/lib.rs"));
    assert!(present(cx, &opened, "history-as-review") && present(cx, &opened, "history-fold-all"));
    for at in 0..4usize {
        assert!(present(cx, &opened, ("diff-row", at)), "row {at}");
    }
    assert!(label(cx, &opened, "history-hints").unwrap().contains("Shift H fold all"));
    assert!(!present(cx, &opened, "history-loading") && !present(cx, &opened, "history-empty"));
}

#[gpui_kit::test]
fn the_lists_loading_and_empty_states_and_the_diffs_are_drawn_in_the_pages_order(cx: &mut TestAppContext) {
    let opened = historied(cx);
    let mut page = HistoryPage { open: true, name: "main".into(), branch: "main".into(), loading: true, ..HistoryPage::default() };
    page.empty = Some("No commits in main yet.".into());
    hear(cx, &opened, vec![historic(page.clone())]);
    assert!(present(cx, &opened, "history-loading") && !present(cx, &opened, "history-empty"), "loading wins over a reason");
    page.loading = false;
    hear(cx, &opened, vec![historic(page)]);
    assert_eq!(label(cx, &opened, "history-empty").as_deref(), Some("No commits in main yet."));
    // The diff: no file says to pick one, then its reason, then the bars, then the rows.
    let mut page = history();
    page.path = None;
    page.diff.clear();
    hear(cx, &opened, vec![historic(page)]);
    assert_eq!(label(cx, &opened, "history-pick").as_deref(), Some("Open a commit and pick a file."));
    assert!(!present(cx, &opened, "history-diff-head"), "no file, no header");
    let mut page = history();
    page.diff.clear();
    page.diff_loading = true;
    page.diff_empty = Some("src/lib.rs is not in this commit's diff.".into());
    hear(cx, &opened, vec![historic(page.clone())]);
    assert!(present(cx, &opened, "history-diff-empty") && !present(cx, &opened, "history-diff-loading"), "a reason wins over loading here");
    page.diff_empty = None;
    hear(cx, &opened, vec![historic(page)]);
    assert!(present(cx, &opened, "history-diff-loading") && !present(cx, &opened, ("diff-row", 0usize)));
}

#[gpui_kit::test]
fn picking_a_file_asks_and_draws_its_diff_only_when_the_event_carries_it(cx: &mut TestAppContext) {
    let opened = historied(cx);
    let mut page = history();
    page.rows[1].on = false;
    page.rows[2].on = true;
    page.path = Some("README.md".into());
    page.diff.clear();
    page.diff_loading = true;
    hear(cx, &opened, vec![historic(page.clone())]);
    forget(&opened);
    click(cx, &opened, ("history-row", 1usize));
    assert_eq!(asked(&opened), ["HistoryAction { action: Next, row: Some(1) }"]);
    // Nothing moved or drew: the cursor and the diff are the session's to change.
    assert_eq!(selected(cx, &opened, ("history-row", 2usize)), Some(true));
    assert_eq!(selected(cx, &opened, ("history-row", 1usize)), Some(false));
    assert!(present(cx, &opened, "history-diff-loading") && !present(cx, &opened, ("diff-row", 1usize)));
    // The event that carries the diff draws it.
    page.diff_loading = false;
    page.diff = vec![hunk(0), line(1, "added", "# Argus")];
    hear(cx, &opened, vec![historic(page)]);
    assert!(present(cx, &opened, ("diff-row", 1usize)) && !present(cx, &opened, "history-diff-loading"));
    assert_eq!(label(cx, &opened, "history-diff-head").as_deref(), Some("README.md"));
}

#[gpui_kit::test]
fn dragging_across_a_historys_diff_selects_text_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = historied(cx);
    forget(&opened);
    drag_rows(cx, &opened, 1, 3);
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
    press(cx, &opened, "ctrl-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()).as_deref(),
        Some("fn main() {
    old();
    new();")
    );
}

#[gpui_kit::test]
fn each_press_on_a_row_or_button_sends_the_call_the_page_sends(cx: &mut TestAppContext) {
    let opened = historied(cx);
    // An open commit folds, a folded one opens, each with its row.
    click(cx, &opened, ("history-row", 0usize));
    assert_eq!(asked(&opened), ["HistoryAction { action: Fold, row: Some(0) }"]);
    click(cx, &opened, ("history-row", 3usize));
    assert_eq!(asked(&opened), ["HistoryAction { action: Unfold, row: Some(3) }"]);
    click(cx, &opened, ("history-row", 2usize));
    assert_eq!(asked(&opened), ["HistoryAction { action: Next, row: Some(2) }"]);
    // A double click is two clicks, as the page hears it, then the choice of the file as a review.
    cx.update_window(opened.handle, |_, window, cx| window.double_click(("history-row", 1usize), cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(
        asked(&opened),
        [
            "HistoryAction { action: Next, row: Some(1) }",
            "HistoryAction { action: Next, row: Some(1) }",
            "HistoryAction { action: Choose, row: None }"
        ]
    );
    click(cx, &opened, "history-fold-all");
    assert_eq!(asked(&opened), ["HistoryAction { action: FoldAll, row: None }"]);
    click(cx, &opened, "history-as-review");
    assert_eq!(asked(&opened), ["HistoryAction { action: AsReview, row: None }"]);
    click(cx, &opened, "git-close");
    assert_eq!(asked(&opened), ["HistoryAction { action: CloseLayer, row: None }"]);
    // None of it moved a row the session had not.
    assert_eq!(selected(cx, &opened, ("history-row", 1usize)), Some(true));
    assert_eq!(label(cx, &opened, ("history-row", 2usize)).as_deref(), Some("README.md"));
}

#[gpui_kit::test]
fn the_filter_sends_each_edit_as_it_stands_and_keeps_its_own_text_across_the_echo(cx: &mut TestAppContext) {
    let opened = historied(cx);
    press(cx, &opened, "/");
    assert!(taken(&opened).is_empty(), "/ focuses the field and sends nothing (SHARED-9): {:?}", taken(&opened));
    type_text(cx, &opened, "fix");
    let said = asked(&opened);
    assert!(!said.is_empty() && said.iter().all(|call| call.starts_with("FilterHistory")), "{said:?}");
    assert_eq!(said.last().map(String::as_str), Some("FilterHistory { filter: \"fix\" }"));
    assert_eq!(keys_typed(&opened), 0, "the letters went to the field, not the pane");
    // The echo of an earlier letter lands after the next: the field's text stays its own.
    let mut echo = history();
    echo.filter = "f".into();
    hear(cx, &opened, vec![historic(echo)]);
    type_text(cx, &opened, "x");
    assert_eq!(asked(&opened).last().map(String::as_str), Some("FilterHistory { filter: \"fixx\" }"), "the field kept fix");
}

#[gpui_kit::test]
fn enter_and_escape_in_the_filter_give_the_list_its_keys_back_and_do_not_close_the_history(cx: &mut TestAppContext) {
    let opened = historied(cx);
    for leave in ["enter", "escape"] {
        press(cx, &opened, "/");
        type_text(cx, &opened, "a");
        forget(&opened);
        press(cx, &opened, leave);
        assert!(taken(&opened).is_empty(), "{leave} in the field sent {:?}", taken(&opened));
        press(cx, &opened, "j");
        assert_eq!(asked(&opened), ["HistoryAction { action: Next, row: None }"], "the list has the keys after {leave}");
    }
    assert!(present(cx, &opened, "history"));
}

#[gpui_kit::test]
fn the_historys_keys_are_heard_by_the_stage_and_never_typed_into_the_pane(cx: &mut TestAppContext) {
    let opened = historied(cx);
    for (key, call) in [
        ("j", "HistoryAction { action: Next, row: None }"),
        ("down", "HistoryAction { action: Next, row: None }"),
        ("k", "HistoryAction { action: Previous, row: None }"),
        ("home", "HistoryAction { action: First, row: None }"),
        ("end", "HistoryAction { action: Last, row: None }"),
        ("l", "HistoryAction { action: Unfold, row: None }"),
        ("h", "HistoryAction { action: Fold, row: None }"),
        ("enter", "HistoryAction { action: Choose, row: None }"),
        ("o", "HistoryAction { action: AsReview, row: None }"),
        ("c", "HistoryAction { action: OpenCode, row: None }"),
        ("escape", "HistoryAction { action: CloseLayer, row: None }"),
    ] {
        press(cx, &opened, key);
        assert_eq!(asked(&opened), [call], "{key}");
    }
    press(cx, &opened, "shift-h");
    assert_eq!(asked(&opened), ["HistoryAction { action: FoldAll, row: None }"]);
    for held in ["x", "1", "q"] {
        press(cx, &opened, held);
    }
    assert!(taken(&opened).is_empty(), "a key the history does not bind is held: {:?}", taken(&opened));
    assert_eq!(keys_typed(&opened), 0);
}

#[gpui_kit::test]
fn a_history_that_gives_way_to_a_review_of_its_commit_draws_the_review_and_empties_the_filter(cx: &mut TestAppContext) {
    let opened = historied(cx);
    press(cx, &opened, "/");
    type_text(cx, &opened, "bump");
    forget(&opened);
    hear(cx, &opened, vec![history_closed(), reviewing(review())]);
    assert!(present(cx, &opened, "review") && !present(cx, &opened, "history"));
    assert_eq!(selected(cx, &opened, "git-tab-changes"), Some(true));
    assert!(taken(&opened).is_empty(), "emptying the field says nothing: {:?}", taken(&opened));
    // The next history starts with an empty field, and the keys are the stage's.
    hear(cx, &opened, vec![closed(), historic(history())]);
    press(cx, &opened, "j");
    assert_eq!(asked(&opened), ["HistoryAction { action: Next, row: None }"]);
    press(cx, &opened, "/");
    type_text(cx, &opened, "z");
    assert_eq!(asked(&opened).last().map(String::as_str), Some("FilterHistory { filter: \"z\" }"), "the old text did not come back");
}

#[gpui_kit::test]
fn the_same_text_typed_in_the_next_history_is_sent_again(cx: &mut TestAppContext) {
    let opened = historied(cx);
    press(cx, &opened, "/");
    type_text(cx, &opened, "z");
    forget(&opened);
    hear(cx, &opened, vec![history_closed()]);
    hear(cx, &opened, vec![historic(history())]);
    press(cx, &opened, "/");
    type_text(cx, &opened, "z");
    assert_eq!(asked(&opened).last().map(String::as_str), Some("FilterHistory { filter: \"z\" }"), "a stale known text sent nothing");
}

#[gpui_kit::test]
fn the_tabs_send_the_calls_that_open_the_other_and_close_this_one(cx: &mut TestAppContext) {
    let opened = historied(cx);
    click(cx, &opened, "git-tab-changes");
    // The history names no checkout: the stage's is the pane's.
    assert_eq!(asked(&opened), ["HistoryAction { action: CloseLayer, row: None }", "OpenReview { checkout: 100 }"]);
    assert_eq!(selected(cx, &opened, "git-tab-history"), Some(true), "the tab is the session's to say");
    click(cx, &opened, "git-tab-history");
    assert!(taken(&opened).is_empty(), "the tab already open sends nothing: {:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_history_whose_daemon_is_gone_keeps_what_it_drew_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = historied(cx);
    hear(cx, &opened, vec![status(Connection::Connecting)]);
    assert!(present(cx, &opened, ("history-row", 1usize)), "the last history stays on screen");
    forget(&opened);
    click(cx, &opened, ("history-row", 0usize));
    click(cx, &opened, "history-fold-all");
    click(cx, &opened, "history-as-review");
    click(cx, &opened, "git-close");
    press(cx, &opened, "j");
    press(cx, &opened, "/");
    type_text(cx, &opened, "a");
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn closing_the_history_gives_the_keys_back_to_the_pane_that_was_shown(cx: &mut TestAppContext) {
    let opened = historied(cx);
    press(cx, &opened, "/");
    hear(cx, &opened, vec![history_closed()]);
    press(cx, &opened, "a");
    assert_eq!(keys_typed(&opened), 1, "the pane has the keys again once the stage is gone");
}

#[gpui_kit::test]
fn a_history_reopened_with_a_filter_shows_it_and_the_next_letter_extends_it(cx: &mut TestAppContext) {
    // HISTORY-7 and SHARED-4: the page fills a fresh field from the history's filter,
    // so a history Code parked and gave back is not filtered by text the field hides.
    let opened = historied(cx);
    hear(cx, &opened, vec![history_closed()]);
    let mut page = history();
    page.filter = "fix".into();
    hear(cx, &opened, vec![historic(page)]);
    assert!(taken(&opened).is_empty(), "filling the field says nothing: {:?}", taken(&opened));
    press(cx, &opened, "/");
    type_text(cx, &opened, "x");
    assert_eq!(asked(&opened).last().map(String::as_str), Some("FilterHistory { filter: \"fixx\" }"));
}

#[gpui_kit::test]
fn the_lit_commit_row_is_brought_into_view_in_a_long_history(cx: &mut TestAppContext) {
    // SHARED-5 and HISTORY-18: the row under the cursor is scrolled to when it moves.
    let opened = historied(cx);
    let mut page = history();
    page.rows = (0..60).map(|i| commit_row(i, "046eb4f", &format!("Commit {i}"), false, None)).collect();
    page.rows[55].on = true;
    hear(cx, &opened, vec![historic(page)]);
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let list = cx.update_window(opened.handle, |_, window, _| window.find("history-list").bounds()).unwrap();
    let lit = cx.update_window(opened.handle, |_, window, _| window.find(("history-row", 55usize)).bounds()).unwrap();
    assert!(lit.top() >= list.top() && lit.bottom() <= list.bottom(), "the lit row {lit:?} is outside the list {list:?}");
}

#[gpui_kit::test]
fn a_commit_rows_meta_says_loading_or_how_many_files_it_unfolded_to(cx: &mut TestAppContext) {
    // HISTORY-2: an unfolded commit counts its files, one waiting on them says so, a folded one says nothing.
    let opened = historied(cx);
    let mut page = history();
    page.rows = vec![
        commit_row(0, "046eb4f", "Loading", true, None),
        commit_row(1, "d2b4e2d", "One file", true, Some(1)),
        commit_row(2, "9a8b7c6", "Two files", true, Some(2)),
        commit_row(3, "1234567", "Folded", false, Some(5)),
    ];
    page.rows[0].loading = true;
    hear(cx, &opened, vec![historic(page)]);
    assert_eq!(label(cx, &opened, ("history-meta", 0usize)).as_deref(), Some("loading\u{2026}"));
    assert_eq!(label(cx, &opened, ("history-meta", 1usize)).as_deref(), Some("1 file"));
    assert_eq!(label(cx, &opened, ("history-meta", 2usize)).as_deref(), Some("2 files"));
    assert!(!present(cx, &opened, ("history-meta", 3usize)), "a folded commit shows no count");
}

#[gpui_kit::test]
fn the_lit_commit_stays_in_view_when_rows_above_it_go_and_it_keeps_its_id(cx: &mut TestAppContext) {
    // SHARED-5 and HISTORY-18: a filter or Fold all that removes rows above the lit one moves it up the list.
    let opened = historied(cx);
    let commits = |range: std::ops::Range<usize>| -> HistoryPage {
        let mut page = history();
        page.rows = range.map(|i| commit_row(i, "046eb4f", &format!("Commit {i}"), false, None)).collect();
        page.rows.iter_mut().find(|row| row.row == 60).unwrap().on = true;
        page
    };
    hear(cx, &opened, vec![historic(commits(0..100))]);
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    hear(cx, &opened, vec![historic(commits(40..100))]);
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let list = cx.update_window(opened.handle, |_, window, _| window.find("history-list").bounds()).unwrap();
    let lit = cx.update_window(opened.handle, |_, window, _| window.find(("history-row", 60usize)).bounds()).unwrap();
    assert!(lit.top() >= list.top() && lit.bottom() <= list.bottom(), "the lit row {lit:?} is outside the list {list:?}");
}

// ---- Tab switch ----------------------------------------------------------------------------

#[gpui_kit::test]
fn the_stage_stays_up_between_a_tab_switchs_close_and_open_and_a_key_there_is_not_the_panes(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    click(cx, &opened, "git-tab-history");
    forget(&opened);
    // Core publishes the close and the open each as a batch of its own.
    hear(cx, &opened, vec![closed()]);
    assert!(present(cx, &opened, "git"), "the stage dropped between the close and the open");
    press(cx, &opened, "j");
    assert_eq!(keys_typed(&opened), 0, "a key in the gap went to the pane: {:?}", taken(&opened));
    hear(cx, &opened, vec![historic(history())]);
    assert!(present(cx, &opened, "history") && present(cx, &opened, "git"));
    press(cx, &opened, "escape");
    assert_eq!(taken(&opened), ["HistoryAction { action: CloseLayer, row: None }"]);
    hear(cx, &opened, vec![history_closed()]);
    assert!(!present(cx, &opened, "git"), "the switch was answered, so the stage goes when the history does");
}

// ---- The split's sash ------------------------------------------------------------------------

fn where_is(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> Bounds<Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find(id).bounds()).unwrap()
}

fn pointer(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>, how: &str) {
    cx.update_window(opened.handle, |_, window, cx| match how {
        "down" => window.pointer_down(at, MouseButton::Left, cx),
        "move" => window.pointer_move(at, Some(MouseButton::Left), cx),
        _ => window.pointer_up(at, MouseButton::Left, cx),
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn by(at: Point<Pixels>, dx: f32) -> Point<Pixels> {
    point(at.x + px(dx), at.y)
}

/// A tab's window, its column, its sash and the width it opens at: Changes, then History.
fn split_of(cx: &mut TestAppContext, tab: usize) -> (Opened, &'static str, &'static str, f32) {
    if tab == 0 {
        (reviewed(cx), "review-column", "review-split", 300.0)
    } else {
        (historied(cx), "history-column", "history-split", 520.0)
    }
}

#[gpui_kit::test]
fn a_sash_press_under_five_pixels_moves_nothing(cx: &mut TestAppContext) {
    for tab in 0..2 {
        let (opened, side, sash, open) = split_of(cx, tab);
        let grab = where_is(cx, &opened, sash).center();
        pointer(cx, &opened, grab, "down");
        pointer(cx, &opened, by(grab, 4.0), "move");
        assert_eq!(where_is(cx, &opened, side).size.width, px(open), "{side}: not yet a drag");
        pointer(cx, &opened, by(grab, 4.0), "up");
        assert_eq!(where_is(cx, &opened, side).size.width, px(open), "{side}: a click");
        assert!(taken(&opened).is_empty(), "{side}: the width is never sent: {:?}", taken(&opened));
    }
}

#[gpui_kit::test]
fn the_column_follows_the_sash_past_five_pixels_and_keeps_where_it_was_let_go(cx: &mut TestAppContext) {
    for tab in 0..2 {
        let (opened, side, sash, open) = split_of(cx, tab);
        let grab = where_is(cx, &opened, sash).center();
        pointer(cx, &opened, grab, "down");
        pointer(cx, &opened, by(grab, 5.0), "move");
        assert_eq!(where_is(cx, &opened, side).size.width, px(open + 5.0), "{side}: 5 px is the threshold");
        pointer(cx, &opened, by(grab, -40.0), "move");
        assert_eq!(where_is(cx, &opened, side).size.width, px(open - 40.0), "{side}");
        pointer(cx, &opened, by(grab, -40.0), "up");
        assert_eq!(where_is(cx, &opened, side).size.width, px(open - 40.0), "{side}: kept once let go");
        assert!(taken(&opened).is_empty(), "{side}: {:?}", taken(&opened));
    }
}

#[gpui_kit::test]
fn escape_during_a_sash_drag_restores_the_width_at_the_press_and_closes_nothing(cx: &mut TestAppContext) {
    for tab in 0..2 {
        let (opened, side, sash, open) = split_of(cx, tab);
        let grab = where_is(cx, &opened, sash).center();
        pointer(cx, &opened, grab, "down");
        pointer(cx, &opened, by(grab, 60.0), "move");
        assert_eq!(where_is(cx, &opened, side).size.width, px(open + 60.0), "{side}");
        press(cx, &opened, "escape");
        assert_eq!(where_is(cx, &opened, side).size.width, px(open), "{side}: Escape restores the width");
        pointer(cx, &opened, by(grab, 60.0), "up");
        assert_eq!(where_is(cx, &opened, side).size.width, px(open), "{side}: the release after it moves nothing");
        assert!(taken(&opened).is_empty(), "{side}: Escape cancelled the drag and did not close the stage: {:?}", taken(&opened));
        assert!(present(cx, &opened, "git"));
    }
}

#[gpui_kit::test]
fn a_sash_press_leaves_the_tabs_keys_with_the_stage(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    let grab = where_is(cx, &opened, "review-split").center();
    pointer(cx, &opened, grab, "down");
    pointer(cx, &opened, by(grab, 30.0), "move");
    pointer(cx, &opened, by(grab, 30.0), "up");
    forget(&opened);
    press(cx, &opened, "j");
    assert_eq!(taken(&opened), ["ReviewAction { action: Next }"], "the keys went back to the stage after the drag");
}

#[gpui_kit::test]
fn left_and_right_scroll_the_diff_sideways_and_send_nothing(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    let mut page = review();
    page.rows = vec![hunk(0), line(1, "context", &"y".repeat(400)), line(2, "context", "z")];
    hear(cx, &opened, vec![reviewing(page)]);
    hear(cx, &opened, vec![]);
    forget(&opened);
    let x = |cx: &mut TestAppContext| cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", 1usize)).bounds().origin.x).unwrap();
    let before = x(cx);
    press(cx, &opened, "right");
    let after = x(cx);
    assert!(after < before, "right moved the line from {before:?} to {after:?}");
    press(cx, &opened, "left");
    assert_eq!(x(cx), before, "left brings it back");
    assert!(taken(&opened).is_empty(), "the arrows are the window's own: {:?}", taken(&opened));
}

/// SHELL-KEYS-14: the dialog hears `Anywhere`'s `?` and no other key of it, as the page's `hear`
/// does: the sheet opens over the review and nothing is sent or typed.
#[gpui_kit::test]
fn a_question_mark_in_the_review_opens_the_shortcuts_sheet_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    let shift = Modifiers { shift: true, ..Modifiers::none() };
    let question = gpui_kit::Keystroke { modifiers: shift, key: "/".into(), key_char: Some("?".into()) };
    cx.update_window(opened.handle, |_, window, cx| window.dispatch_keystroke(question, cx)).unwrap();
    cx.run_until_parked();
    let up = cx.update(|cx| crate::kit::overlay::host::Overlays::get(cx).open_kinds());
    assert_eq!(up, [crate::kit::overlay::kind::Overlay::Sheet]);
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}
