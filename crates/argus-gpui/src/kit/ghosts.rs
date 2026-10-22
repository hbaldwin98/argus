//! What a list or a diff looks like while it loads: the page's `.ghost` bars.
//!
//! Each bar is a share of the width or a fixed width, a `--fill` strip `--ghost-h` tall that
//! fades to 45% and back on the page's 1.4s shimmer, so a pane that has nothing yet is seen to
//! be waiting rather than empty. A bar can be indented, as `ghosts` in `kit/dom.ts` lets one be.

use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, relative, Animation, AnimationExt as _, SharedString};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a bar keeps of its colour at the middle of the shimmer.
const DIM: f32 = 0.45;

/// One bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    width: Width,
    indent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Width {
    /// A share of the row, under 1.
    Share(f32),
    /// Pixels.
    Fixed(f32),
}

impl Bar {
    /// `share` of the row's width.
    pub const fn share(share: f32) -> Self {
        Self { width: Width::Share(share), indent: 0.0 }
    }

    /// `width` pixels wide.
    pub const fn fixed(width: f32) -> Self {
        Self { width: Width::Fixed(width), indent: 0.0 }
    }

    /// Moves it `indent` pixels in from the left.
    pub const fn indented(mut self, indent: f32) -> Self {
        self.indent = indent;
        self
    }

    /// The bar the page's list of numbers names: under 1 a share of the row, else pixels
    /// (`width < 1 ? width * 100% : width px`).
    pub const fn page(width: f32) -> Self {
        if width < 1.0 {
            Self::share(width)
        } else {
            Self::fixed(width)
        }
    }
}

/// The column of bars named `id`: `ghost` has each 8px under the last, and
/// 16px in from the sides.
pub fn ghosts(id: &'static str, bars: &'static [f32], tokens: &Tokens) -> impl IntoElement {
    let bars: Vec<Bar> = bars.iter().map(|width| Bar::page(*width)).collect();
    column(id, &bars, tokens)
}

/// The same column with bars that can be a fixed width or indented.
pub fn column(id: &'static str, bars: &[Bar], tokens: &Tokens) -> impl IntoElement {
    let fill = tokens.fill;
    v_flex().id(id).test_support().size_full().px(rem(tokens.sp_16)).children(bars.iter().copied().enumerate().map(move |(at, bar)| {
        div()
            .id((id, at))
            .mt(rem(tokens.sp_8))
            .ml(rem(bar.indent))
            .flex_shrink_0()
            .h(rem(tokens.ghost_h))
            .map(|row| match bar.width {
                Width::Share(share) => row.w(relative(share)),
                Width::Fixed(width) => row.w(rem(width)),
            })
            .rounded(rem(tokens.r_xs))
            .bg(fill)
            .with_animation(
                SharedString::from(format!("{id}-{at}-shimmer")),
                Animation::new(tokens.motion_shimmer).repeat(),
                // 1 at either end and `DIM` in the middle, as `@keyframes ghost`.
                |bar, delta| bar.opacity(1.0 - (1.0 - DIM) * (1.0 - (2.0 * delta - 1.0).abs())),
            )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pages_numbers_under_one_are_shares_and_the_rest_pixels() {
        assert_eq!(Bar::page(0.4), Bar::share(0.4));
        assert_eq!(Bar::page(120.0), Bar::fixed(120.0));
        assert_eq!(Bar::page(1.0), Bar::fixed(1.0), "a width of 1 is a pixel, as in `width < 1`");
    }

    #[test]
    fn a_bar_is_indented_only_when_asked() {
        assert_eq!(Bar::share(0.5).indent, 0.0);
        assert_eq!(Bar::share(0.5).indented(24.0).indent, 24.0);
    }
}
