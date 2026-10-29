//! How a cell says whether it has the keys: the ring round the one that has them, and the step back
//! the others take.
//!
//! The ring is the card's own border in the ring colour, round the whole cell, where the page draws an
//! accent edge under the head alone. The others draw their head and body at `opacity_unkeyed`, and whole
//! again while the pointer is over them; the card behind them stays solid, so a dimmed cell is still a
//! card and not a hole in the stage. The terminal
//! element paints through `paint_quad` and shaped lines, which GPUI multiplies by the element opacity it
//! is painted under, so its screen dims with the rest (a scrim would have been the other way). One pane
//! alone has nothing to be told apart from, so it is neither ringed nor dimmed.

use gpui_kit::Hsla;

use crate::theme::Tokens;

/// Whether the cell is the one with the keys, of a grid of more than one, and whether one without
/// them is pointed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Alone,
    Keyed,
    Unkeyed { pointed: bool },
}

impl Standing {
    pub fn of(split: bool, keyed: bool, pointed: bool) -> Self {
        match (split, keyed) {
            (false, _) => Standing::Alone,
            (true, true) => Standing::Keyed,
            (true, false) => Standing::Unkeyed { pointed },
        }
    }

    /// The card's border.
    pub fn edge(self, tokens: &Tokens) -> Hsla {
        if self == Standing::Keyed { tokens.ring } else { tokens.line }
    }

    /// What the cell's head and body are drawn at.
    pub fn strength(self, tokens: &Tokens) -> f32 {
        if self == (Standing::Unkeyed { pointed: false }) { tokens.opacity_unkeyed } else { 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::themes;

    #[test]
    fn the_keyed_cell_of_a_grid_is_ringed_and_the_others_step_back() {
        // The restyle proposal: ring the keyed cell, the rest at about 62%.
        let tokens = themes::ledger::tokens();
        assert_eq!(Standing::of(true, true, false).edge(&tokens), tokens.ring);
        assert_eq!(Standing::of(true, true, false).strength(&tokens), 1.0);
        assert_eq!(Standing::of(true, false, false).edge(&tokens), tokens.line);
        assert_eq!(Standing::of(true, false, false).strength(&tokens), 0.62);
    }

    #[test]
    fn an_unkeyed_cell_under_the_pointer_is_drawn_whole_but_not_ringed() {
        let tokens = themes::ledger::tokens();
        let pointed = Standing::of(true, false, true);
        assert_eq!(pointed.strength(&tokens), 1.0);
        assert_eq!(pointed.edge(&tokens), tokens.line);
        assert_eq!(Standing::of(true, true, true), Standing::Keyed);
    }

    #[test]
    fn one_pane_alone_is_neither_ringed_nor_dimmed() {
        let tokens = themes::ledger::tokens();
        for keyed in [true, false] {
            assert_eq!(Standing::of(false, keyed, false), Standing::Alone);
            assert_eq!(Standing::of(false, keyed, false).edge(&tokens), tokens.line);
            assert_eq!(Standing::of(false, keyed, false).strength(&tokens), 1.0);
        }
    }

    #[test]
    fn every_theme_dims_an_unkeyed_cell_short_of_the_daemon_gone_and_rings_the_keyed_one_apart_from_the_rest() {
        // `opacity_dim` means the daemon is gone; a cell without the keys must not read as that.
        for (theme, name) in argus_desktop_core::settings::flavours::FLAVOURS.iter() {
            let tokens = crate::theme::choice::tokens(*theme, true, 100);
            let unkeyed = Standing::Unkeyed { pointed: false }.strength(&tokens);
            assert!(unkeyed > tokens.opacity_dim && unkeyed < 1.0, "{name}: {unkeyed}");
            assert_ne!(Standing::Keyed.edge(&tokens), Standing::Unkeyed { pointed: false }.edge(&tokens), "{name}");
        }
    }
}
