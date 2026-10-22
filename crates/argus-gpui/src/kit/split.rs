//! A resizable pair: a column on the left at a width the sash gives it, beside what fills the rest.
//!
//! The Git tabs use it for the files or commits beside the diff, and any stage with a column of
//! its own (the outline, the Code tree) is the same shape. The width follows the sash through the
//! window's one drag helper (`kit::drag`), so it moves only after the 5 px threshold and Escape
//! puts it back to what it was at the press, as the docks' sashes do (CROSS-18).
//!
//! What a split does with its width is the surface's to say. A split that only keeps its width
//! (the Git tabs) sends nothing. One given [`Split::reporting`] sends the width once, on release
//! (SHELL-EDGE-2), and a double-click gives back the usual width and reports `None`; while it
//! is dragged, [`Split::holding`] says so, for the panes that wait on a resize to settle
//! (SHELL-EDGE-1). A tab holds one `Split` entity for as long as it lives, so the width is kept
//! while another tab or a pane is shown. The width drawn after a release is the split's own; a
//! surface whose width is Rust's calls [`Split::follow`] with what Rust says next.

use std::rc::Rc;

use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    div, prelude::*, AnyElement, App, Context, Entity, FocusHandle, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, Role,
    Window,
};

use crate::kit::drag::{self, Drag, Dragger, Step};
use crate::theme::scale::{rem, zoom};
use crate::theme::Tokens;

/// What a press of the sash holds: where the pointer was, and how wide the
/// column was, so the column does not jump to put its edge under the pointer. The pointer is the
/// window's pixels and the width the page's, so the zoom of the press is held too.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    from: f32,
    start: f32,
    zoom: f32,
    /// A column on the right, whose edge is on its left: moving the pointer left widens it.
    mirror: bool,
}

/// Where a release is reported: the width in whole pixels, or `None` for the usual one.
type Report = Rc<dyn Fn(Option<u32>, &mut Window, &mut App)>;

/// Where a drag beginning and ending is said.
type Hold = Rc<dyn Fn(bool, &mut App)>;

pub struct Split {
    id: &'static str,
    label: &'static str,
    /// The width when nothing has been dragged.
    usual: f32,
    width: f32,
    min: f32,
    max: f32,
    drag: Drag<Held>,
    /// The handle the drag takes the keys with, so Escape reaches it.
    keys: FocusHandle,
    report: Option<Report>,
    /// Whether the column sits at the right of what it is beside, its sash on its left edge.
    mirror: bool,
    hold: Option<Hold>,
}

/// The width with the pointer at `x`, within the bounds the column is kept to.
fn width_at(held: Held, x: f32, min: f32, max: f32) -> f32 {
    let travel = (x - held.from) / held.zoom;
    (if held.mirror { held.start - travel } else { held.start + travel }).clamp(min, max)
}

impl Split {
    pub fn new(id: &'static str, label: &'static str, open: f32, min: f32, max: f32, cx: &mut Context<Self>) -> Self {
        Self { id, label, usual: open, width: open, min, max, drag: Drag::new(), keys: cx.focus_handle(), report: None, mirror: false, hold: None }
    }

    /// Reports the width on release, and `None` on a double-click, which also gives the usual
    /// width back.
    pub fn reporting(mut self, report: impl Fn(Option<u32>, &mut Window, &mut App) + 'static) -> Self {
        self.report = Some(Rc::new(report));
        self
    }

    /// Says when a drag begins and ends, so the panes beside can wait for the size to settle.
    pub fn holding(mut self, hold: impl Fn(bool, &mut App) + 'static) -> Self {
        self.hold = Some(Rc::new(hold));
        self
    }

    /// The column is at the right: its sash is on its left edge and dragging it left widens it.
    pub fn mirrored(mut self) -> Self {
        self.mirror = true;
        self
    }

    /// Whether the sash is held now: the width drawn is the pointer's, not the page's.
    pub fn dragging(&self) -> bool {
        self.drag.is_dragging()
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    /// Rust says the width is `width`, as it does after a release: drawn unless the sash is held,
    /// held to the bounds.
    pub fn follow(&mut self, width: f32, cx: &mut Context<Self>) {
        let width = width.clamp(self.min, self.max);
        if !self.drag.is_dragging() && width != self.width {
            self.width = width;
            cx.notify();
        }
    }

    fn holding_now(&self, holding: bool, cx: &mut App) {
        if let Some(hold) = &self.hold {
            hold(holding, cx);
        }
    }

    fn reported(&self, width: Option<u32>, window: &mut Window, cx: &mut App) {
        if let Some(report) = &self.report {
            report(width, window, cx);
        }
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

    fn dragged(&mut self, step: Step<Held>, window: &mut Window, cx: &mut Context<Self>) {
        match step {
            Step::Began(held) => {
                self.holding_now(true, cx);
                if let Some(at) = self.drag.at() {
                    self.width = width_at(held, at.x.as_f32(), self.min, self.max);
                }
            }
            Step::Moved(held) => {
                if let Some(at) = self.drag.at() {
                    self.width = width_at(held, at.x.as_f32(), self.min, self.max);
                }
            }
            Step::Dropped(held, at) => {
                self.width = width_at(held, at.x.as_f32(), self.min, self.max);
                self.holding_now(false, cx);
                self.reported(Some(self.width.round() as u32), window, cx);
            }
            Step::Cancelled(held) => {
                self.width = held.start;
                self.holding_now(false, cx);
            }
            Step::Clicked(_) => {}
        }
    }
}

impl Render for Split {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let accent = tokens.accent;
        div()
            .id(self.id)
            .role(Role::Splitter)
            .test_support()
            .aria_label(self.label)
            .size_full()
            .cursor_col_resize()
            .when(self.drag.is_dragging(), |zone| zone.bg(accent))
            .hover(move |style| style.bg(accent.opacity(tokens.share_sash)))
            // The press is the stage's to take the keys with, as any press on it is:
            // the handle the drag takes them with is on a child that nothing can press.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if event.click_count >= 2 && this.report.is_some() {
                        // A double-click gives back the usual width, and says so.
                        this.width = this.usual;
                        this.reported(None, window, cx);
                        cx.notify();
                        return;
                    }
                    let held = Held { from: event.position.x.as_f32(), start: this.width, zoom: zoom(window.rem_size()), mirror: this.mirror };
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
pub fn side(split: &Entity<Split>, id: &'static str, content: impl IntoElement, tokens: &Tokens, cx: &App) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .relative()
        .flex_shrink_0()
        .h_full()
        .w(rem(split.read(cx).width()))
        .border_r_1()
        .border_color(tokens.line)
        .child(content)
        // The grab zone along the column's right edge, as wide as a dock's.
        .child(div().absolute().top_0().bottom_0().right_0().w(rem(tokens.sp_6)).child(split.clone()))
        .into_any_element()
}

#[cfg(test)]
mod tests;
