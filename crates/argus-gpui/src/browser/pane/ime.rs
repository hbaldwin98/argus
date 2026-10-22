//! The text an input method gives a browser's screen: GPUI's input handler for the cell, which
//! holds no text of its own.
//!
//! The page's browser has no path for composed text (SHELL-BROWSER-13 (d)); this fills that gap
//! with the rule its terminal follows (`panes/input.ts` `compositionend`, SHELL-INPUT-3): no
//! preedit is shown, and the daemon has no composition message, only `Input.insertText`; so a
//! marked range is held here, drawn nowhere, and sent as one `Text` when the platform commits or
//! unmarks it. A plain key never gets here: the key walk
//! takes it as text and stops it, so the platform's own `WM_CHAR` for it is never typed (see
//! `keys`). This is for IME commits, the emoji panel, dead keys and a key typed as several
//! characters.

use std::ops::Range;

use argus_desktop_core::connection::page::Call;
use argus_protocol::BrowserInput;
use gpui_kit::{px, size, Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};

use super::BrowserPane;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The preedit text the input method is composing: kept so ending the composition sends it, and
/// replaced (never added to) as the person keeps typing.
#[derive(Default)]
pub struct Marked(String);

impl Marked {
    /// UTF-16 length, which is the unit the platform counts the range in.
    fn len16(&self) -> usize {
        self.0.encode_utf16().count()
    }

    /// The text to send for a commit: the commit replaces the preedit, so only it is sent.
    pub fn commit(&mut self, text: &str) -> Option<String> {
        self.0.clear();
        Some(text.to_owned()).filter(|text| !text.is_empty())
    }

    /// A new preedit replaces the last; an empty one cancels it. Nothing is sent.
    pub fn mark(&mut self, text: &str) {
        self.0 = text.to_owned();
    }

    /// Ending the composition where it stands commits what was marked (`compositionend`'s data).
    pub fn unmark(&mut self) -> Option<String> {
        Some(std::mem::take(&mut self.0)).filter(|text| !text.is_empty())
    }
}

impl BrowserPane {
    fn type_text(&self, text: Option<String>) {
        if let Some(text) = text {
            self.caller.call(Call::BrowserInput { pane: self.pane, input: BrowserInput::Text { text } });
        }
    }
}

impl EntityInputHandler for BrowserPane {
    fn text_for_range(&mut self, range: Range<usize>, adjusted: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        // Only the preedit is known here; the page's own text is across the process boundary.
        let marked: Vec<u16> = self.marked.0.encode_utf16().collect();
        let (start, end) = (range.start.min(marked.len()), range.end.min(marked.len()));
        *adjusted = Some(start..end);
        String::from_utf16(&marked[start..end]).ok()
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        let end = self.marked.len16();
        Some(UTF16Selection { range: end..end, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        Some(0..self.marked.len16()).filter(|range| range.end > 0)
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        let text = self.marked.unmark();
        self.type_text(text);
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, _: &mut Context<Self>) {
        let text = self.marked.commit(text);
        self.type_text(text);
    }

    fn replace_and_mark_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) {
        self.marked.mark(text);
    }

    /// A line-high box at the screen's top left corner: the daemon reports no caret, so a candidate
    /// window opens there rather than at the window's. The box has a height because Windows puts
    /// the candidate window at the left edge, half the height down; a whole-screen box would float
    /// it in the middle of the page.
    fn bounds_for_range(&mut self, _: Range<usize>, element_bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let line = rem(Tokens::global(cx).fs_md).to_pixels(window.rem_size());
        Some(Bounds { origin: element_bounds.origin, size: size(px(0.0), line) })
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commit_is_the_text_once_and_ends_the_preedit() {
        let mut marked = Marked::default();
        marked.mark("にほ");
        assert_eq!(marked.commit("日本").as_deref(), Some("日本"));
        assert_eq!(marked.unmark(), None, "the commit consumed the preedit: nothing is sent twice");
    }

    #[test]
    fn a_preedit_is_replaced_as_it_changes_and_sent_only_when_it_ends() {
        let mut marked = Marked::default();
        marked.mark("に");
        marked.mark("にほ");
        marked.mark("にほん");
        assert_eq!(marked.len16(), 3);
        assert_eq!(marked.unmark().as_deref(), Some("にほん"));
        assert_eq!(marked.unmark(), None);
    }

    #[test]
    fn an_emptied_preedit_is_cancelled_and_sends_nothing() {
        let mut marked = Marked::default();
        marked.mark("にほ");
        marked.mark("");
        assert_eq!(marked.unmark(), None);
        assert_eq!(marked.commit(""), None);
    }

    #[test]
    fn ranges_count_utf16_units() {
        let mut marked = Marked::default();
        marked.mark("a😀");
        assert_eq!(marked.len16(), 3);
    }
}
