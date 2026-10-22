//! The Code stage in the whole window: its frame, the files column, the search and the start panel,
//! what each press and key sends, and that nothing changes until the session's next page says so.
//! Rules cited are `gpui/parity/checkouts-code-views.md`'s (`CODE-n`).
//!
//! The window is the shell over a caller that collects its calls (`keys::root::tests`), and the
//! session's page is sent as an event, so nothing here asks a real session.

use argus_desktop_core::code::stage::{CodeChangedFile, CodeHit, CodeHitFile, CodeJump, CodePage, CodeSearch, CodeTreeRow};
use argus_desktop_core::connection::page::{Event, Rail, Stage};
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{point, px, AppContext as _, ElementId, MouseButton, TestAppContext};

pub(super) use crate::checkouts::stage::tests::{click_id, connected, label, present, rail};
pub(super) use crate::keys::root::tests::{drain, hear, open, press, type_text, Opened};

mod frame;
mod hops;
mod keys;
mod lists;
mod outline;
mod start;

pub(crate) fn code_rail() -> Rail {
    Rail { stage: Stage::Code, ..rail() }
}

pub(crate) fn tree_row(path: &str, folder: bool, changed: bool) -> CodeTreeRow {
    CodeTreeRow {
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        depth: path.matches('/').count(),
        folder,
        open: false,
        changed,
        added: 3,
        removed: 1,
        on: false,
        ignored: false,
    }
}

pub(crate) fn changed(path: &str, added: u32, removed: u32) -> CodeChangedFile {
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    CodeChangedFile { path: path.into(), name: name.into(), dir: dir.into(), added, removed }
}

/// The page of `main` with nothing open: a changed folder, two files, and two ignored folders.
pub(crate) fn page() -> CodePage {
    let ignored = |name: &str| CodeTreeRow { ignored: true, ..tree_row(name, true, false) };
    CodePage {
        open: true,
        checkout: 100,
        name: "main".into(),
        branch: "main".into(),
        changed: 4,
        tree: vec![tree_row("crates", true, true), tree_row("AGENTS.md", false, false), tree_row("DESIGN.md", false, true), ignored("target"), ignored("target-check")],
        changed_files: vec![changed("crates/argusd/src/board/standing.rs", 10, 0), changed("DESIGN.md", 4, 2)],
        ..CodePage::default()
    }
}

/// A connected window on the Code stage that has heard `page`, its calls cleared.
pub(crate) fn with(cx: &mut TestAppContext, page: CodePage) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), Event::Rail(Box::new(code_rail())), Event::Code(Box::new(page))]);
    drain(&opened);
    opened
}

pub(crate) fn says(cx: &mut TestAppContext, opened: &Opened, page: CodePage) {
    hear(cx, opened, vec![Event::Code(Box::new(page))]);
}

pub(crate) fn sent(opened: &Opened) -> Vec<String> {
    drain(opened)
}

pub(crate) fn is_selected(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> Option<bool> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.find(id).selected()).unwrap()
}

pub(crate) fn width_of(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> f32 {
    cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().size.width.as_f32()).unwrap()
}

/// A search with hits in two files, the cursor on the second hit of the first.
pub(crate) fn searching(current: bool) -> CodePage {
    let hit = |index, line, on| CodeHit { index, line, parts: vec![("pub struct ".into(), false), ("Dragged".into(), true), (" {".into(), false)], on };
    CodePage {
        search: Some(CodeSearch {
            query: "Dragged".into(),
            current,
            total: 3,
            files: vec![
                CodeHitFile { path: "src/a.rs".into(), dir: "src/".into(), name: "a.rs".into(), count: 2, open: true, hits: vec![hit(0, 3, false), hit(1, 9, true)] },
                CodeHitFile { path: "src/b.rs".into(), dir: "src/".into(), name: "b.rs".into(), count: 1, open: false, hits: vec![hit(2, 4, false)] },
            ],
            ..CodeSearch::default()
        }),
        ..page()
    }
}

pub(crate) fn jumping() -> CodePage {
    CodePage { jump: Some(CodeJump { query: "ma".into(), matches: vec!["src/main.rs".into(), "README.md".into()], at: Some(1) }), ..page() }
}

/// Where the middle of a sash is, for a drag.
pub(crate) fn sash(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> gpui_kit::Point<gpui_kit::Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().center()).unwrap()
}

pub(crate) fn drag(cx: &mut TestAppContext, opened: &Opened, from: gpui_kit::Point<gpui_kit::Pixels>, dx: f32) {
    let to = point(from.x + px(dx), from.y);
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(from, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(to, Some(MouseButton::Left), cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(to, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

