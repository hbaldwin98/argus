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

use crate::kit::icon_button::{icon_button, Node};
use crate::kit::icons::Glyph;
use crate::theme::Tokens;
use crate::style::Typed as _;

/// `--tab-max`.
pub const TAB_MAX: f32 = 220.0;

/// `.dock-tabs`, `.chead`: `--strip-h` tall, 8px in from the edges, 4px
/// between its children, on the chrome colour over a hairline.
pub fn bar(tokens: &Tokens) -> Div {
    h_flex()
        .flex_shrink_0()
        .h(px(tokens.strip_h))
        .px(px(tokens.sp_8))
        .gap(px(tokens.sp_4))
        .items_center()
        .bg(tokens.bg_chrome)
        .border_b_1()
        .border_color(tokens.line)
}

/// `.dock-tab`, `.ctab`: a chip `--icon-btn` tall. The selected one is on the
/// selected colour with a strong edge, and the edge lights to the ring colour
/// under the pointer; another lifts to `--fill`. `closes` is whether a close
/// button follows the title, which narrows the padding on that side.
///
/// The title is the caller's child, in [`title`], so the press and the
/// tooltip are its own.
pub fn tab(id: impl Into<ElementId>, on: bool, closes: bool, label: impl Into<SharedString>, tokens: &Tokens) -> Node {
    let (fill, ring) = (tokens.fill, tokens.accent);
    h_flex()
        .id(id)
        .role(Role::Tab)
        .test_support()
        .aria_selected(on)
        .aria_label(label)
        .flex_shrink_0()
        .min_w_0()
        .max_w(px(TAB_MAX))
        .h(px(tokens.icon_btn))
        .items_center()
        .rounded(px(tokens.r_sm))
        .border_1()
        .border_color(if on { tokens.line_strong } else { transparent_black() })
        .text_size(px(tokens.fs_sm))
        .weighted(tokens.weight_medium)
        .text_color(if on { tokens.fg } else { tokens.fg_muted })
        .when(on, |tab| tab.bg(tokens.bg_selected).hover(move |style| style.border_color(ring)))
        .when(!on, |tab| tab.hover(move |style| style.bg(fill)))
        // The pick's padding: 12px before the title, 4px before a close, else 12px.
        .pl(px(tokens.sp_12))
        .pr(px(if closes { tokens.sp_4 } else { tokens.sp_12 }))
        .cursor_pointer()
}

/// `.ttl`: a long title is cut short with an ellipsis, never wrapped.
pub fn title(text: impl Into<SharedString>) -> Div {
    div().min_w_0().truncate().child(text.into())
}

/// `.x`: a tab's close, 24px, faint until the pointer is on it.
pub fn close_x(id: impl Into<ElementId>, words: impl Into<SharedString>, tokens: &Tokens) -> Node {
    icon_button(id, words, false, tokens)
        .w(px(tokens.control_h_sm))
        .h(px(tokens.control_h_sm))
        .mr(px(tokens.sp_4))
        .text_color(tokens.fg_faint)
        .child(Glyph::Close.at(tokens.icon_xs))
}

/// `.icon`, as the kit's button, for the ones that open a menu (the kit's
/// dropdown is a method of its button only): 28px, muted, the page's fill on
/// hover and press. `glyph` is drawn at `at` pixels.
pub fn menu_button(id: impl Into<ElementId>, glyph: Glyph, at: f32, tokens: &Tokens, cx: &App) -> Button {
    let text = tokens.fg;
    let variant = ButtonCustomVariant::new(cx)
        .foreground(tokens.fg_muted)
        .hover(tokens.fill)
        .active(tokens.fill.blend(text.opacity(tokens.share_press_icon)));
    Button::new(id)
        .custom(variant)
        .w(px(tokens.icon_btn))
        .h(px(tokens.icon_btn))
        .p_0()
        .rounded(px(tokens.r_sm))
        .child(glyph.at(at))
}
