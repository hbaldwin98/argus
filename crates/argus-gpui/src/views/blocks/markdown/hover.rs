//! Which link the pointer is over: it is underlined, the pointer turns to a hand and its address is
//! named beside the pointer, as the page's `.vlink` does (`cursor: pointer`, underline on hover,
//! the URL as its `title`), so what a click will open can be seen before it is clicked.
//!
//! A Markdown's text is one wrapped run per leaf, so the link is read back from the leaf layouts of
//! the last build rather than from an element of its own.

use gpui_kit::TestSupportExt as _;
use gpui_kit::{anchored, deferred, div, point, prelude::*, AnyElement, Context, MouseMoveEvent, Pixels, Point, SharedString, Window};

use super::Markdown;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// How far the address sits from the pointer, so it does not hide the link.
const OFFSET: f32 = 14.0;

/// The link under the pointer: its leaf, which of the leaf's links it is, and where the pointer is.
#[derive(Clone, Copy, PartialEq)]
pub struct Hover {
    pub leaf: usize,
    pub link: usize,
    pub at: Point<Pixels>,
}

impl Markdown {
    /// The link of a leaf the point `at` is on, as `(leaf, link)`.
    pub(super) fn link_under(&self, at: Point<Pixels>) -> Option<(usize, usize)> {
        let layouts = self.layouts.borrow();
        let (leaf, layout) = layouts.iter().find(|(_, layout)| layout.bounds().contains(&at))?;
        let Ok(index) = layout.index_for_position(at) else { return None };
        let styled = &self.leaves.get(*leaf)?.styled;
        let (_, marks) = styled.spans.iter().find(|(range, _)| range.contains(&index))?;
        Some((*leaf, marks.link?))
    }

    /// The pointer moved to `event`'s place: what it is over changes what is drawn only when it
    /// crosses into or out of a link, or moves within one, where its address follows it.
    pub(super) fn moved(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        // A drag selects text; nothing is a link while it goes on.
        let hover = if self.drag.at().is_some() { None } else { self.link_under(event.position).map(|(leaf, link)| Hover { leaf, link, at: event.position }) };
        if hover != self.hover {
            self.hover = hover;
            cx.notify();
        }
    }

    /// The address of the link under the pointer, on a card beside it.
    pub(super) fn address(&self, rem_size: Pixels, tokens: &Tokens) -> Option<AnyElement> {
        let hover = self.hover?;
        let url = SharedString::from(self.leaves.get(hover.leaf)?.styled.links.get(hover.link)?.clone());
        let size = pixels(OFFSET, rem_size);
        let card = div()
            .id(SharedString::from(format!("{}-address", self.id)))
            .test_support()
            .aria_label(url.clone())
            .max_w(rem(tokens.dialog_w))
            .px(rem(tokens.sp_8))
            .py(rem(tokens.sp_4))
            .rounded(rem(tokens.r_sm))
            .border(rem(tokens.hairline))
            .border_color(tokens.line_strong)
            .bg(tokens.bg_surface)
            .typed(tokens.type_small)
            .text_color(tokens.fg)
            .child(url);
        Some(deferred(anchored().position(hover.at + point(size, size)).snap_to_window().child(card)).into_any_element())
    }
}
