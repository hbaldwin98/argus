//! The key bar under an open feature (BOARD-WORK-19): the keys of the lens read, and on the right Esc
//! back to the features. `E`, `A`, `D` and `R` of the Work lens are on buttons in view or in the menu, so
//! its bar names the rarer ones.

use argus_desktop_core::connection::page::Lens;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The keys of `lens`, each with what it does.
pub fn keys(lens: Lens) -> &'static [(&'static str, &'static str)] {
    match lens {
        Lens::Record => &[("E", "edit"), ("U", "supersede"), ("Y", "copy as ADR"), ("D", "decision")],
        Lens::Flows => &[("F", "expand"), ("D", "bind"), ("E", "source"), ("A", "flow")],
        Lens::Work => &[("S", "under it"), ("B", "brief"), ("T", "link"), ("?", "more")],
    }
}

/// `.kbar`: a row `--kbar-h` tall under a hairline, the lens's keys and then Esc at the far end.
pub fn key_bar(lens: Lens, tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    let hint = |key: &'static str, what: &'static str| {
        let mono = mono.clone();
        h_flex().gap(rem(tokens.sp_6)).items_center().whitespace_nowrap().child(keycap(key, mono, tokens)).child(what)
    };
    let said: Vec<String> = keys(lens).iter().map(|(key, what)| format!("{key} {what}")).chain(std::iter::once("Esc back to features".to_owned())).collect();
    h_flex()
        .id("feature-keys")
        .test_support()
        .aria_label(SharedString::from(said.join(", ")))
        .flex_shrink_0()
        .h(rem(tokens.kbar_h))
        .px(rem(tokens.sp_20))
        .gap(rem(tokens.sp_16))
        .items_center()
        .border_t_1()
        .border_color(tokens.line)
        .typed(tokens.type_small)
        .text_color(tokens.fg_faint)
        .children(keys(lens).iter().map(|(key, what)| hint(key, what)))
        .child(div().flex_1())
        .child(hint("Esc", "back to features"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // BOARD-WORK-19, as the page's `keyBar` says them.
    #[test]
    fn each_lens_names_its_own_keys() {
        let words = |lens| keys(lens).iter().map(|(key, what)| format!("{key} {what}")).collect::<Vec<_>>();
        assert_eq!(words(Lens::Work), ["S under it", "B brief", "T link", "? more"]);
        assert_eq!(words(Lens::Record), ["E edit", "U supersede", "Y copy as ADR", "D decision"]);
        assert_eq!(words(Lens::Flows), ["F expand", "D bind", "E source", "A flow"]);
    }
}
