//! A flow's sequence diagram, drawn and picked (BOARD-SEQ-3, -5, -6, -13 to -15): lifelines left to right,
//! calls solid, replies dashed, the lit step in the accent, and an arrow a click puts the cursor on.
//!
//! The geometry is core's (`board::lenses::picture`) and the painting `paint`'s. Inline in the Flows lens
//! the diagram is read-only apart from a click; expanded over the window an arrow dragged up or down, or
//! a lifeline's head dragged sideways, is sent as the flow rewritten, and the diagram moves when the
//! pushed flows say it did: a drag only shows where it would land. The press is followed by the window's
//! one drag helper (`kit::drag`), so the threshold is 5 px, a press that never travels is a click on what
//! it was made on, and Escape puts a drag back with nothing sent.

pub mod expanded;
pub mod paint;
pub mod panel;
pub mod toolbar;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::board::lenses::picture::{nearest_lifeline, nearest_step, Picture, HEAD, LOOP_DROP, LOOP_OUT, MARGIN};
use argus_desktop_core::connection::page::{Call, FeatureEdit};
use gpui_kit::{canvas, div, prelude::*, Bounds, Context, ElementId, Entity, FocusHandle, KeyDownEvent, Pixels, Point, Role, SharedString, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::area;
use crate::kit::drag::{self, Drag, Dragger, Step};
use crate::kit::ink::Frame;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use paint::Guide;

/// What is pressed on the diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// A step's arrow, by its number.
    Step(usize),
    /// A lifeline's head, by its place from the left.
    Lifeline(usize),
}

/// What the diagram is showing: the flow, laid out, at a scale.
#[derive(Clone, PartialEq)]
pub struct Shown {
    pub picture: Rc<Picture>,
    pub diagram: i64,
    pub title: String,
    /// What the source calls each lifeline, which a drag names.
    pub names: Vec<String>,
    /// How big it is drawn, one being the diagram's own units as pixels of the page.
    pub scale: f32,
}

pub struct SequenceView {
    id: &'static str,
    heard: Entity<Heard>,
    caller: Caller,
    shown: Option<Shown>,
    /// Whether an arrow or a head can be dragged: only in the expanded view.
    draggable: bool,
    drag: Drag<Press>,
    keys: FocusHandle,
    hovered: Option<usize>,
    /// Where the diagram was laid out, to turn the pointer into the diagram's units.
    area: Rc<Cell<Bounds<Pixels>>>,
    /// Where the lit arrow's box was laid out, which the bind menu opens under.
    lit_arrow: Rc<Cell<Bounds<Pixels>>>,
}

impl SequenceView {
    pub fn new(id: &'static str, draggable: bool, heard: Entity<Heard>, caller: Caller, cx: &mut Context<Self>) -> Self {
        Self { id, heard, caller, shown: None, draggable, drag: Drag::new(), keys: cx.focus_handle(), hovered: None, area: Rc::default(), lit_arrow: Rc::default() }
    }

    /// Puts a drag under way back, sending nothing (Escape); whether there was one.
    pub fn cancel_drag(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.drag.is_dragging() {
            return false;
        }
        self.drag.cancel();
        cx.notify();
        true
    }

    /// The box of the lit step's arrow as last laid out.
    pub fn lit_arrow(&self) -> Bounds<Pixels> {
        self.lit_arrow.get()
    }

    /// Shows `shown`; a redraw only when it says something else than it did.
    pub fn show(&mut self, shown: Option<Shown>, cx: &mut Context<Self>) {
        if self.shown != shown {
            self.shown = shown;
            cx.notify();
        }
    }

    fn send(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// Where a drag of `press` would land with the pointer where it is: the step or lifeline nearest it.
    fn target(&self, press: Press, at: Point<Pixels>) -> Option<(usize, Guide)> {
        let shown = self.shown.as_ref()?;
        let area = self.area.get();
        let unit = f32::from(area.size.width) / shown.picture.width.max(1.0);
        if unit <= 0.0 {
            return None;
        }
        let (x, y) = (f32::from(at.x - area.origin.x) / unit, f32::from(at.y - area.origin.y) / unit);
        match press {
            Press::Step(_) => {
                let step = nearest_step(&shown.picture, y)?;
                let arrow = shown.picture.arrows.iter().find(|arrow| arrow.step == step)?;
                Some((step, Guide::Step(arrow.y)))
            }
            Press::Lifeline(_) => {
                let at = nearest_lifeline(&shown.picture, x)?;
                Some((at, Guide::Lifeline(shown.picture.heads[at].x)))
            }
        }
    }

    /// The drop line of the drag now, if one is under way.
    fn guide(&self) -> Option<Guide> {
        let press = *self.drag.payload()?;
        let at = self.drag.at()?;
        self.drag.is_dragging().then(|| self.target(press, at)).flatten().map(|(_, guide)| guide)
    }
}

impl Dragger for SequenceView {
    type Payload = Press;

    fn drag(&self) -> &Drag<Press> {
        &self.drag
    }

    fn drag_mut(&mut self) -> &mut Drag<Press> {
        &mut self.drag
    }

    fn keys(&self) -> &FocusHandle {
        &self.keys
    }

    fn dragged(&mut self, step: Step<Press>, _: &mut Window, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.clone() else { return };
        match step {
            Step::Clicked(Press::Step(number)) => self.send(Call::SelectStep { diagram: shown.diagram, step: Some(number - 1) }, cx),
            Step::Clicked(Press::Lifeline(_)) | Step::Began(_) | Step::Moved(_) | Step::Cancelled(_) => {}
            Step::Dropped(press, at) if self.draggable => {
                let Some((landing, _)) = self.target(press, at) else { return };
                match press {
                    Press::Step(number) if landing != number => {
                        // The cursor stays on the step: it is lit at its new place as soon as the pushed flow puts it there.
                        self.send(Call::EditFeature { edit: FeatureEdit::MoveStep { diagram: shown.diagram, step: number, to: landing } }, cx);
                        self.send(Call::SelectStep { diagram: shown.diagram, step: Some(landing - 1) }, cx);
                    }
                    Press::Lifeline(at) if landing != at => {
                        if let Some(name) = shown.names.get(at) {
                            self.send(Call::EditFeature { edit: FeatureEdit::MoveParticipant { diagram: shown.diagram, name: name.clone(), to: landing } }, cx);
                        }
                    }
                    _ => {}
                }
            }
            Step::Dropped(..) => {}
        }
    }
}

impl Render for SequenceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let root = div().id(self.id).test_support().relative().role(Role::Image);
        let Some(shown) = self.shown.clone() else { return root };
        let picture = shown.picture.clone();
        let (hovered, guide) = (self.hovered, self.guide());
        let scale = shown.scale;
        let weak = cx.weak_entity();
        let paint = canvas(
            |_, _, _| (),
            {
                let picture = picture.clone();
                move |area: Bounds<Pixels>, _, window: &mut Window, cx: &mut gpui_kit::App| {
                    let unit = f32::from(area.size.width) / picture.width.max(1.0);
                    let frame = Frame::new(area.origin, unit, window.rem_size());
                    paint::paint(window, cx, &frame, &picture, hovered, guide, &tokens);
                }
            },
        )
        .absolute()
        .size_full();
        let hit = |id: ElementId, label: SharedString, x: f32, y: f32, w: f32, h: f32| {
            div().id(id).test_support().aria_label(label).absolute().left(rem(x * scale)).top(rem(y * scale)).w(rem(w * scale)).h(rem(h * scale))
        };
        let arrows = picture.arrows.iter().map(|arrow| {
            let number = arrow.step;
            let (x, y, w, h) = match arrow.line {
                argus_desktop_core::board::lenses::picture::Line::Across { from, to, y } => (from.min(to), y - 22.0, (to - from).abs(), 34.0),
                argus_desktop_core::board::lenses::picture::Line::Loop { x, y } => (x - 10.0, y - 4.0, LOOP_OUT + 48.0 + arrow.text.chars().count() as f32 * 7.2 + 8.0, LOOP_DROP + 10.0),
            };
            let (hovering, pressing, lit) = (weak.clone(), weak.clone(), self.lit_arrow.clone());
            hit(ElementId::NamedInteger(SharedString::from(format!("{}-step", self.id)), number as u64), SharedString::from(arrow.label.clone()), x, y, w, h)
                .role(Role::Button)
                .cursor_pointer()
                .when(arrow.on, |hit| hit.child(area::measure(lit)))
                .on_hover(move |hovered, _, cx| {
                    hovering
                        .update(cx, |this, cx| {
                            let now = hovered.then_some(number);
                            if this.hovered != now && (now.is_some() || this.hovered == Some(number)) {
                                this.hovered = now;
                                cx.notify();
                            }
                        })
                        .ok();
                })
                .on_mouse_down(gpui_kit::MouseButton::Left, move |event, _, cx| {
                    pressing.update(cx, |this, _| this.drag.press(event.position, Press::Step(number))).ok();
                })
        });
        let heads = self.draggable.then(|| {
            picture.heads.iter().enumerate().map(|(at, head)| {
                let pressing = weak.clone();
                let width = argus_desktop_core::board::lenses::picture::head_width();
                hit(ElementId::NamedInteger(SharedString::from(format!("{}-head", self.id)), at as u64), SharedString::from(head.label.clone()), head.x - width / 2.0, MARGIN, width, HEAD)
                    .cursor_ew_resize()
                    .on_mouse_down(gpui_kit::MouseButton::Left, move |event, _, cx| {
                        pressing.update(cx, |this, _| this.drag.press(event.position, Press::Lifeline(at))).ok();
                    })
            })
        });
        root.w(rem(picture.width * scale))
            .h(rem(picture.height * scale))
            .aria_label(SharedString::from(format!("Sequence diagram: {}", shown.title)))
            .child(area::measure(self.area.clone()))
            .child(paint)
            .children(arrows)
            .children(heads.into_iter().flatten())
            .child(div().id((self.id, 1usize)).size_0().track_focus(&self.keys).on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| drag::escape(this, event, window, cx))))
            .child(drag::watch(cx.weak_entity()))
    }
}

#[cfg(test)]
mod tests;
