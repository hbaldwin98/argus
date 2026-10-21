//! The footer that names a tab's keys: each as its caps and what it does.
//!
//! The words are the tab's own, so the Changes and History tabs say theirs
//! through the one bar; what is bound and unhinted stays out of it, as in the
//! page.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, SharedString};

use crate::theme::Ledger;

/// The caps of one hint, then its words.
pub type Hint = (&'static [&'static str], &'static str);

/// The bar named `id`, whose label is every hint said in order.
pub fn bar(id: &'static str, hints: &'static [Hint], ledger: &Ledger) -> impl IntoElement {
    let said: Vec<String> = hints.iter().map(|(keys, words)| format!("{} {words}", keys.join(" "))).collect();
    h_flex()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(said.join(", ")))
        .flex_shrink_0()
        .h(px(40.0))
        .px_4()
        .gap_3()
        .items_center()
        .border_t_1()
        .border_color(ledger.line)
        .text_xs()
        .text_color(ledger.faint)
        .children(hints.iter().map(|(keys, words)| {
            h_flex()
                .gap_1()
                .items_center()
                .children(keys.iter().map(|key| {
                    div()
                        .px_1()
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(ledger.line)
                        .text_color(ledger.muted)
                        .child(SharedString::from((*key).to_owned()))
                }))
                .child(*words)
        }))
}
