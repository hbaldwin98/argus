//! The parts of a question the person is asked before something cannot be taken back: the danger
//! tile in its head, the warning callout, and the foot with its two answers (`.danger-tile`,
//! `.warn-callout` and `.dfoot` of the page's confirm dialogs).
//!
//! The words and what each answer sends are the caller's: the project's removal question and the
//! Checkouts stage's both draw from these. `Keep it` is the one that has the keys, without a focus
//! ring, so a stray Enter answers no.

use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, App, FocusHandle, SharedString, TestSupportExt as _, Window};

use crate::kit::button::{button, Look};
use crate::kit::dialog::{esc, foot};
use crate::kit::icons::Glyph;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.danger-tile`: the bin on a tile of the danger colour.
pub fn tile(tokens: &Tokens) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .size(rem(tokens.sp_32))
        .rounded(rem(tokens.r_md))
        .bg(tokens.danger.opacity(tokens.share_danger_tile))
        .text_color(tokens.danger)
        .child(Glyph::Trash.at(tokens.icon_box))
}

/// `.warn-callout`: a warning on a wash of the attention colour. `id` names it.
pub fn callout(id: impl Into<SharedString>, words: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    let (id, words) = (id.into(), words.into());
    h_flex()
        .id(id)
        .test_support()
        .aria_label(words.clone())
        .gap(rem(tokens.sp_8))
        .items_start()
        .rounded(rem(tokens.r_md))
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_8))
        .bg(tokens.attention.opacity(tokens.share_callout))
        .text_color(tokens.fg)
        .child(div().mt(rem(tokens.sp_2)).flex_shrink_0().text_color(tokens.attention).child(Glyph::Warn.at(tokens.icon_sm)))
        .child(div().min_w_0().child(words))
}

/// The foot: `keep` says no, with the `Esc` cap and the keys, and `yes` in danger says yes.
/// The buttons are named `<prefix>-keep` and `<prefix>-yes`.
pub fn answers(
    prefix: &'static str,
    keep_words: &'static str,
    yes_words: &'static str,
    focus: &FocusHandle,
    keep: impl Fn(&mut Window, &mut App) + 'static,
    yes: impl Fn(&mut Window, &mut App) + 'static,
    tokens: &Tokens,
) -> impl IntoElement {
    foot(tokens)
        .justify_end()
        .child(
            button(format!("{prefix}-keep"), keep_words, Look::Plain, false, tokens)
                .track_focus(focus)
                .focus_visible(|style| style)
                .child(keep_words)
                .child(esc(tokens))
                .on_click(move |_, window, cx| keep(window, cx)),
        )
        .child(button(format!("{prefix}-yes"), yes_words, Look::Danger, false, tokens).child(yes_words).on_click(move |_, window, cx| yes(window, cx)))
}
