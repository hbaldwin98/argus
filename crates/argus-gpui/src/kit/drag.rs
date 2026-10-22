//! The one drag helper: a press that becomes a drag after the decided 5 px
//! threshold, a click when it never does, and cancelled by Escape with no
//! drop (`gpui/parity/cross-cutting.md`, Decided 6). Panel docking and the
//! docks' sashes use it; board cards, sequence arrows and pane headers are
//! to use it too, each as a [`Dragger`] of its own payload, so the threshold
//! and the cancel are never written twice.
//!
//! A drag does not own what it moves: it tells its host each [`Step`] and the
//! host decides what the step means, including sending a `Call`. The release
//! that ends a drag is consumed, so a drag that ends over a button does not
//! press it, and the press that never travelled is a [`Step::Clicked`] on
//! what was pressed, delivered on release rather than by the element's own
//! click.
//!
//! The pointer is followed on the window, not on the element pressed, so a
//! drag goes on over every other view. The keys are taken for the drag's
//! duration (the host's `keys` handle) so Escape reaches the host first, and
//! handed back to what had them when it ends.

use gpui_kit::{
    canvas, px, App, Context, DispatchPhase, FocusHandle, IntoElement, KeyDownEvent, MouseButton, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, WeakEntity, Window,
};

/// How far the pointer goes before a press is a drag.
pub const THRESHOLD: Pixels = px(5.0);

/// What a drag tells its host.
#[derive(Debug, Clone, PartialEq)]
pub enum Step<T> {
    /// The pointer went the threshold: the press is a drag now.
    Began(T),
    /// A drag's pointer moved; read where from [`Drag::at`].
    Moved(T),
    /// Let go without ever being a drag: a click on what was pressed.
    Clicked(T),
    /// A drag let go at the point.
    Dropped(T, Point<Pixels>),
    /// Escape, or a release lost, ended a drag: nothing is dropped.
    Cancelled(T),
}

struct Held<T> {
    payload: T,
    origin: Point<Pixels>,
    at: Point<Pixels>,
    dragging: bool,
    /// What had the keys when the drag began, to give them back.
    restore: Option<FocusHandle>,
}

/// The press being followed, if any: a payload naming what was pressed.
pub struct Drag<T> {
    held: Option<Held<T>>,
}

impl<T: Clone> Drag<T> {
    pub fn new() -> Self {
        Self { held: None }
    }

    /// Starts following a press of `payload` at `at`; one already held is replaced.
    pub fn press(&mut self, at: Point<Pixels>, payload: T) {
        self.held = Some(Held { payload, origin: at, at, dragging: false, restore: None });
    }

    /// Whether a press is held, dragged yet or not.
    pub fn pressed(&self) -> bool {
        self.held.is_some()
    }

    /// Whether the press went the threshold and is a drag.
    pub fn is_dragging(&self) -> bool {
        self.held.as_ref().is_some_and(|held| held.dragging)
    }

    pub fn payload(&self) -> Option<&T> {
        self.held.as_ref().map(|held| &held.payload)
    }

    /// Where the pointer last was while held.
    pub fn at(&self) -> Option<Point<Pixels>> {
        self.held.as_ref().map(|held| held.at)
    }

    /// The pointer went to `at`. The first move that reaches the threshold
    /// begins the drag; a press that has not is no drag however it wanders
    /// short of it.
    pub fn moved(&mut self, at: Point<Pixels>) -> Option<Step<T>> {
        let held = self.held.as_mut()?;
        held.at = at;
        if held.dragging {
            return Some(Step::Moved(held.payload.clone()));
        }
        let (dx, dy) = ((at.x - held.origin.x).as_f32(), (at.y - held.origin.y).as_f32());
        if dx.hypot(dy) < THRESHOLD.as_f32() {
            return None;
        }
        held.dragging = true;
        Some(Step::Began(held.payload.clone()))
    }

    /// The button went up at `at`.
    pub fn released(&mut self, at: Point<Pixels>) -> Option<Step<T>> {
        let held = self.held.take()?;
        Some(if held.dragging { Step::Dropped(held.payload, at) } else { Step::Clicked(held.payload) })
    }

    /// Escape, or a release that never arrived: whatever is held is let go
    /// and nothing is dropped or clicked. Only a drag has anything to tell.
    pub fn cancel(&mut self) -> Option<Step<T>> {
        let held = self.held.take()?;
        held.dragging.then(|| Step::Cancelled(held.payload))
    }
}

impl<T: Clone> Default for Drag<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A view that follows a drag: it holds the [`Drag`], names the focus handle
/// the drag takes the keys with, and is told each step.
pub trait Dragger: Sized + 'static {
    type Payload: Clone + 'static;

    fn drag(&self) -> &Drag<Self::Payload>;

    fn drag_mut(&mut self) -> &mut Drag<Self::Payload>;

    /// The handle the drag moves the focus to while it is a drag, so Escape
    /// reaches [`escape`] before whatever had the keys.
    fn keys(&self) -> &FocusHandle;

    fn dragged(&mut self, step: Step<Self::Payload>, window: &mut Window, cx: &mut Context<Self>);
}

/// Hands `step` to the host. The keys are taken as the drag begins, and
/// before the step is told they are given back as it ends, so a step that
/// moves the focus itself has the last word.
fn tell<V: Dragger>(host: &mut V, step: Option<Step<V::Payload>>, restore: Option<FocusHandle>, window: &mut Window, cx: &mut Context<V>) {
    let Some(step) = step else { return };
    match &step {
        Step::Began(_) => {
            let previous = window.focused(cx);
            let keys = host.keys().clone();
            if let Some(held) = host.drag_mut().held.as_mut() {
                held.restore = previous;
            }
            window.focus(&keys, cx);
        }
        Step::Dropped(..) | Step::Cancelled(_) => {
            if let Some(handle) = restore {
                window.focus(&handle, cx);
            }
        }
        Step::Moved(_) | Step::Clicked(_) => {}
    }
    host.dragged(step, window, cx);
    cx.notify();
}

/// What had the keys before the drag took them, taken out so it is given
/// back once.
fn restore<V: Dragger>(host: &mut V) -> Option<FocusHandle> {
    host.drag_mut().held.as_mut().and_then(|held| held.restore.take())
}

/// Cancels the host's drag if Escape is pressed while it is one; the host
/// calls it from the key handler of its `keys` element. The key is consumed
/// so no view under the drag acts on it.
pub fn escape<V: Dragger>(host: &mut V, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<V>) {
    if event.keystroke.key != "escape" || !host.drag().is_dragging() {
        return;
    }
    cx.stop_propagation();
    let back = restore(host);
    let cancelled = host.drag_mut().cancel();
    tell(host, cancelled, back, window, cx);
}

/// An element to put among the host's children that follows the pointer on
/// the whole window while a press is held: moves, the release, and a move
/// that says the button is already up (a release that went elsewhere).
pub fn watch<V: Dragger>(host: WeakEntity<V>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, _, window: &mut Window, _: &mut App| {
            let moves = host.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let left = event.pressed_button == Some(MouseButton::Left);
                moves
                    .update(cx, |host, cx| {
                        if !host.drag().pressed() {
                            return;
                        }
                        if left {
                            let moved = host.drag_mut().moved(event.position);
                            tell(host, moved, None, window, cx);
                        } else {
                            let back = restore(host);
                            let lost = host.drag_mut().cancel();
                            tell(host, lost, back, window, cx);
                        }
                    })
                    .ok();
            });
            let ups = host.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                    return;
                }
                ups.update(cx, |host, cx| {
                    let back = restore(host);
                    let released = host.drag_mut().released(event.position);
                    // The release that ends a drag is the drag's alone: nothing
                    // under the pointer is pressed by it.
                    if matches!(released, Some(Step::Dropped(..) | Step::Cancelled(_))) {
                        cx.stop_propagation();
                    }
                    tell(host, released, back, window, cx);
                })
                .ok();
            });
        },
    )
}

#[cfg(test)]
mod tests;
