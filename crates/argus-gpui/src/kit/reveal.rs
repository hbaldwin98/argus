//! What a row shows or puts away while it is hovered: the close on a tab, the stage button that
//! takes the place of a file's counts.
//!
//! The change is a width, never `display`. GPUI judges a group's hover twice a frame, from the
//! element's remembered state while laying out and from the pointer while painting, and an element
//! new under a resting pointer gets both answers. A child `display: none` hid from layout and
//! hover then showed is painted without being laid out, and GPUI panics; at no width it is laid
//! out all the same, and clipped.

use gpui_kit::{div, prelude::*, Div, SharedString};

use crate::theme::scale::rem;

/// `child` at its own width while `group` is hovered, or always while `held`; at none otherwise.
pub fn on_hover(group: impl Into<SharedString>, held: bool, child: impl IntoElement) -> Div {
    div()
        .flex_none()
        .overflow_hidden()
        .when(!held, |cell| cell.w(rem(0.0)).group_hover(group, |style| style.w_auto()))
        .child(child)
}

/// `child` at its own width until `group` is hovered, then at none.
pub fn off_hover(group: impl Into<SharedString>, child: impl IntoElement) -> Div {
    div().flex_none().overflow_hidden().group_hover(group, |style| style.w(rem(0.0))).child(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px, Context, Modifiers, TestAppContext, Window};

    /// A row with a revealed button that is a new element each time `fresh` moves on, as the
    /// Changes pane's rows are when it opens under the pointer.
    struct Row {
        fresh: usize,
    }

    impl Render for Row {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let button = div().id(("button", self.fresh)).w(px(10.0)).h(px(10.0)).child(div().w(px(4.0)).h(px(4.0)));
            let counts = div().id(("counts", self.fresh)).w(px(10.0)).h(px(10.0)).child(div().w(px(4.0)).h(px(4.0)));
            div().size_full().child(
                div()
                    .id("row")
                    .group("row")
                    .flex()
                    .w(px(200.0))
                    .h(px(20.0))
                    .child(off_hover("row", counts))
                    .child(on_hover("row", false, button)),
            )
        }
    }

    #[gpui_kit::test]
    fn a_new_row_under_a_resting_pointer_paints(cx: &mut TestAppContext) {
        let (row, cx) = cx.add_window_view(|_, _| Row { fresh: 0 });
        cx.run_until_parked();
        cx.simulate_mouse_move(point(px(5.0), px(5.0)), None, Modifiers::default());
        cx.run_until_parked();
        for _ in 0..3 {
            row.update(cx, |row, cx| {
                row.fresh += 1;
                cx.notify();
            });
            cx.run_until_parked();
        }
    }
}
