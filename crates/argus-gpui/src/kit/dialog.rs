//! The parts a dialog is cut from: the head with its title and its close button, the foot bar with
//! what it says on the left, and the `Esc` cap of a Cancel.
//!
//! The kit's `Dialog` is the surface and its padding is taken off (`p_0`), so a dialog that uses
//! these draws what the page's `.dhead` and `.dfoot` do and the kit adds nothing of its own.

use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, App, ElementId, FocusHandle, SharedString, TestSupportExt as _, Window};

use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.dhead`: the title in the title type and the ✕ at the right edge. `close` is what the ✕ does.
pub fn head(id: &'static str, title: impl IntoElement, close: impl Fn(&mut Window, &mut App) + 'static, focus: Option<&FocusHandle>, tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(rem(tokens.sp_12))
        .px(rem(tokens.sp_24))
        .pt(rem(tokens.sp_24))
        .pb(rem(tokens.sp_16))
        .child(div().flex_1().min_w_0().typed(tokens.type_title).text_color(tokens.fg).child(title))
        .child(
            icon_button(ElementId::Name(format!("{id}-close").into()), "Close", false, tokens)
                .on_click(move |_, window, cx| close(window, cx))
                .when_some(focus.cloned(), |button, focus| button.track_focus(&focus).focus_visible(|style| style))
                .child(Glyph::Close.at(tokens.icon_sm)),
        )
}

/// `.sheet-dialog > .dhead`: a sheet's head is a toolbar one row tall like a stage's, under a rule:
/// the title, what the sheet says of itself (`aside`, the quiet words with their mark) and the close
/// button, with the same `focus` as [`head`].
pub fn sheet_head(
    id: &'static str,
    title: impl IntoElement,
    aside: impl IntoElement,
    close: impl Fn(&mut Window, &mut App) + 'static,
    focus: Option<&FocusHandle>,
    tokens: &Tokens,
) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(rem(tokens.sp_12))
        .min_h(rem(tokens.sp_44 + tokens.sp_12))
        .px(rem(tokens.sp_24))
        .py(rem(tokens.sp_8))
        .border_b_1()
        .border_color(tokens.line)
        .child(div().flex_1().min_w_0().typed(tokens.type_title).text_color(tokens.fg).child(title))
        .child(aside)
        .child(
            icon_button(ElementId::Name(format!("{id}-close").into()), "Close", false, tokens)
                .on_click(move |_, window, cx| close(window, cx))
                .when_some(focus.cloned(), |button, focus| button.track_focus(&focus).focus_visible(|style| style))
                .child(Glyph::Close.at(tokens.icon_sm)),
        )
}

/// `.dfoot`: a bar under a rule, its children in a row. The caller puts its `why` first.
pub fn foot(tokens: &Tokens) -> gpui_kit::Div {
    h_flex().flex_shrink_0().items_center().gap(rem(tokens.sp_8)).px(rem(tokens.sp_24)).py(rem(tokens.sp_16)).border_t_1().border_color(tokens.line)
}

/// `.dwhy`: what the footer says on its left, in the small muted type, taking the room.
pub fn why(words: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    div().flex_1().min_w_0().typed(tokens.type_small).text_color(tokens.fg_muted).child(words.into())
}

/// The `Esc` cap a Cancel carries.
pub fn esc(tokens: &Tokens) -> impl IntoElement {
    keycap("Esc", SharedString::from(tokens.font_mono), tokens)
}

/// `.dialog .label`: the name of a field, above it, in the medium body type.
pub fn label(words: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    let words = words.into();
    div().id(words.clone()).test_support().aria_label(words.clone()).typed(crate::theme::scale::Type::new(tokens.weight_medium, tokens.fs_md, 1.4)).text_color(tokens.fg).child(words)
}

/// The same words over a field, with no name of their own: the field is what is named.
pub fn field_label(words: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    div().typed(crate::theme::scale::Type::new(tokens.weight_medium, tokens.fs_md, 1.4)).text_color(tokens.fg).child(words.into())
}

/// `.meta`: a line of the small muted type, named by its words so a test or a screen reader finds it.
pub fn meta(words: impl Into<SharedString>, tokens: &Tokens) -> crate::kit::icon_button::Node {
    let words = words.into();
    div().id(words.clone()).test_support().aria_label(words.clone()).typed(tokens.type_small).text_color(tokens.fg_muted).child(words)
}
