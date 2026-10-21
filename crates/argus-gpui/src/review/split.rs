//! How wide the left column of a Git tab is: the files beside the diff in
//! Changes, the commits beside it in History.
//!
//! The width is the window's own and is never sent anywhere. It follows the
//! sash through the window's one drag helper (`pointer::drag`), so it moves
//! only after the 5 px threshold and Escape puts it back to what it was at the
//! press (`gpui/parity/cross-cutting.md`, Decided 6), as the docks' sashes do.
//! A tab holds one [`Split`] entity for as long as it lives, so the width is
//! kept while the other tab or a pane is shown.

use gpui_kit::{
    div, prelude::*, px, AnyElement, App, Context, Entity, FocusHandle, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, Role, Window,
};
use gpui_kit::TestSupportExt as _;

use crate::pointer::drag::{self, Drag, Dragger, Step};
use crate::theme::Ledger;

/// The grab zone along the column's right edge, as wide as a dock's.
const SASH: f32 = 6.0;

/// What a press of the sash holds: where the pointer was, and how wide the
/// column was, so the column does not jump to put its edge under the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    from: f32,
    start: f32,
}

pub struct Split {
    id: &'static str,
    label: &'static str,
    width: f32,
    min: f32,
    max: f32,
    drag: Drag<Held>,
    /// The handle the drag takes the keys with, so Escape reaches it.
    keys: FocusHandle,
}

/// The width with the pointer at `x`, within the bounds the column is kept to.
fn width_at(held: Held, x: f32, min: f32, max: f32) -> f32 {
    (held.start + x - held.from).clamp(min, max)
}

impl Split {
    pub fn new(id: &'static str, label: &'static str, open: f32, min: f32, max: f32, cx: &mut Context<Self>) -> Self {
        Self { id, label, width: open, min, max, drag: Drag::new(), keys: cx.focus_handle() }
    }

    pub fn width(&self) -> f32 {
        self.width
    }
}

impl Dragger for Split {
    type Payload = Held;

    fn drag(&self) -> &Drag<Held> {
        &self.drag
    }

    fn drag_mut(&mut self) -> &mut Drag<Held> {
        &mut self.drag
    }

    fn keys(&self) -> &FocusHandle {
        &self.keys
    }

    fn dragged(&mut self, step: Step<Held>, _: &mut Window, _: &mut Context<Self>) {
        match step {
            Step::Began(held) | Step::Moved(held) => {
                if let Some(at) = self.drag.at() {
                    self.width = width_at(held, at.x.as_f32(), self.min, self.max);
                }
            }
            Step::Dropped(held, at) => self.width = width_at(held, at.x.as_f32(), self.min, self.max),
            Step::Cancelled(held) => self.width = held.start,
            Step::Clicked(_) => {}
        }
    }
}

impl Render for Split {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let accent = Ledger::global(cx).accent;
        div()
            .id(self.id)
            .role(Role::Splitter)
            .test_support()
            .aria_label(self.label)
            .size_full()
            .cursor_col_resize()
            .when(self.drag.is_dragging(), |zone| zone.bg(accent))
            .hover(move |style| style.bg(accent.opacity(0.4)))
            // The press is the stage's to take the keys with, as any press on it is:
            // the handle the drag takes them with is on a child that nothing can press.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, _| {
                    let held = Held { from: event.position.x.as_f32(), start: this.width };
                    this.drag.press(event.position, held);
                }),
            )
            .child(
                div()
                    .id((self.id, 1usize))
                    .size_0()
                    .track_focus(&self.keys)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| drag::escape(this, event, window, cx))),
            )
            .child(drag::watch(cx.weak_entity()))
    }
}

/// The left column at the width `split` holds, with its sash along the right
/// edge. `id` names the column, for the test that measures it.
pub fn side(split: &Entity<Split>, id: &'static str, content: impl IntoElement, ledger: &Ledger, cx: &App) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .relative()
        .flex_shrink_0()
        .h_full()
        .w(px(split.read(cx).width()))
        .border_r_1()
        .border_color(ledger.line)
        .child(content)
        .child(div().absolute().top_0().bottom_0().right_0().w(px(SASH)).child(split.clone()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELD: Held = Held { from: 400.0, start: 320.0 };

    #[test]
    fn the_column_keeps_its_offset_from_the_pointer_and_does_not_jump() {
        assert_eq!(width_at(HELD, 400.0, 200.0, 520.0), 320.0);
        assert_eq!(width_at(HELD, 450.0, 200.0, 520.0), 370.0);
        assert_eq!(width_at(HELD, 380.0, 200.0, 520.0), 300.0);
    }

    #[test]
    fn the_column_stays_within_its_bounds() {
        assert_eq!(width_at(HELD, 0.0, 200.0, 520.0), 200.0);
        assert_eq!(width_at(HELD, 9000.0, 200.0, 520.0), 520.0);
    }
}
