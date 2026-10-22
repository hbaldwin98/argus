//! What the page's `.icon` button looks like, for every surface that has one.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, px, ElementId, Role, SharedString};

use crate::theme::Tokens;

/// A box with an id, the shape every button here is built on.
pub type Node = gpui_kit::base::ObservedElement<gpui_kit::Stateful<gpui_kit::Div>>;

/// `.icon`: a 28px button in the muted colour that lifts to `--fill` and the
/// text colour, and presses to the fill with 12% of the text colour over it;
/// `hot` is the lit one (`.icon.hot`), accent on the fill. The glyph is the
/// caller's child. The page fades these in `--t-fast`; a GPUI style switches.
pub fn icon_button(id: impl Into<ElementId>, words: impl Into<SharedString>, hot: bool, tokens: &Tokens) -> Node {
    let (fill, text) = (tokens.fill, tokens.fg);
    let (rest, over) = if hot { (tokens.accent, tokens.accent) } else { (tokens.fg_muted, text) };
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(words)
        .h(px(tokens.icon_btn))
        .w(px(tokens.icon_btn))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(tokens.r_sm))
        .cursor_pointer()
        .text_color(rest)
        .when(hot, |button| button.bg(fill))
        .hover(move |style| style.bg(if hot { fill.blend(text.opacity(tokens.share_hover_icon)) } else { fill }).text_color(over))
        .active(move |style| style.bg(fill.blend(text.opacity(tokens.share_press_icon))).text_color(over))
}
