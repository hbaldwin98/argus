//! The one tab strip of the page: the bar, the tab chip and the buttons that
//! the dock's tabs, the console's tabs and a split side's head are all drawn from.
//!
//! The page gives them one set of rules (`.dock-tabs`, `.chead`, `.dock-tab`,
//! `.ctab`), so a tab is the same height, radius and selected state wherever
//! it sits; this is that set, so no strip repeats it. What a tab means, and
//! the press it makes, is the strip's own.

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, transparent_black, App, Div, ElementId, Role, SharedString};

use crate::chrome::icon_button::{icon_button, Node};
use crate::icons::Glyph;
use crate::theme::Ledger;
use crate::tokens::{fs, radius, size, sp, weight};
use crate::style::Typed as _;

/// `--tab-max`.
pub const TAB_MAX: f32 = 220.0;

/// `.dock-tabs`, `.chead`: `--strip-h` tall, 8px in from the edges, 4px
/// between its children, on the chrome colour over a hairline.
pub fn bar(ledger: &Ledger) -> Div {
    h_flex()
        .flex_shrink_0()
        .h(px(size::STRIP_H))
        .px(px(sp::S8))
        .gap(px(sp::S4))
        .items_center()
        .bg(ledger.chrome)
        .border_b_1()
        .border_color(ledger.line)
}

/// `.dock-tab`, `.ctab`: a chip `--icon-btn` tall. The selected one is on the
/// selected colour with a strong edge, and the edge lights to the ring colour
/// under the pointer; another lifts to `--fill`. `closes` is whether a close
/// button follows the title, which narrows the padding on that side.
///
/// The title is the caller's child, in [`title`], so the press and the
/// tooltip are its own.
pub fn tab(id: impl Into<ElementId>, on: bool, closes: bool, label: impl Into<SharedString>, ledger: &Ledger) -> Node {
    let (fill, ring) = (ledger.fill(), ledger.accent);
    h_flex()
        .id(id)
        .role(Role::Tab)
        .test_support()
        .aria_selected(on)
        .aria_label(label)
        .flex_shrink_0()
        .min_w_0()
        .max_w(px(TAB_MAX))
        .h(px(size::ICON_BTN))
        .items_center()
        .rounded(px(radius::SM))
        .border_1()
        .border_color(if on { ledger.line_strong } else { transparent_black() })
        .text_size(px(fs::SM))
        .weighted(weight::MEDIUM)
        .text_color(if on { ledger.foreground } else { ledger.muted })
        .when(on, |tab| tab.bg(ledger.selected).hover(move |style| style.border_color(ring)))
        .when(!on, |tab| tab.hover(move |style| style.bg(fill)))
        // The pick's padding: 12px before the title, 4px before a close, else 12px.
        .pl(px(sp::S12))
        .pr(px(if closes { sp::S4 } else { sp::S12 }))
        .cursor_pointer()
}

/// `.ttl`: a long title is cut short with an ellipsis, never wrapped.
pub fn title(text: impl Into<SharedString>) -> Div {
    div().min_w_0().truncate().child(text.into())
}

/// `.x`: a tab's close, 24px, faint until the pointer is on it.
pub fn close_x(id: impl Into<ElementId>, words: impl Into<SharedString>, ledger: &Ledger) -> Node {
    icon_button(id, words, false, ledger)
        .w(px(size::CONTROL_H_SM))
        .h(px(size::CONTROL_H_SM))
        .mr(px(sp::S4))
        .text_color(ledger.faint)
        .child(Glyph::Close.at(12.0))
}

/// `.icon`, as the kit's button, for the ones that open a menu (the kit's
/// dropdown is a method of its button only): 28px, muted, the page's fill on
/// hover and press. `glyph` is drawn at `at` pixels.
pub fn menu_button(id: impl Into<ElementId>, glyph: Glyph, at: f32, ledger: &Ledger, cx: &App) -> Button {
    let text = ledger.foreground;
    let variant = ButtonCustomVariant::new(cx)
        .foreground(ledger.muted)
        .hover(ledger.fill())
        .active(ledger.fill().blend(text.opacity(0.15)));
    Button::new(id)
        .custom(variant)
        .w(px(size::ICON_BTN))
        .h(px(size::ICON_BTN))
        .p_0()
        .rounded(px(radius::SM))
        .child(glyph.at(at))
}
