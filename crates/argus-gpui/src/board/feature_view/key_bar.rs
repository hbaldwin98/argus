//! The key bar under an open feature (BOARD-WORK-19): the keys of the lens read, and on the right Esc
//! back to the features. `E`, `A`, `D` and `R` of the Work lens are on buttons in view or in the menu, so
//! its bar names the rarer ones.

use argus_desktop_core::connection::page::Lens;
use gpui_kit::prelude::*;

use crate::kit::hints::{self, Hint};
use crate::theme::Tokens;

/// The keys of `lens`, each with what it does.
pub fn keys(lens: Lens) -> &'static [(&'static str, &'static str)] {
    match lens {
        Lens::Record => &[("E", "edit"), ("U", "supersede"), ("Y", "copy as ADR"), ("D", "decision")],
        Lens::Flows => &[("F", "expand"), ("D", "bind"), ("E", "source"), ("A", "flow")],
        Lens::Work => &[("S", "under it"), ("B", "brief"), ("T", "link"), ("?", "more")],
    }
}

/// The lens's keys and then Esc at the far end, in the one key bar.
pub fn key_bar(lens: Lens, tokens: &Tokens) -> impl IntoElement {
    let said: Vec<Hint> = keys(lens).iter().map(|(key, what)| (std::slice::from_ref(key), *what)).collect();
    hints::bar("feature-keys", &said, &[(&["Esc"], "back to features")], None, tokens)
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
