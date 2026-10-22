//! Where the file is scrolled: each file where it was left, and the cursor brought into view when it
//! moves (CODE-41).
//!
//! A hidden list forgets how far it was scrolled, so the offset of each file is kept by path as the
//! person leaves it and put back when its lines are next there. Only once a file's lines are on
//! screen is its offset worth keeping: a file still loading shows nothing to have been scrolled.

use argus_desktop_core::code::scroll::into_view;
use argus_desktop_core::code::stage::CodePage;
use gpui_kit::Context;

use super::CodeFile;

/// The list is about to show `page`: the file it was showing is left, and its offset kept.
pub(super) fn leave(file: &mut CodeFile, page: &CodePage, cx: &Context<CodeFile>) {
    if file.shown != page.path {
        if let Some(left) = file.shown.take() {
            file.scrolled.insert(left, file.diff.read(cx).scrolled_px());
        }
    }
}

/// The list has `page`'s rows: a file whose lines have just arrived is put where it was left, at the top
/// when it was not.
pub(super) fn restore(file: &mut CodeFile, page: &CodePage, cx: &Context<CodeFile>) {
    let Some(path) = page.path.as_ref().filter(|_| !page.lines.is_empty()) else { return };
    if file.shown.as_ref() == Some(path) {
        return;
    }
    file.shown = Some(path.clone());
    file.diff.read(cx).scroll_to_px(file.scrolled.get(path).copied().unwrap_or(0.0));
}

/// A moved cursor is brought into view by the rule, once the list has been laid out and so knows how
/// tall it is. The move waits while the cursor's line has not arrived.
pub(super) fn settle(file: &mut CodeFile, page: &CodePage, cx: &Context<CodeFile>) {
    if !file.settle {
        return;
    }
    let diff = file.diff.read(cx);
    let viewport = diff.viewport_px();
    if viewport <= 0.0 {
        return;
    }
    match diff.row_px(page.at as usize) {
        Some((top, height)) => {
            if let Some(to) = into_view(top, height, diff.scrolled_px(), viewport) {
                diff.scroll_to_px(to);
            }
            file.settle = false;
        }
        None if !page.lines.is_empty() => file.settle = false,
        None => {}
    }
}
