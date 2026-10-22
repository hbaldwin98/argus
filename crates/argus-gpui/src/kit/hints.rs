//! The footer that names a tab's keys: each as its caps and what it does.
//!
//! The words are the tab's own, so the Changes and History tabs say theirs
//! through the one bar; what is bound and unhinted stays out of it, as in the
//! page. The bar is `.dfoot.meta` and each hint a `.kbar-key`.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, px, SharedString};

use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::Tokens;

/// The caps of one hint, then its words.
pub type Hint = (&'static [&'static str], &'static str);

/// The bar named `id`, whose label is every hint said in order.
pub fn bar(id: &'static str, hints: &'static [Hint], tokens: &Tokens) -> impl IntoElement {
    let said: Vec<String> = hints.iter().map(|(keys, words)| format!("{} {words}", keys.join(" "))).collect();
    let mono = SharedString::from(tokens.font_mono);
    h_flex()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(said.join(", ")))
        .flex_shrink_0()
        .px(px(tokens.sp_24))
        .py(px(tokens.sp_16))
        .gap(px(tokens.sp_8))
        .items_center()
        .border_t_1()
        .border_color(tokens.line)
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted)
        .children(hints.iter().map(|(keys, words)| {
            h_flex()
                .gap(px(tokens.sp_6))
                .items_center()
                .whitespace_nowrap()
                .children(keys.iter().map(|key| keycap(SharedString::from((*key).to_owned()), mono.clone(), tokens)))
                .child(*words)
        }))
}
