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
use crate::theme::Ledger;
use crate::tokens::{fs, radius, size, sp, weight};

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
pub fn button(id: impl Into<ElementId>, words: impl Into<SharedString>, look: Look, small: bool, ledger: &Ledger) -> Node {
    let (text, accent, fill) = (ledger.foreground, ledger.accent, ledger.fill());
    let edge = ledger.line_strong.blend(accent.opacity(0.45));
    let base = h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(words)
        .flex_shrink_0()
        .h(px(if small { size::CONTROL_H_SM } else { size::CONTROL_H }))
        .px(px(if small { sp::S8 } else { sp::S12 }))
        .gap(px(sp::S6))
        .items_center()
        .rounded(px(radius::PILL))
        .border_1()
        .text_size(px(fs::SM))
        .weighted(weight::SEMIBOLD)
        .whitespace_nowrap()
        .cursor_pointer();
    match look {
        Look::Primary => {
            let on_accent = ledger.background;
            base.border_color(transparent_black())
                .bg(accent)
                .text_color(on_accent)
                .hover(move |style| style.bg(accent.lighten(0.06)))
                .active(move |style| style.bg(accent.darken(0.06)))
        }
        Look::Plain => base
            .border_color(ledger.line_strong)
            .text_color(text)
            .hover(move |style| style.bg(text.opacity(0.07)).border_color(edge))
            .active(move |style| style.bg(text.opacity(0.12))),
        Look::Quiet => base
            .border_color(transparent_black())
            .text_color(ledger.muted)
            .hover(move |style| style.bg(fill).text_color(text))
            .active(move |style| style.bg(text.opacity(0.12)).text_color(text)),
    }
}

/// `.chipf`: a choice among a few, a pill `--icon-btn` tall in the muted
/// colour that lifts to `--fill` and the text colour, and stays lifted, its
/// edge gone into the fill, while it is the one chosen.
pub fn chip(id: impl Into<ElementId>, words: impl Into<SharedString>, on: bool, ledger: &Ledger) -> Node {
    let (fill, text) = (ledger.fill(), ledger.foreground);
    let base = h_flex()
        .id(id)
        .role(Role::RadioButton)
        .test_support()
        .aria_selected(on)
        .aria_label(words)
        .flex_shrink_0()
        .h(px(size::ICON_BTN))
        .px(px(sp::S8))
        .gap(px(sp::S4))
        .items_center()
        .rounded(px(radius::PILL))
        .border_1()
        .text_size(px(fs::SM))
        .whitespace_nowrap()
        .cursor_pointer();
    if on {
        return base.border_color(fill).bg(fill).text_color(text);
    }
    base.border_color(ledger.line_strong)
        .text_color(ledger.muted)
        .hover(move |style| style.bg(fill).text_color(text))
}
