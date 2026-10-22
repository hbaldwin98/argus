//! Markdown drawn as a view: the blocks core read (`views::markdown`) as headings, paragraphs,
//! lists, quotes, code and tables whose text can be selected and copied (cross-cutting Decided 4).
//!
//! The text is an agent's, read from somewhere nobody here controls: every string is drawn as text,
//! raw HTML shows as the text it is, an image is a link, and a link is only ever an http(s) one
//! (core guarantees it), whose press asks Rust to open it (`Call::OpenUrl`).
//!
//! Each leaf of the document (a paragraph, a list item, a table cell) is one wrapped run of text,
//! numbered in the order core's `leaves` numbers them, and a place in the document is a leaf and a
//! byte of its text. The selection is this view's own, gone when the text changes; the text it
//! reads as, and what lies between two places, are core's. What copies it is Rust's, through the
//! one clipboard writer (`Call::CopyText`): a right-click on selected text offers Copy, and the
//! surface that hosts a Markdown asks it to `copy_selection` on its copy key. A selection belongs
//! to one Markdown: one cannot run into another block's text.

pub mod draw;
pub mod hover;
pub mod menu;
pub mod runs;
pub mod select;

use argus_desktop_core::views::markdown::blocks::parse;
use argus_desktop_core::views::markdown::leaves::{leaves, Leaf, Pos};
use argus_desktop_core::views::markdown::model::Block;
use gpui_kit::{prelude::*, Context, FocusHandle, SharedString, Window};

use self::hover::Hover;
use self::runs::Layouts;
use self::select::Held;
use crate::connection::caller::Caller;
use crate::kit::drag::Drag;
use crate::theme::Tokens;

pub struct Markdown {
    id: SharedString,
    caller: Caller,
    source: String,
    blocks: Vec<Block>,
    leaves: Vec<Leaf>,
    /// Where the selection began and where it is now, either way round.
    selection: Option<(Pos, Pos)>,
    /// The layout of each leaf this build drew, so a pointer's place is read from what is on screen.
    layouts: Layouts,
    drag: Drag<Held>,
    /// The link the pointer is over, which is underlined and named (`hover`).
    hover: Option<Hover>,
    /// What the drag takes the keys with, so Escape reaches it first.
    keys: FocusHandle,
    /// Whether the whole text is in the muted colour, as a list item's body or a timeline's is.
    muted: bool,
}

impl Markdown {
    /// A Markdown named `id` (for the ids of what is in it) drawing `text`.
    pub fn new(id: impl Into<SharedString>, text: &str, caller: Caller, cx: &mut Context<Self>) -> Self {
        let blocks = parse(text);
        Self {
            id: id.into(),
            caller,
            source: text.to_owned(),
            leaves: leaves(&blocks),
            blocks,
            selection: None,
            layouts: Layouts::default(),
            drag: Drag::new(),
            hover: None,
            keys: cx.focus_handle(),
            muted: false,
        }
    }

    /// Draws `text`. Nothing changes, and nothing is notified, when it is what was drawn already: an
    /// agent shows a view again whenever anything in it changed, and a selection survives a view
    /// whose text did not.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.source == text {
            return;
        }
        self.source = text.to_owned();
        self.blocks = parse(text);
        self.leaves = leaves(&self.blocks);
        self.selection = None;
        self.hover = None;
        cx.notify();
    }

    /// Draws the whole text muted, or not. A selection is kept: only the colour changes.
    pub fn set_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        if self.muted != muted {
            self.muted = muted;
            cx.notify();
        }
    }

    /// How many leaves of text the document has.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }
}

impl Render for Markdown {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        // Only what this build draws is on screen to be read.
        self.layouts.borrow_mut().clear();
        self.body(&tokens, window.rem_size(), cx)
    }
}

#[cfg(test)]
mod tests;
