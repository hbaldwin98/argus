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
use gpui_kit::{prelude::*, px, transparent_black, ElementId, Role, SharedString};

use crate::chrome::icon_button::Node;
use crate::style::Typed as _;
use crate::theme::Tokens;

/// Which `.btn` it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Plain,
    /// `.btn.primary`.
    Primary,
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
    let (text, accent, fill) = (tokens.fg, tokens.accent, tokens.fill);
    let (hover, press) = (tokens.hover_fill, tokens.press_fill);
    let edge = tokens.line_strong.blend(accent.opacity(tokens.share_edge));
    let base = h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(words)
        .flex_shrink_0()
        .h(px(if small { tokens.control_h_sm } else { tokens.control_h }))
        .px(px(if small { tokens.sp_8 } else { tokens.sp_12 }))
        .gap(px(tokens.sp_6))
        .items_center()
        .rounded(px(tokens.r_pill))
        .border_1()
        .text_size(px(tokens.fs_sm))
        .weighted(tokens.weight_semibold)
        .whitespace_nowrap()
        .cursor_pointer();
    match look {
        Look::Primary => {
            let on_accent = tokens.fg_on_accent;
            base.border_color(transparent_black())
                .bg(accent)
                .text_color(on_accent)
                .hover(move |style| style.bg(accent.lighten(tokens.step_lift)))
                .active(move |style| style.bg(accent.darken(tokens.step_lift)))
        }
        Look::Plain => base
            .border_color(tokens.line_strong)
            .text_color(text)
            .hover(move |style| style.bg(hover).border_color(edge))
            .active(move |style| style.bg(press)),
        Look::Quiet => base
            .border_color(transparent_black())
            .text_color(tokens.fg_muted)
            .hover(move |style| style.bg(fill).text_color(text))
            .active(move |style| style.bg(press).text_color(text)),
    }
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
        .h(px(tokens.icon_btn))
        .px(px(tokens.sp_8))
        .gap(px(tokens.sp_4))
        .items_center()
        .rounded(px(tokens.r_pill))
        .border_1()
        .text_size(px(tokens.fs_sm))
        .whitespace_nowrap()
        .cursor_pointer();
    if on {
        return base.border_color(fill).bg(fill).text_color(text);
    }
    base.border_color(tokens.line_strong)
        .text_color(tokens.fg_muted)
        .hover(move |style| style.bg(fill).text_color(text))
}
