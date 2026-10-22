//! What a stage says beside its title: the Checkouts stage's counts (`.head-meta`), muted and
//! small, the branch it names in mono in the reference colour. A stage's view hands the parts in
//! through its [`Says`](argus_desktop_core::chrome::strip::Says); the strip only draws them.

use argus_desktop_core::chrome::strip::Part;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The meta of `parts`, with `hover` as its tooltip when it has one. Its words are its accessible
/// name, joined.
pub fn meta(parts: &[Part], hover: Option<&'static str>, tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    let words: String = parts.iter().map(Part::words).collect();
    h_flex()
        .id("head-meta")
        .test_support()
        .aria_label(SharedString::from(words))
        .when_some(hover, |meta, words| meta.tooltip(tip(words)))
        .min_w_0()
        .overflow_hidden()
        .typed(tokens.type_small)
        // `.head-meta` overrides `.meta`: --fs-xs and the faint colour; the line height stays.
        .text_size(rem(tokens.fs_xs))
        .text_color(tokens.fg_faint)
        .whitespace_nowrap()
        .children(parts.iter().map(|part| match part {
            Part::Text(text) => div().child(SharedString::from(text.clone())).into_any_element(),
            Part::Mono(text) => div().font_family(mono.clone()).text_color(tokens.ref_).child(SharedString::from(text.clone())).into_any_element(),
            Part::Attention(text) => div().text_color(tokens.attention).child(SharedString::from(text.clone())).into_any_element(),
            Part::Ok(text) => div().text_color(tokens.ok).child(SharedString::from(text.clone())).into_any_element(),
        }))
}
