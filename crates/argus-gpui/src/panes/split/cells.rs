//! What fills a cell of the grid: a terminal, or the place a browser will fill.
//!
//! A browser pane lives in the grid as any pane does (the Browsers stage is its other home), and
//! its cell draws the owner's screencast where a terminal would be. That view is not in the GPUI
//! window yet, so the kind is reserved from the start: the cell knows it is not a terminal, keeps
//! its place, its header and its drop zones, and says what it is, rather than drawing the screen of
//! a program that is not running.

use argus_desktop_core::connection::page::Shown;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, SharedString};

use crate::theme::Tokens;

/// What a cell's body is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Terminal,
    Browser,
}

impl Kind {
    /// The kind of the pane in a cell; a cell the tree no longer names keeps its terminal's place.
    pub fn of(pane: Option<&Shown>) -> Kind {
        if pane.is_some_and(|pane| pane.browser) {
            Kind::Browser
        } else {
            Kind::Terminal
        }
    }
}

/// The body of a cell that is not a terminal (or has no screen to draw): what it is, in the muted
/// colour, centred.
pub fn reserved(slot: u8, kind: Kind, tokens: &Tokens) -> impl IntoElement {
    let words = match kind {
        Kind::Browser => "A browser pane: not in the GPUI window yet.",
        Kind::Terminal => "",
    };
    v_flex()
        .id(SharedString::from(format!("cell-reserved-{slot}")))
        .test_support()
        .aria_label(SharedString::from(words))
        .size_full()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(tokens.fg_muted)
        .child(div().child(words))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(browser: bool) -> Shown {
        Shown {
            pane: 1,
            repository: String::new(),
            checkout: String::new(),
            branch: String::new(),
            agent: false,
            browser,
            title: "t".into(),
            status: "idle",
            resumed: None,
        }
    }

    #[test]
    fn a_browser_pane_is_not_a_terminal_and_every_other_cell_is() {
        assert_eq!(Kind::of(Some(&pane(true))), Kind::Browser);
        assert_eq!(Kind::of(Some(&pane(false))), Kind::Terminal);
        assert_eq!(Kind::of(None), Kind::Terminal, "a cell the tree no longer names keeps its place");
    }
}
