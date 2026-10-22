//! A palette row drawn: the mark of its kind, its text with the characters the search matched
//! marked, its detail, and what it says at its end (SHELL-PALETTE-7, 10).

use argus_desktop_core::keys::chords::{presses_of, KeyChord};
use argus_desktop_core::keys::rank::Group;
use argus_desktop_core::keys::results::{Aside, Mark, Result};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, FontWeight, HighlightStyle, Hsla, SharedString, StyledText, TestSupportExt as _};
use std::ops::Range;

use crate::kit::icons::Glyph;
use crate::kit::keycap::{keycap, keycaps};
use crate::kit::selected::selected;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The glyph of a kind of row.
pub fn glyph(mark: Mark) -> Glyph {
    match mark {
        Mark::Workspace => Glyph::Workspace,
        Mark::List => Glyph::List,
        Mark::Branch => Glyph::Branch,
        Mark::Code => Glyph::Code,
        Mark::Console => Glyph::Console,
        Mark::Search => Glyph::Search,
        Mark::Keyboard => Glyph::Keyboard,
        Mark::Gear => Glyph::Gear,
        Mark::Plus => Glyph::Plus,
        Mark::Terminal => Glyph::Terminal,
        Mark::Puzzle => Glyph::Puzzle,
        Mark::Chevron => Glyph::Right,
    }
}

/// The byte ranges of the characters at `marks` (character positions) in `text`, a run of
/// neighbours as one range.
pub fn ranges(text: &str, marks: &[usize]) -> Vec<Range<usize>> {
    let mut found: Vec<Range<usize>> = Vec::new();
    for (at, (start, ch)) in text.char_indices().enumerate() {
        if !marks.contains(&at) {
            continue;
        }
        match found.last_mut() {
            Some(last) if last.end == start => last.end = start + ch.len_utf8(),
            _ => found.push(start..start + ch.len_utf8()),
        }
    }
    found
}

/// A status or a standing in its word: what needs a person loud, work in the accent, the rest quiet.
fn tone(word: &str, tokens: &Tokens) -> Hsla {
    match word {
        "waiting" | "needs-review" => tokens.attention,
        "failed" => tokens.danger,
        "working" => tokens.accent,
        "done" => tokens.ok,
        _ => tokens.fg_faint,
    }
}

fn aside(row: &Result, leader: &str, tokens: &Tokens) -> Option<gpui_kit::AnyElement> {
    let mono = SharedString::from(tokens.font_mono);
    match &row.aside {
        Aside::None => None,
        Aside::Word(word) => {
            Some(div().text_size(rem(tokens.fs_xs)).weighted(tokens.weight_semibold).text_color(tone(word, tokens)).child(SharedString::from(word.replace('-', " "))).into_any_element())
        }
        Aside::Git(summary) => Some(div().typed(tokens.type_small).font_family(mono).text_color(tokens.fg_muted).child(SharedString::from(summary.clone())).into_any_element()),
        Aside::Keys { place, chords } => {
            let keys: Vec<Vec<String>> = chords.iter().map(|chord: &KeyChord| presses_of(chord, leader)).collect();
            Some(
                h_flex()
                    .gap(rem(tokens.sp_6))
                    .items_center()
                    .children(place.map(|place| div().typed(tokens.type_small).text_color(tokens.fg_faint).child(place)))
                    .child(keycaps(&keys, tokens))
                    .into_any_element(),
            )
        }
    }
}

/// One row. `on` is the highlighted one, which says what Enter does to it.
pub fn row(at: usize, result: &Result, marks: &[usize], on: bool, leader: &str, tokens: &Tokens) -> crate::kit::icon_button::Node {
    let accent = HighlightStyle { color: Some(tokens.accent), font_weight: Some(FontWeight(650.0)), ..HighlightStyle::default() };
    let text = StyledText::new(SharedString::from(result.text.clone())).with_highlights(ranges(&result.text, marks).into_iter().map(|range| (range, accent)).collect::<Vec<_>>());
    let open = on.then(|| {
        h_flex()
            .gap(rem(tokens.sp_6))
            .items_center()
            .flex_shrink_0()
            .typed(tokens.type_small)
            .text_color(tokens.fg_muted)
            .child("Open")
            .child(keycap("\u{21b5}", SharedString::from(tokens.font_mono), tokens))
    });
    let line = h_flex()
        .id(("palette-row", at))
        .role(gpui_kit::Role::ListBoxOption)
        .test_support()
        .aria_label(SharedString::from(result.text.clone()))
        .aria_selected(on)
        .flex_shrink_0()
        .h(rem(tokens.control_h))
        .px(rem(tokens.sp_8))
        .gap(rem(tokens.sp_8))
        .items_center()
        .rounded(rem(tokens.r_sm))
        .cursor_pointer()
        .text_color(tokens.fg)
        .child(div().flex_shrink_0().text_color(tokens.fg_muted).child(glyph(result.mark).at(tokens.icon_sm)))
        .child(div().flex_1().min_w_0().truncate().child(text))
        .children(result.detail.clone().map(|detail| div().min_w_0().truncate().typed(tokens.type_small).font_family(SharedString::from(tokens.font_mono)).text_color(tokens.fg_muted).child(SharedString::from(detail))))
        .children(open)
        .children(aside(result, leader, tokens));
    selected(line, on, tokens)
}

/// `.mlabel` over a group's rows.
pub fn heading(group: Group, tokens: &Tokens) -> impl IntoElement {
    div()
        .id(("palette-group", group as usize))
        .test_support()
        .aria_label(group.title())
        .flex_shrink_0()
        .px(rem(tokens.sp_8))
        .pt(rem(tokens.sp_6))
        .pb(rem(tokens.sp_2))
        .typed(crate::theme::scale::Type::new(tokens.weight_semibold, tokens.fs_sm, 1.4))
        .text_color(tokens.fg_muted)
        .child(group.title())
}

#[cfg(test)]
mod tests;
