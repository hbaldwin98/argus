//! How a page token lands on an element: its type, weight and line height.
//!
//! The numbers are the token space's (`theme`); this is the one place they become style calls,
//! so a view says `.typed(tokens.type_heading)` as the stylesheet says
//! `font: var(--type-heading)` and no view repeats the three calls.

use gpui_kit::{prelude::*, FontWeight};

use crate::theme::scale::Type;


pub trait Typed: Styled + Sized {
    /// `font: var(--type-*)`: weight, size and line height together.
    fn typed(self, ty: Type) -> Self {
        self.font_weight(ty.weight()).text_size(ty.size()).line_height(ty.line_height())
    }

    /// `font-weight` alone, for the rows that keep their size from the parent.
    fn weighted(self, weight: f32) -> Self {
        self.font_weight(FontWeight(weight))
    }
}

impl<T: Styled + Sized> Typed for T {}
