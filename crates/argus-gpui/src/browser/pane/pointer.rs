//! The person's pointer on a cell's screen, as a window's own is on a page: a move hovers (or drags,
//! with a button down), a press and its release are sent apart so a drag selects, the wheel
//! scrolls the page and never the window, and the cursor drawn is the one the page shows under it.
//!
//! Points are the page's own pixels, found by core's `within`; the bands either side of the picture
//! are off the page, as is anywhere past the screen. A release is sent only for a press the page
//! was sent, wherever it lands: a drag that ends past the screen still lets the button go.

use argus_desktop_core::browser::within::{within, Box2};
use argus_desktop_core::connection::page::Call;
use argus_protocol::{BrowserButton, BrowserCursor, BrowserInput};
use gpui_kit::{Context, CursorStyle, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Point, Pixels, ScrollWheelEvent, Window};

use super::BrowserPane;

/// What the pointer last told the page: where it was, and the button it pressed there.
#[derive(Default)]
pub struct Pointer {
    at: Option<(f64, f64)>,
    down: Option<BrowserButton>,
}

/// The window's cursor for the one the page shows.
pub fn style(cursor: BrowserCursor) -> CursorStyle {
    match cursor {
        BrowserCursor::Default => CursorStyle::Arrow,
        BrowserCursor::Pointer => CursorStyle::PointingHand,
        BrowserCursor::Text => CursorStyle::IBeam,
        BrowserCursor::Crosshair => CursorStyle::Crosshair,
        BrowserCursor::Grab => CursorStyle::OpenHand,
        BrowserCursor::Grabbing => CursorStyle::ClosedHand,
        BrowserCursor::NotAllowed => CursorStyle::OperationNotAllowed,
        BrowserCursor::EwResize => CursorStyle::ResizeLeftRight,
        BrowserCursor::NsResize => CursorStyle::ResizeUpDown,
        BrowserCursor::NeswResize => CursorStyle::ResizeUpRightDownLeft,
        BrowserCursor::NwseResize => CursorStyle::ResizeUpLeftDownRight,
        BrowserCursor::ColResize => CursorStyle::ResizeColumn,
        BrowserCursor::RowResize => CursorStyle::ResizeRow,
    }
}

/// The page's button for the window's; the back and forward buttons are not the page's.
fn button(button: MouseButton) -> Option<BrowserButton> {
    match button {
        MouseButton::Left => Some(BrowserButton::Left),
        MouseButton::Right => Some(BrowserButton::Right),
        MouseButton::Middle => Some(BrowserButton::Middle),
        MouseButton::Navigate(_) => None,
    }
}

impl BrowserPane {
    fn send(&self, input: BrowserInput) {
        self.caller.call(Call::BrowserInput { pane: self.pane, input });
    }

    /// The point of a pointer's place in the page's own pixels, or none in the bands either side of
    /// the picture (and while there is no picture to be on).
    fn point_in_page(&self, at: Point<Pixels>, cx: &gpui_kit::App) -> Option<(f64, f64)> {
        let (width, height) = {
            let heard = self.heard.read(cx);
            let picture = heard.pictures.of(self.pane)?;
            (picture.width, picture.height)
        };
        let area = self.area.get();
        let room = Box2 {
            left: f64::from(area.origin.x),
            top: f64::from(area.origin.y),
            width: f64::from(area.size.width),
            height: f64::from(area.size.height),
        };
        within(room, (width, height), f64::from(at.x), f64::from(at.y)).map(|at| (at.x, at.y))
    }

    pub(super) fn pressed(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let (Some((x, y)), Some(button)) = (self.point_in_page(event.position, cx), button(event.button)) else { return };
        self.pointer = Pointer { at: Some((x, y)), down: Some(button) };
        self.send(BrowserInput::Press { x, y, button, count: event.click_count as u32 });
    }

    /// Lets go of the button the page was pressed with: where it lands when that is on the page,
    /// else where the pointer last was on it.
    pub(super) fn released(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(down) = self.pointer.down.filter(|down| Some(*down) == button(event.button)) else { return };
        let Some((x, y)) = self.point_in_page(event.position, cx).or(self.pointer.at) else { return };
        self.pointer.down = None;
        self.send(BrowserInput::Release { x, y, button: down, count: event.click_count as u32 });
    }

    pub(super) fn moved(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let held = self.pointer.down.filter(|_| event.pressed_button.is_some());
        match self.point_in_page(event.position, cx) {
            Some(at) if self.pointer.at != Some(at) => {
                self.pointer.at = Some(at);
                self.send(BrowserInput::Move { x: at.0, y: at.1, held });
            }
            Some(_) => {}
            // Into a band beside the picture, or past the screen: off the page. A drag keeps its
            // last point for the release.
            None => self.left(),
        }
    }

    /// The pointer went off the page: what it hovered is no longer hovered.
    pub(super) fn left(&mut self) {
        if self.pointer.down.is_none() && self.pointer.at.take().is_some() {
            self.send(BrowserInput::Leave);
        }
    }

    /// The wheel over the screen is the page's, whether or not it is over the picture: the window
    /// does not scroll.
    pub(super) fn wheeled(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let Some((x, y)) = self.point_in_page(event.position, cx) else { return };
        // GPUI's deltas are positive toward the top and left, the page's `deltaX` and `deltaY`
        // positive toward the bottom and right.
        let delta = event.delta.pixel_delta(window.line_height());
        let (dx, dy) = (-f64::from(delta.x), -f64::from(delta.y));
        self.send(BrowserInput::Wheel { x, y, dx, dy });
    }
}
