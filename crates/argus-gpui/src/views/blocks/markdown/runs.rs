//! A leaf's text as one wrapped run: its marks as faces and colours, its selection as a wash, and
//! its layout kept so a pointer's place can be read back from what is on screen.
//!
//! GPUI's highlights cannot change a family, and an inline code span is mono in a line of the UI
//! face, so the run is told every face itself. The size and line height are the parent's, which is
//! how the type tokens reach it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use argus_desktop_core::views::markdown::runs::{Cut, Styled};
use gpui_kit::{px, FontFeatures, FontStyle, FontWeight, Hsla, SharedString, StrikethroughStyle, StyledText, TextLayout, TextRun, UnderlineStyle};

use crate::theme::Tokens;

/// The text layout of each leaf on screen, by its number, as the last build left them.
pub type Layouts = Rc<RefCell<HashMap<usize, TextLayout>>>;

/// What a leaf is written in before its marks: the colour, the weight, and whether the face is the
/// mono one (a code block).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    pub colour: Hsla,
    pub weight: f32,
    pub mono: bool,
}

/// One cut of a leaf as a run: what its marks make of the face and the colour.
/// `lit` is the link under the pointer, which is underlined.
pub fn run(cut: &Cut, base: Face, lit: Option<usize>, tokens: &Tokens, selection: Hsla) -> TextRun {
    let marks = cut.marks;
    let mono = base.mono || marks.code;
    let weight = if marks.strong { tokens.weight_bold } else { base.weight };
    let colour = if marks.link.is_some() {
        tokens.ref_
    } else if marks.del {
        tokens.fg_faint
    } else {
        base.colour
    };
    TextRun {
        len: cut.range.len(),
        font: gpui_kit::Font {
            family: SharedString::from(if mono { tokens.font_mono } else { tokens.font_ui }),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight(weight),
            style: if marks.em { FontStyle::Italic } else { FontStyle::Normal },
        },
        color: colour,
        background_color: if cut.selected {
            Some(selection)
        } else if marks.code && !base.mono {
            Some(tokens.fill)
        } else {
            None
        },
        underline: (marks.link.is_some() && marks.link == lit).then(|| UnderlineStyle { thickness: px(1.0), color: Some(colour), wavy: false }),
        strikethrough: marks.del.then(|| StrikethroughStyle { thickness: px(1.0), color: Some(tokens.fg_faint) }),
    }
}

/// `styled` as text, `selected` washed, its layout recorded under `leaf` in `layouts`.
pub fn text(leaf: usize, styled: &Styled, selected: Option<Range<usize>>, lit: Option<usize>, base: Face, tokens: &Tokens, layouts: &Layouts) -> StyledText {
    let runs: Vec<TextRun> = styled.cuts(selected).iter().map(|cut| run(cut, base, lit, tokens, tokens.selection)).collect();
    let text = StyledText::new(SharedString::from(styled.text.clone()));
    let text = if runs.is_empty() { text } else { text.with_runs(runs) };
    layouts.borrow_mut().insert(leaf, text.layout().clone());
    text
}
