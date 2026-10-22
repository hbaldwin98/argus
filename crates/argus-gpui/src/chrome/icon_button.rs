//! What the page's `.icon` button looks like, for every surface that has one.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, px, ElementId, Role, SharedString};

use crate::theme::Ledger;
use crate::tokens::{radius, size};

/// A box with an id, the shape every button here is built on.
pub type Node = gpui_kit::base::ObservedElement<gpui_kit::Stateful<gpui_kit::Div>>;

/// `.icon`: a 28px button in the muted colour that lifts to `--fill` and the
/// text colour, and presses to the fill with 12% of the text colour over it;
/// `hot` is the lit one (`.icon.hot`), accent on the fill. The glyph is the
/// caller's child. The page fades these in `--t-fast`; a GPUI style switches.
pub fn icon_button(id: impl Into<ElementId>, words: impl Into<SharedString>, hot: bool, ledger: &Ledger) -> Node {
    let (fill, text) = (ledger.fill(), ledger.foreground);
    let (rest, over) = if hot { (ledger.accent, ledger.accent) } else { (ledger.muted, text) };
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(words)
        .h(px(size::ICON_BTN))
        .w(px(size::ICON_BTN))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(radius::SM))
        .cursor_pointer()
        .text_color(rest)
        .when(hot, |button| button.bg(fill))
        .hover(move |style| style.bg(if hot { fill.blend(text.opacity(0.10)) } else { fill }).text_color(over))
        .active(move |style| style.bg(fill.blend(text.opacity(0.15))).text_color(over))
}
