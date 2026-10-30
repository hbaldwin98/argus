//! The file the Code stage reads, in the middle column: its header, the lines drawn read-only with
//! the places lines were removed said between them, the change ruler beside them, and the comment
//! box under the lines it is on (`file`, `code/stage.ts`).
//!
//! Rust holds the file, its cursor and its range (`argus-desktop-core::code`); this draws the page
//! it sends and sends a click on a line as the click it is, so a refused action leaves the file
//! showing what is true. Each file is scrolled where it was last left, kept by path, since a hidden
//! list forgets; and only a move of the cursor brings it into view, and then a third of the way down
//! (`code::scroll`). The lines are the diff's rows in one view (`kit/diff`); while the stage edits the
//! file, `editor` takes their place, the ruler beside it, and the comment box is an in-flow block above them.
//! A markdown file is read as a document (`rendered`) until the header's choice says Source, which
//! this window keeps by path; the lines come back while a comment is open, since it is on them.

pub mod editor;
mod header;
pub mod rendered;
mod ruler;
mod scroll;
mod states;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use argus_desktop_core::code::file::{area, rows, Area};
use argus_desktop_core::code::scroll::cursor_key;
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Bounds, Context, Entity, Pixels, Subscription, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::area;
use crate::kit::diff::{DiffEvent, DiffView, Slot};
use crate::review::comment::{self, ways, CommentBox};
use crate::theme::scale::{css, rem, zoom};
use crate::theme::Tokens;
use editor::CodeEditor;
use rendered::mode::{is_markdown, Mode, Modes};
use rendered::choice::Choose;
use rendered::Rendered;
use ruler::{CodeRuler, Over};

pub use header::bar as header;

pub struct CodeFile {
    heard: Entity<Heard>,
    caller: Caller,
    diff: Entity<DiffView>,
    comment: Entity<CommentBox>,
    ruler: CodeRuler,
    /// The editor that stands in for the lines while the stage edits the file.
    editor: Entity<CodeEditor>,
    /// The markdown file read as a document, and which way each markdown file was last read.
    document: Entity<Rendered>,
    modes: Modes,
    /// Where each file was scrolled, by path: kept here, since a list that is not drawn forgets.
    scrolled: HashMap<String, f32>,
    /// The file whose lines the list is showing, once they are there: what its scroll is kept under.
    shown: Option<String>,
    /// The cursor as last drawn, to tell a move from a redraw.
    cursor: (Option<String>, u32),
    /// A move of the cursor to bring into view once the list has been laid out.
    settle: bool,
    /// Where the file area was laid out, whose width says how much of the head's line there is.
    area: Rc<Cell<Bounds<Pixels>>>,
    /// Whether the stage has room for the outline's column, which the stage says as it lays out.
    pub(super) outline_up: bool,
    _pressed: Subscription,
    _laid_out: Subscription,
}

impl CodeFile {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tokens = *Tokens::global(cx);
        let diff = cx.new(|cx| DiffView::new("code-lines", true, false, cx).reading(&tokens));
        let comment = cx.new(|cx| CommentBox::new(heard.clone(), caller.clone(), Rc::new(ways::Code), window, cx));
        let editor = cx.new(|cx| CodeEditor::new(heard.clone(), caller.clone(), window, cx));
        let ruler = CodeRuler::new(heard.clone(), caller.clone(), diff.clone(), editor.clone(), cx);
        let document = cx.new(|cx| Rendered::new(heard.clone(), caller.clone(), editor.clone(), cx));
        let pressed = cx.subscribe(&diff, |this, _, event: &DiffEvent, cx| match *event {
            // A click on a line moves the cursor to it, and Shift extends a range from where it was.
            DiffEvent::Row { row, extend } => this.ask(CodeGesture::Click { target: CodeClick::Line { line: row as u32, extend } }, cx),
            DiffEvent::CopySelection => {
                if let Some(text) = this.diff.read(cx).selection_text() {
                    // Not through `ask`: a copy needs no daemon.
                    this.caller.call(Call::CopyText { text });
                }
            }
            // A file read whole stages nothing, so it draws no boxes to tick.
            DiffEvent::Fold { .. } | DiffEvent::Range { .. } | DiffEvent::Tick { .. } | DiffEvent::Hunk { .. } => {}
        });
        // The list's first layout says how tall it is, which the cursor's place on screen waits for.
        let laid_out = cx.observe(&diff, |_, _, cx| cx.notify());
        Self { heard, caller, diff, comment, ruler, editor, document, modes: Modes::default(), scrolled: HashMap::new(), shown: None, cursor: (None, 0), settle: false, area: Rc::default(), outline_up: true, _pressed: pressed, _laid_out: laid_out }
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
        let editing = area(page) == Area::Editor;
        let lines = area(page) == Area::Lines;
        let slot = page.comment.as_ref().filter(|_| lines).map(|c| Slot { view: self.comment.clone().into(), height: comment::height(c, zoom(window.rem_size())) });
        let shown = if lines { rows(page) } else { Vec::new() };
        scroll::leave(self, page, cx);
        self.diff.update(cx, |diff, cx| diff.show(&shown, false, slot, cx));
        scroll::restore(self, page, cx);
        // The editor stands in for the lines while the stage edits the file; the page's file, the
        // marks and the line Rust moved to are what it follows.
        if editing {
            self.editor.update(cx, |editor, cx| editor.follow(page, cx));
        } else {
            self.editor.update(cx, |editor, cx| editor.leave(cx));
        }
        let over = if editing { Over::Editor } else { Over::Lines };
        self.ruler.show(if lines || editing { &page.ruler } else { &[] }, over, cx);
        let key = cursor_key(page.path.as_deref(), page.at);
        if key != self.cursor {
            self.cursor = key;
            self.settle = true;
        }
        cx.notify();
    }

    /// How the header offers to read the file shown, when it is a markdown file with text to show,
    /// whether the lines or the editor stand for it. The comment box is on that text, so a comment open
    /// shows it as it is whatever was chosen.
    fn reading(&self, page: &CodePage) -> Option<Mode> {
        let path = page.path.as_deref().filter(|path| is_markdown(path) && matches!(area(page), Area::Lines | Area::Editor))?;
        Some(if page.comment.is_some() { Mode::Source } else { self.modes.of(path) })
    }

    fn reads_rendered(&self, page: &CodePage) -> bool {
        self.reading(page) == Some(Mode::Rendered)
    }

    fn choose(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = self.heard.read(cx).code.clone() else { return };
        if let Some(path) = page.path.clone() {
            self.modes.choose(&path, mode);
            // The text is the editor's again, with the keys it had before the document stood over it.
            if mode == Mode::Source && page.editing {
                self.editor.update(cx, |editor, cx| editor.take_keys(window, cx));
            }
            cx.notify();
        }
    }

    fn body(&self, page: &CodePage, tokens: &Tokens) -> AnyElement {
        match area(page) {
            Area::Failed(why) => states::said("code-failed", why, tokens),
            Area::Note(note) => states::said("code-note", note, tokens),
            Area::Loading => states::loading(tokens),
            // The start panel and the match card are the stage's, which has the press to send.
            Area::Start => div().into_any_element(),
            Area::Editor | Area::Lines if self.reads_rendered(page) => div().id("code-wrap").test_support().size_full().min_h_0().child(self.document.clone()).into_any_element(),
            // In the editor the comment box is an in-flow block above it, not among the lines.
            Area::Editor => v_flex()
                .id("code-wrap")
                .test_support()
                .size_full()
                .min_h_0()
                .when(page.comment.is_some(), |wrap| wrap.child(div().id("code-comment").test_support().flex_shrink_0().px(rem(tokens.sp_12)).py(rem(tokens.sp_8)).border_b_1().border_color(tokens.line).bg(tokens.bg_sunken).child(self.comment.clone())))
                .child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .child(div().flex_1().min_w_0().h_full().child(self.editor.clone()))
                        .when(!page.lines.is_empty(), |wrap| wrap.child(self.ruler.view.clone())),
                )
                .into_any_element(),
            Area::Lines => h_flex()
                .id("code-wrap")
                .test_support()
                .size_full()
                .min_h_0()
                .child(div().id("code-lines-pane").test_support().flex_1().min_w_0().h_full().child(self.diff.clone()))
                .when(!page.lines.is_empty(), |wrap| wrap.child(self.ruler.view.clone()))
                .into_any_element(),
        }
    }
}

impl Render for CodeFile {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).code.clone().filter(|page| page.open) else {
            return div().into_any_element();
        };
        scroll::settle(self, &page, cx);
        let me = cx.weak_entity();
        let choose: Choose = Rc::new(move |mode, window, cx| {
            me.update(cx, |this, cx| this.choose(mode, window, cx)).ok();
        });
        // The first frame has not been measured: it is drawn with room, and the next is true.
        let room = match self.area.get().size.width {
            width if width > gpui_kit::px(0.0) => css(width, window.rem_size()),
            _ => f32::MAX,
        };
        v_flex()
            .id("code-file-area")
            .test_support()
            .relative()
            .size_full()
            .min_h_0()
            .child(area::measure(self.area.clone()))
            .children(header(&page, &self.heard, &self.caller, &tokens, self.reading(&page).map(|mode| (mode, choose)), room, self.outline_up))
            .child(div().flex_1().min_h_0().bg(tokens.bg_sunken).child(self.body(&page, &tokens)))
            .into_any_element()
    }
}

#[cfg(test)]
pub(crate) mod tests;
