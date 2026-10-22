//! The larger view's state: where the drawing sits in its stage, whether its source is beside it,
//! and the drag that moves it (FIGURE-2 to FIGURE-6).
//!
//! The drawing's place is core's (`views::blocks::zoom`); this holds it and changes it by the four
//! things a person does (a wheel turn, a step, a drag, Fit). It is unset until the stage has been
//! measured, since how it opens depends on how big the stage is. A drag moves the drawing after
//! the decided 5 px, and Escape, which closes the view, is how it is let go of.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::views::blocks::zoom::{actual, fit, start, wheel_factor, zoom_at, Unit, Zoom, STEP};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, Bounds, Context, FocusHandle, Pixels, Point, Role, ScrollDelta, SharedString, Window};

use super::{bar, stage, Figure};
use crate::kit::drag::{Drag, Dragger, Step};
use crate::theme::scale::{css, pixels};
use crate::theme::Tokens;

/// What the keys and buttons do to the drawing's place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// One step larger about the stage's centre.
    In,
    Out,
    /// 100%, about the centre.
    Actual,
    /// The whole drawing in the stage.
    Fit,
}

/// A press being dragged: where it began and where the drawing was then.
#[derive(Debug, Clone, Copy)]
pub struct Held {
    from: Point<Pixels>,
    at: Zoom,
}

pub struct Larger {
    pub(super) figure: Rc<Figure>,
    pub(super) zoom: Option<Zoom>,
    pub(super) source: bool,
    /// Where the stage was laid out last, which the drawing's place is measured in.
    pub(super) stage: Rc<Cell<Bounds<Pixels>>>,
    drag: Drag<Held>,
    /// Whether the person has zoomed or dragged; until then the drawing is placed afresh each time the
    /// stage changes size, as an opening dialog's stage does while it settles.
    touched: bool,
    keys: gpui_kit::FocusHandle,
    pub(super) dragging: bool,
    pub(super) source_scroll: gpui_kit::ScrollHandle,
}

impl Larger {
    pub fn new(figure: Rc<Figure>, cx: &mut Context<Self>) -> Self {
        Self { figure, zoom: None, source: false, stage: Rc::default(), drag: Drag::new(), touched: false, keys: cx.focus_handle(), dragging: false, source_scroll: gpui_kit::ScrollHandle::new() }
    }

    /// The stage's size in the page's pixels.
    fn stage_size(&self, window: &Window) -> (f32, f32) {
        let size = self.stage.get().size;
        (css(size.width, window.rem_size()), css(size.height, window.rem_size()))
    }

    /// The stage has been laid out: a drawing not yet placed opens as `start` says.
    pub(super) fn measured(&mut self, window: &Window, cx: &mut Context<Self>) {
        let (sw, sh) = self.stage_size(window);
        if !self.touched && sw > 0.0 && sh > 0.0 {
            let (w, h) = self.figure.size;
            self.zoom = Some(start(w, h, sw, sh));
        }
        cx.notify();
    }

    /// A point of the window as a point of the stage, in the page's pixels.
    fn at_stage(&self, at: Point<Pixels>, window: &Window) -> (f32, f32) {
        let origin = self.stage.get().origin;
        (css(at.x - origin.x, window.rem_size()), css(at.y - origin.y, window.rem_size()))
    }

    /// A step, 100% or Fit, about the stage's centre.
    pub fn apply(&mut self, by: Move, window: &Window, cx: &mut Context<Self>) {
        let (sw, sh) = self.stage_size(window);
        let (w, h) = self.figure.size;
        let Some(zoom) = self.zoom else { return };
        self.touched = true;
        self.zoom = Some(match by {
            Move::In => zoom_at(zoom, STEP, sw / 2.0, sh / 2.0),
            Move::Out => zoom_at(zoom, 1.0 / STEP, sw / 2.0, sh / 2.0),
            Move::Actual => actual(zoom, sw / 2.0, sh / 2.0),
            Move::Fit => fit(w, h, sw, sh),
        });
        cx.notify();
    }

    /// A wheel turn zooms about the pointer, which the drawing keeps under it. A GPUI wheel's delta
    /// is positive away from the person, the web's toward them.
    pub(super) fn wheel(&mut self, delta: ScrollDelta, at: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let Some(zoom) = self.zoom else { return };
        self.touched = true;
        let factor = match delta {
            ScrollDelta::Pixels(p) => wheel_factor(-css(p.y, window.rem_size()), Unit::Pixels),
            ScrollDelta::Lines(l) => wheel_factor(-l.y, Unit::Lines),
        };
        let (px, py) = self.at_stage(at, window);
        self.zoom = Some(zoom_at(zoom, factor, px, py));
        cx.notify();
    }

    /// The left button went down on the stage.
    pub(super) fn press(&mut self, at: Point<Pixels>) {
        if let Some(zoom) = self.zoom {
            self.drag.press(at, Held { from: at, at: zoom });
        }
    }

    /// Shows or hides the source beside the drawing.
    pub(super) fn toggle_source(&mut self, cx: &mut Context<Self>) {
        self.source = !self.source;
        cx.notify();
    }

    pub fn is_source_shown(&self) -> bool {
        self.source
    }

    pub fn zoom(&self) -> Option<Zoom> {
        self.zoom
    }
}

impl Dragger for Larger {
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

    fn dragged(&mut self, step: Step<Held>, window: &mut Window, _: &mut Context<Self>) {
        match step {
            Step::Began(_) => {
                self.dragging = true;
                self.touched = true;
            }
            Step::Moved(held) => {
                if let Some(now) = self.drag.at() {
                    let rem_size = window.rem_size();
                    let (dx, dy) = (css(now.x - held.from.x, rem_size), css(now.y - held.from.y, rem_size));
                    self.zoom = Some(Zoom { x: held.at.x + dx, y: held.at.y + dy, ..held.at });
                }
            }
            Step::Dropped(..) => self.dragging = false,
            Step::Cancelled(held) => {
                self.dragging = false;
                self.zoom = Some(held.at);
            }
            Step::Clicked(_) => {}
        }
    }
}

impl Render for Larger {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        // The window less the 24 px the view keeps clear all round.
        let height = window.viewport_size().height - pixels(tokens.sp_24, window.rem_size()) * 2.0;
        v_flex()
            .id("figure")
            .role(Role::Dialog)
            .test_support()
            .aria_label(SharedString::from(self.figure.title.clone()))
            .w_full()
            .h(height)
            .overflow_hidden()
            .child(bar::bar(self, &tokens, cx))
            .child(h_flex().flex_1().min_h_0().child(stage::stage(self, window, &tokens, cx)).children(stage::source(self, &tokens)))
            .child(
                // What a drag takes the keys with.
                gpui_kit::div().id("figure-keys").test_support().size_0().track_focus(&self.keys),
            )
    }
}
