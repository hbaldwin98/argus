//! The file read in the whole window: its lines, header, ruler and comment box, what each press
//! sends, and that nothing changes until the session's next page says so. Rules cited are
//! `gpui/parity/checkouts-code-views.md`'s (`CODE-n`).

use argus_desktop_core::code::stage::{CodeChangeRow, CodeMark, CodeOutlineRow, CodePage, CodeRow};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};

pub(super) use crate::code::stage::tests::{page, says, sent, with};
use crate::keys::root::tests::Opened;

mod comment;
mod header;
mod lines;
mod rendered;
mod ruler;

pub(crate) fn line(no: u32) -> CodeRow {
    CodeRow { no, parts: vec![("let ".into(), "k"), (format!("x{no}"), ""), (" = 1;".into(), "")], added: false, removed_before: 0, cursor: false, picked: false }
}

/// `src/lib.rs` read whole, `lines` lines long, the cursor on `at`.
pub(crate) fn reading(lines: u32, at: u32) -> CodePage {
    CodePage {
        path: Some("src/lib.rs".into()),
        language: Some("Rust"),
        lines: (1..=lines).map(|no| CodeRow { cursor: no == at, ..line(no) }).collect(),
        at,
        outline: true,
        ..page()
    }
}

pub(crate) fn mark(line: u32, span: u32, kind: &'static str) -> CodeMark {
    CodeMark { line, span, kind }
}

pub(crate) fn symbol(name: &str, line: u32, on: bool, changed: bool) -> CodeOutlineRow {
    CodeOutlineRow { name: name.into(), kind: "fn", line, on, changed }
}

pub(crate) fn change(line: u32, text: &str, on: bool) -> CodeChangeRow {
    CodeChangeRow { line, text: text.into(), on }
}

pub(crate) fn bounds_of(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.find(id).bounds()).unwrap()
}

/// The window drawn a few frames, so the list has been laid out and what waited on its size has run.
pub(crate) fn settle(cx: &mut TestAppContext, opened: &Opened) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }
}

