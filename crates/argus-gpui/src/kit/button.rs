//! What the page's text buttons look like: `.btn`, its primary and small
//! forms, and the `.chipf` choice chip.
//!
//! The kit's `Button` is its own look, and the page's is a pill on a strong
//! edge that tints under the pointer, so the dialogs' and the stage's buttons
//! are drawn from these. What a press means is the caller's `on_click`. The
//! page fades the states in `--t-fast`; a GPUI style switches.

use gpui_kit::component::h_flex;
use gpui_kit::component::Colorize as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, transparent_black, ElementId, Hsla, Role, SharedString};

use crate::kit::icon_button::Node;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Which `.btn` it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Plain,
    /// `.btn.primary`.
    Primary,
    /// `.btn.danger`: the danger colour filled, for what cannot be got back.
    Danger,
    /// `.btn.quiet`: no edge and no fill until the pointer is on it.
    Quiet,
}

/// `.btn`, `.btn.sm`, `.btn.primary` and `.btn.quiet`: a pill `--control-h`
/// tall (24 when `small`) with the words in 12px semibold. Plain, it is the
/// text colour on a strong edge, tinted 7% under the pointer with the edge
/// drawn toward the accent, 12% pressed; primary, it is the accent with the
/// window colour on it, lifting a step on hover and sinking on press; quiet,
/// it is muted and lifts to `--fill` and the text colour. The words and any
/// cap are the caller's children.
pub fn button(id: impl Into<ElementId>, words: impl Into<SharedString>, look: Look, small: bool, tokens: &Tokens) -> Node {
    dressed(id, words, look, small, true, tokens)
}

/// A button as `look` dresses it that cannot be pressed now: its fill, edge and words stay (the
/// page's `.btn[disabled]` loses to a tone's own rule), and the pointer does nothing to it.
pub fn inert(id: impl Into<ElementId>, words: impl Into<SharedString>, look: Look, small: bool, tokens: &Tokens) -> Node {
    dressed(id, words, look, small, false, tokens)
}

fn dressed(id: impl Into<ElementId>, words: impl Into<SharedString>, look: Look, small: bool, live: bool, tokens: &Tokens) -> Node {
    let (text, accent, fill) = (tokens.fg, tokens.accent, tokens.fill);
    let (hover, press) = (tokens.hover_fill, tokens.press_fill);
    let edge = tokens.line_strong.blend(accent.opacity(tokens.share_edge));
    let base = shell(id, words, small, live, tokens);
    match look {
        Look::Primary => {
            let on_accent = tokens.fg_on_accent;
            base.border_color(transparent_black())
                .bg(accent)
                .text_color(on_accent)
                .when(live, |node| {
                    node.hover(move |style| style.bg(accent.lighten(tokens.step_lift))).active(move |style| style.bg(accent.darken(tokens.step_lift)))
                })
        }
        Look::Danger => {
            let on_accent = tokens.fg_on_accent;
            base.border_color(tokens.danger)
                .bg(tokens.danger)
                .text_color(on_accent)
                .when(live, |node| {
                    node.hover(move |style| style.bg(tokens.danger.lighten(tokens.step_lift))).active(move |style| style.bg(tokens.danger.darken(tokens.step_lift)))
                })
        }
        Look::Plain => base
            .border_color(tokens.line_strong)
            .text_color(text)
            .when(live, |node| node.hover(move |style| style.bg(hover).border_color(edge)).active(move |style| style.bg(press))),
        Look::Quiet => base
            .border_color(transparent_black())
            .text_color(tokens.fg_muted)
            .when(live, |node| node.hover(move |style| style.bg(fill).text_color(text)).active(move |style| style.bg(press).text_color(text))),
    }
}

/// What every `.btn` is before its look: a pill `--control-h` tall (24 when `small`) with the words in
/// 12px semibold, pressable.
fn shell(id: impl Into<ElementId>, words: impl Into<SharedString>, small: bool, live: bool, tokens: &Tokens) -> Node {
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(words)
        .flex_shrink_0()
        .h(rem(if small { tokens.control_h_sm } else { tokens.control_h }))
        .px(rem(if small { tokens.sp_8 } else { tokens.sp_12 }))
        .gap(rem(tokens.sp_6))
        .items_center()
        .rounded(rem(tokens.r_pill))
        .border_1()
        .text_size(rem(tokens.fs_sm))
        .weighted(tokens.weight_semibold)
        .whitespace_nowrap()
        .when(live, |node| node.cursor_pointer())
}

/// A plain button whose edge and words are `colour`, and stay it under the pointer, which only tints
/// the fill: the page's `.btn.vtone-bad`, `-warn` and `-info`, the tones that are said and not shouted.
pub fn outlined(id: impl Into<ElementId>, words: impl Into<SharedString>, colour: Hsla, small: bool, tokens: &Tokens) -> Node {
    outlined_as(id, words, colour, small, true, tokens)
}

/// [`outlined`] that cannot be pressed now: the tone stays, the pointer does nothing.
pub fn outlined_inert(id: impl Into<ElementId>, words: impl Into<SharedString>, colour: Hsla, small: bool, tokens: &Tokens) -> Node {
    outlined_as(id, words, colour, small, false, tokens)
}

fn outlined_as(id: impl Into<ElementId>, words: impl Into<SharedString>, colour: Hsla, small: bool, live: bool, tokens: &Tokens) -> Node {
    let (hover, press) = (tokens.hover_fill, tokens.press_fill);
    shell(id, words, small, live, tokens)
        .border_color(colour)
        .text_color(colour)
        .when(live, |node| node.hover(move |style| style.bg(hover).border_color(colour)).active(move |style| style.bg(press)))
}

/// `.btn[disabled]`: a button that cannot be pressed now: no fill, the quiet edge and the faint text,
/// and no hover. `words` is what it says, which a caller that has a reason gives as its tooltip.
pub fn off(id: impl Into<ElementId>, words: impl Into<SharedString>, small: bool, tokens: &Tokens) -> Node {
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(words)
        .flex_shrink_0()
        .h(rem(if small { tokens.control_h_sm } else { tokens.control_h }))
        .px(rem(if small { tokens.sp_8 } else { tokens.sp_12 }))
        .gap(rem(tokens.sp_6))
        .items_center()
        .rounded(rem(tokens.r_pill))
        .border_1()
        .border_color(tokens.line)
        .text_color(tokens.fg_faint)
        .text_size(rem(tokens.fs_sm))
        .weighted(tokens.weight_semibold)
        .whitespace_nowrap()
}

/// `.chipf`: a choice among a few, a pill `--icon-btn` tall in the muted
/// colour that lifts to `--fill` and the text colour, and stays lifted, its
/// edge gone into the fill, while it is the one chosen.
pub fn chip(id: impl Into<ElementId>, words: impl Into<SharedString>, on: bool, tokens: &Tokens) -> Node {
    let (fill, text) = (tokens.fill, tokens.fg);
    let base = h_flex()
        .id(id)
        .role(Role::RadioButton)
        .test_support()
        .aria_selected(on)
        .aria_label(words)
        .flex_shrink_0()
        .h(rem(tokens.icon_btn))
        .px(rem(tokens.sp_8))
        .gap(rem(tokens.sp_4))
        .items_center()
        .rounded(rem(tokens.r_pill))
        .border_1()
        .text_size(rem(tokens.fs_sm))
        .whitespace_nowrap()
        .cursor_pointer();
    if on {
        return base.border_color(fill).bg(fill).text_color(text);
    }
    base.border_color(tokens.line_strong)
        .text_color(tokens.fg_muted)
        .hover(move |style| style.bg(fill).text_color(text))
}
