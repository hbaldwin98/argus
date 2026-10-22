//! The mouse over a screen: the press that hands its pane the keys, the child's mouse reports, a
//! drag that selects and copies on release, and the wheel (SHELL-INPUT-5 to -10, -13).
//!
//! What a pointer event means is `argus-desktop-core`'s (`panes/pointer`): which cell is under it,
//! whether a press is the child's or a selection's, the move not worth a report, the notches. This
//! reads GPUI's events into it and sends the calls it names. A press is followed on the whole
//! window, as the page follows it on `window`, so a drag that leaves the screen goes on and a
//! release anywhere ends it. A selection is only drawn here (`Heard::mark`); the text it copies is
//! read from the session's grid.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::panes::pointer::{cell_at, Cell, Gesture, Unit};
use gpui_kit::{
    canvas, App, Context, DispatchPhase, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    NavigationDirection, Pixels, Point, ScrollDelta, ScrollWheelEvent, WeakEntity, Window,
};

use super::{drawn, Terminal};
use crate::connection::screens::Marked;
use crate::panes::selection::Selection;

/// The DOM's number for a button: left 0, middle 1, right 2, back 3, forward 4.
pub fn dom_button(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::Navigate(NavigationDirection::Back) => 3,
        MouseButton::Navigate(NavigationDirection::Forward) => 4,
    }
}

/// The call a gesture is.
fn call(gesture: Gesture) -> Call {
    match gesture {
        Gesture::Mouse { gesture, button, at } => Call::Mouse { gesture, button, at },
        Gesture::Copy { from, to } => Call::Copy { from, to },
    }
}

impl Terminal {
    /// The cell under a window point, held inside the grid the screen holds.
    fn cell_under(&self, at: Point<Pixels>, cx: &App) -> Cell {
        let (cell_w, cell_h) = self.cell.get();
        let origin = self.origin.get();
        let (rows, cols) = drawn(self.heard.read(cx), self.side, &self.blank).size();
        cell_at(
            f32::from(at.x - origin.x),
            f32::from(at.y - origin.y),
            f32::from(cell_w),
            f32::from(cell_h),
            rows,
            cols,
        )
    }

    /// Whether the child of the pane drawn asked for the mouse reports and the view is live.
    fn takes_mouse(&self, cx: &App) -> bool {
        drawn(self.heard.read(cx), self.side, &self.blank).takes_mouse()
    }

    fn send(&self, gesture: Option<Gesture>) {
        if let Some(gesture) = gesture {
            self.caller.call(call(gesture));
        }
    }

    /// Shows the selection a drag has reached, if it has left the cell it began in.
    fn select(&mut self, head: Cell, cx: &mut Context<Self>) {
        let side = self.side;
        let marked = self.pointer.selection(head).map(|(anchor, head)| Marked { side, selection: Selection { anchor, head } });
        if marked.is_some() {
            self.heard.update(cx, |heard, cx| heard.mark(marked, cx));
        }
    }

    /// A button went down on the screen: the pane gets the keys, every selection goes, and the
    /// press is the child's or the start of a new selection.
    pub(super) fn pressed(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.claim(window, cx);
        self.heard.update(cx, |heard, cx| heard.mark(None, cx));
        let at = self.cell_under(event.position, cx);
        let takes = self.takes_mouse(cx);
        let reported = self.pointer.press(dom_button(event.button), event.modifiers.shift, takes, at);
        self.send(reported);
    }

    /// The pointer moved with no button down over the screen.
    pub(super) fn hovered(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button.is_some() {
            return;
        }
        let at = self.cell_under(event.position, cx);
        let keyed = self.has_keys(self.heard.read(cx));
        let takes = self.takes_mouse(cx);
        let reported = self.pointer.hover(at, takes, keyed);
        self.send(reported);
    }

    /// The pointer moved anywhere in the window while a press is followed.
    fn dragged(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.pointer.active() {
            return;
        }
        let at = self.cell_under(event.position, cx);
        // A button reported up that the window never saw come up: the press is over.
        if event.pressed_button.is_none() {
            let lost = self.pointer.lost(at);
            self.send(lost);
            return;
        }
        let reported = self.pointer.drag(at);
        self.send(reported);
        self.select(at, cx);
    }

    /// A button came up anywhere in the window.
    fn released(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        if !self.pointer.active() {
            return;
        }
        let at = self.cell_under(event.position, cx);
        let reported = self.pointer.release(dom_button(event.button), at);
        self.send(reported);
    }

    /// The wheel over the screen: the whole notches go to the pane under the pointer, whichever
    /// side has the keys. A notch is 100 px or 3 lines; what it does to the pane is the session's.
    pub(super) fn scrolled(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let Some(pane) = self.heard.read(cx).pane_of(self.side) else { return };
        // GPUI's y is positive up, the page's `deltaY` positive down.
        let (delta, unit) = match event.delta {
            ScrollDelta::Pixels(delta) => (-f32::from(delta.y), Unit::Pixels),
            ScrollDelta::Lines(delta) => (-delta.y, Unit::Lines),
        };
        cx.stop_propagation();
        let notches = self.wheel.add(delta, unit);
        if notches != 0 {
            let at = self.cell_under(event.position, cx);
            self.caller.call(Call::Wheel { pane, notches, at });
        }
    }
}

/// An element that follows the pointer on the whole window for the screen it is put among: its
/// moves while a press is held, and the release wherever it comes (the capture phase, so nothing
/// over the screen keeps it).
pub fn watch(terminal: WeakEntity<Terminal>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, _, window: &mut Window, _: &mut App| {
            let moves = terminal.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    moves.update(cx, |terminal, cx| terminal.dragged(event, cx)).ok();
                }
            });
            let ups = terminal.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    ups.update(cx, |terminal, cx| terminal.released(event, cx)).ok();
                }
            });
        },
    )
}
