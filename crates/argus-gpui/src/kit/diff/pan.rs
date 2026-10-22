//! Moving the diff sideways without the scrollbar: the arrow keys' nudge and a held middle button.

use gpui_kit::{point, px, Context, MouseDownEvent, MouseMoveEvent, MouseButton, Pixels, Point};

use super::DiffView;

impl DiffView {
    /// Scrolls sideways by `cells` (left is negative), for the arrow keys.
    pub fn nudge(&mut self, cells: f32, cx: &mut Context<Self>) {
        let step = self.advance.max(px(1.0)) * cells;
        let offset = self.scroll.offset();
        self.place(point(offset.x - step, offset.y), cx);
    }

    /// Puts the scroll at `offset`, held to what the content has to show.
    fn place(&mut self, offset: Point<Pixels>, cx: &mut Context<Self>) {
        let most = self.scroll.max_offset();
        self.scroll.set_offset(point(offset.x.clamp(-most.x, px(0.0)), offset.y.clamp(-most.y, px(0.0))));
        cx.notify();
    }

    /// The middle button went down: the pointer and the scroll it started from.
    pub(super) fn begin_pan(&mut self, event: &MouseDownEvent) {
        self.panning = Some((event.position, self.scroll.offset()));
    }

    /// The pointer moved with the middle button held: the diff follows it, both ways.
    pub(super) fn pan(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((from, offset)) = self.panning else { return };
        if event.pressed_button != Some(MouseButton::Middle) {
            self.panning = None;
            return;
        }
        self.place(offset + (event.position - from), cx);
    }

    pub(super) fn end_pan(&mut self) {
        self.panning = None;
    }
}
