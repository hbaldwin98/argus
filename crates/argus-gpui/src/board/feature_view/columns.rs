//! How wide an open feature's outline is (BOARD-WORK-20, -25): a share of the split until the person
//! drags its edge, then the width Rust keeps, drawn from the pointer while the sash is held.
//!
//! Every lens's list takes the same width (`--outline-flex`), though only the Work lens draws the sash.

use gpui_kit::Styled;

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the outline's width is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Width {
    /// Its share of the split: `1.25` to the detail's `1`.
    Share(f32),
    /// A width dragged to, in the page's pixels.
    Fixed(f32),
}

/// The width: the pointer's while the sash is held, else the one Rust keeps, else the usual share.
pub fn width(kept: Option<u32>, held: Option<f32>, tokens: &Tokens) -> Width {
    match (held, kept) {
        (Some(held), _) => Width::Fixed(held),
        (None, Some(kept)) => Width::Fixed(kept as f32),
        (None, None) => Width::Share(tokens.outline_flex),
    }
}

/// `element` at `width`: a share of the split grows from nothing as the page's `flex: 1.25` does (so its own padding and
/// border count against its share, and the detail's too), a dragged width is fixed.
pub fn set<E: Styled>(mut element: E, width: Width) -> E {
    match width {
        Width::Share(share) => {
            let style = element.style();
            style.flex_grow = Some(share);
            style.flex_shrink = Some(1.0);
            style.flex_basis = Some(gpui_kit::px(0.0).into());
            element.min_w_0()
        }
        Width::Fixed(pixels) => element.flex_shrink_0().w(rem(pixels)),
    }
}

/// The detail beside an outline: the rest of the split, 20px in from its edges as `.work-detail` pads it.
pub fn detail(tokens: &Tokens) -> gpui_kit::Div {
    { let mut detail = gpui_kit::div().relative().min_w_0().h_full().px(rem(tokens.sp_20)); let style = detail.style(); style.flex_grow = Some(1.0); style.flex_shrink = Some(1.0); style.flex_basis = Some(gpui_kit::px(0.0).into()); detail }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::themes::ledger;

    // BOARD-WORK-20: the usual width is a share, a dragged one is pixels, and the held one wins.
    #[test]
    fn the_outline_is_a_share_until_dragged_then_the_width_kept_and_while_held_the_pointers() {
        let tokens = ledger::tokens();
        assert_eq!(width(None, None, &tokens), Width::Share(tokens.outline_flex));
        assert_eq!(width(Some(400), None, &tokens), Width::Fixed(400.0));
        assert_eq!(width(Some(400), Some(512.0), &tokens), Width::Fixed(512.0));
        assert!((tokens.outline_share - 1.25 / 2.25).abs() < 1e-6, "1.25 to 1");
    }
}
