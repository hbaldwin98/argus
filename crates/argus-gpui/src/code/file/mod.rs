//! The file the Code stage reads, in the middle column: its header, the lines drawn read-only with
//! the places lines were removed said between them, the change ruler beside them, and the comment
//! box under the lines it is on (`file`, `code/stage.ts`).
//!
//! Rust holds the file, its cursor and its range (`argus-desktop-core::code`); this draws the page
//! it sends and sends a click on a line as the click it is, so a refused action leaves the file
//! showing what is true. Each file is scrolled where it was last left, kept by path, since a hidden
//! list forgets; and only a move of the cursor brings it into view, and then a third of the way down
//! (`code::scroll`). The lines are the diff's rows in one view (`kit/diff`), so the editor that is
//! to stand in for them takes this body's place and keeps the header, the ruler and the box.

mod header;
mod ruler;
mod scroll;
mod states;

use std::collections::HashMap;
use std::rc::Rc;

use argus_desktop_core::code::file::{area, rows, Area};
use argus_desktop_core::code::scroll::cursor_key;
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Context, Entity, Subscription, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::diff::{DiffEvent, DiffView, Slot};
use crate::review::comment::{self, ways, CommentBox};
use crate::theme::scale::zoom;
use crate::theme::Tokens;
use ruler::Ruler;

pub use header::bar as header;

pub struct CodeFile {
    heard: Entity<Heard>,
    caller: Caller,
    diff: Entity<DiffView>,
    comment: Entity<CommentBox>,
    ruler: Entity<Ruler>,
    /// Where each file was scrolled, by path: kept here, since a list that is not drawn forgets.
    scrolled: HashMap<String, f32>,
    /// The file whose lines the list is showing, once they are there: what its scroll is kept under.
    shown: Option<String>,
    /// The cursor as last drawn, to tell a move from a redraw.
    cursor: (Option<String>, u32),
    /// A move of the cursor to bring into view once the list has been laid out.
    settle: bool,
    _pressed: Subscription,
    _laid_out: Subscription,
}

impl CodeFile {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tokens = *Tokens::global(cx);
        let diff = cx.new(|cx| DiffView::new("code-lines", true, false, cx).reading(&tokens));
        let comment = cx.new(|cx| CommentBox::new(heard.clone(), caller.clone(), Rc::new(ways::Code), window, cx));
        let ruler = cx.new(|cx| Ruler::new(heard.clone(), caller.clone(), diff.clone(), cx));
        let pressed = cx.subscribe(&diff, |this, _, event: &DiffEvent, cx| match *event {
            // A click on a line moves the cursor to it, and Shift extends a range from where it was.
            DiffEvent::Row { row, extend } => this.ask(CodeGesture::Click { target: CodeClick::Line { line: row as u32, extend } }, cx),
            DiffEvent::CopySelection => {
                if let Some(text) = this.diff.read(cx).selection_text() {
                    // Not through `ask`: a copy needs no daemon.
                    this.caller.call(Call::CopyText { text });
                }
            }
            DiffEvent::Fold { .. } | DiffEvent::Range { .. } => {}
        });
        // The list's first layout says how tall it is, which the cursor's place on screen waits for.
        let laid_out = cx.observe(&diff, |_, _, cx| cx.notify());
        Self { heard, caller, diff, comment, ruler, scrolled: HashMap::new(), shown: None, cursor: (None, 0), settle: false, _pressed: pressed, _laid_out: laid_out }
    }

    /// The list of lines, for the stage's sideways keys and a test.
    pub fn diff(&self) -> &Entity<DiffView> {
        &self.diff
    }

    pub fn ask(&self, gesture: CodeGesture, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::Code { gesture });
        }
    }

    /// Follows the session: the comment box, then the lines with the box under the one it is on,
    /// then the ruler. Another file puts the one left where it was and brings back where this one
    /// was left; a cursor that moved asks to be brought into view.
    pub fn follow(&mut self, page: &CodePage, window: &mut Window, cx: &mut Context<Self>) {
        self.comment.update(cx, |comment, cx| comment.follow(page.comment.as_ref(), window, cx));
        let lines = matches!(area(page), Area::Lines | Area::Editor);
        let slot = page.comment.as_ref().filter(|_| lines).map(|c| Slot { view: self.comment.clone().into(), height: comment::height(c, zoom(window.rem_size())) });
        let shown = if lines { rows(page) } else { Vec::new() };
        scroll::leave(self, page, cx);
        self.diff.update(cx, |diff, cx| diff.show(&shown, false, slot, cx));
        scroll::restore(self, page, cx);
        self.ruler.update(cx, |ruler, cx| ruler.show(if lines { &page.ruler } else { &[] }, cx));
        let key = cursor_key(page.path.as_deref(), page.at);
        if key != self.cursor {
            self.cursor = key;
            self.settle = true;
        }
        cx.notify();
    }

    fn body(&self, page: &CodePage, tokens: &Tokens) -> AnyElement {
        match area(page) {
            Area::Failed(why) => states::said("code-failed", why, tokens),
            Area::Note(note) => states::said("code-note", note, tokens),
            Area::Loading => states::loading(tokens),
            // The start panel and the match card are the stage's, which has the press to send.
            Area::Start => div().into_any_element(),
            Area::Lines | Area::Editor => h_flex()
                .id("code-wrap")
                .test_support()
                .size_full()
                .min_h_0()
                .child(div().id("code-lines-pane").test_support().flex_1().min_w_0().h_full().child(self.diff.clone()))
                .when(!page.lines.is_empty(), |wrap| wrap.child(self.ruler.clone()))
                .into_any_element(),
        }
    }
}

impl Render for CodeFile {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).code.clone().filter(|page| page.open) else {
            return div().into_any_element();
        };
        scroll::settle(self, &page, cx);
        v_flex()
            .id("code-file-area")
            .test_support()
            .size_full()
            .min_h_0()
            .children(header(&page, &self.heard, &self.caller, &tokens))
            .child(div().flex_1().min_h_0().bg(tokens.bg_sunken).child(self.body(&page, &tokens)))
            .into_any_element()
    }
}

#[cfg(test)]
pub(crate) mod tests;
