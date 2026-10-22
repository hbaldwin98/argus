//! A side's text painted with its highlight colours and its selection, and kept so a pointer's
//! place can be read back from what is on screen.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{prelude::*, FontStyle, HighlightStyle, Hsla, SharedString, StyledText, TextLayout};

use super::text::{self, Code};
use super::tone;
use crate::theme::Tokens;

/// The text layout of each code on screen, by row and half, as the last build left them.
pub(super) type Layouts = Rc<RefCell<HashMap<(usize, usize), TextLayout>>>;

/// What a code needs to be selectable: which it is, what of it is selected
/// (in the text as drawn, wrapped), and where its layout is kept.
pub(super) struct Ink {
    pub(super) row: usize,
    pub(super) half: usize,
    pub(super) selected: Option<Range<usize>>,
    pub(super) selection: Hsla,
    pub(super) layouts: Layouts,
}

/// The side's text with its highlight kinds as colours.
pub(super) fn code(drawn: &Code, tokens: &Tokens, ink: Ink) -> impl IntoElement {
    let highlights = text::runs(drawn, ink.selected.clone()).into_iter().map(|(range, kind, selected)| {
        let tone = kind.and_then(|kind| tone::tone(kind, tokens));
        let style = HighlightStyle {
            color: tone.map(|tone| tone.colour),
            font_style: tone.and_then(|tone| tone.italic.then_some(FontStyle::Italic)),
            background_color: selected.then_some(ink.selection),
            ..HighlightStyle::default()
        };
        (range, style)
    });
    let text = StyledText::new(SharedString::from(drawn.text.clone())).with_highlights(highlights);
    // Kept so a pointer's place can be read back from what is on screen.
    ink.layouts.borrow_mut().insert((ink.row, ink.half), text.layout().clone());
    text
}
