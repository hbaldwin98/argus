//! The work area under the header: the stage in the middle, a dock on its
//! left, right and bottom, and the one drag that moves a panel or sizes a dock.
//!
//! The arrangement is the page's (`index.html`): the left and right docks run the
//! work area's full height, the bottom dock sits under the stage between them.
//! What each dock holds, shows and measures is Rust's (`DocksPage`, drawn by
//! `layout/dock`); this owns what only a gesture knows: the pointer, the size a
//! held sash draws, and the preview under a panel being carried. A gesture
//! sends its `Call` when it ends and changes nothing else: the dock is as big,
//! and the panel where it was, until the next `Docks` event says otherwise, and
//! Escape ends a drag with no call at all.
//!
//! Panels are dragged by a tab or a single panel's head, to a dock or the
//! stage (`layout/drop_target`), through the window's one drag helper
//! (`pointer::drag`) so the 5 px threshold and the cancel are not written here.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{Dock, DockGesture, Panel};
use argus_desktop_core::panes::console::ConsolePlace;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    anchored, canvas, deferred, div, point, prelude::*, px, AnyView, Bounds, Context, Entity, FocusHandle, KeyDownEvent,
    MouseButton, Pixels, Point, SharedString, Subscription, Window,
};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::layout::dock::{self, DockView};
use crate::layout::drop_target::{self, Target};
use crate::layout::sash;
use crate::pointer::drag::{self, Drag, Dragger, Step};
use crate::theme::Ledger;

/// What is being pressed.
#[derive(Debug, Clone, PartialEq)]
pub enum Held {
    /// A dock tab or a single panel's head: carried to a dock or the stage.
    /// A press that never travels is a click only on a tab, which shows it.
    Panel { panel: Panel, title: String, stage: bool, click: bool },
    /// A dock's sash, grabbed `grab` pixels from the dock's edge.
    Sash { dock: Dock, grab: f32 },
}

pub struct WorkView {
    heard: Entity<Heard>,
    caller: Caller,
    stage: AnyView,
    /// The console: in its dock, or the stage while it is the Console stage or fullscreen.
    console: AnyView,
    left: Entity<DockView>,
    right: Entity<DockView>,
    bottom: Entity<DockView>,
    drag: Drag<Held>,
    /// The handle the drag takes the keys with, so Escape reaches it.
    keys: FocusHandle,
    /// The areas as last laid out, for where a pointer is within them.
    work: Rc<Cell<Bounds<Pixels>>>,
    center: Rc<Cell<Bounds<Pixels>>>,
    /// The size a held sash draws, until it is let go.
    sizing: Option<(Dock, u32)>,
    _observing: Subscription,
}

impl WorkView {
    pub fn new(heard: Entity<Heard>, caller: Caller, stage: AnyView, console: AnyView, cx: &mut Context<Self>) -> Self {
        let work = cx.weak_entity();
        let dock = |dock, cx: &mut Context<Self>| {
            let (heard, caller, work, console) = (heard.clone(), caller.clone(), work.clone(), console.clone());
            cx.new(|cx| DockView::new(dock, heard, caller, work, console, cx))
        };
        let (left, right, bottom) = (dock(Dock::Left, cx), dock(Dock::Right, cx), dock(Dock::Bottom, cx));
        let observing = cx.observe(&heard, |_, _, cx| cx.notify());
        Self {
            heard,
            caller,
            stage,
            console,
            left,
            right,
            bottom,
            drag: Drag::new(),
            keys: cx.focus_handle(),
            work: Rc::default(),
            center: Rc::default(),
            sizing: None,
            _observing: observing,
        }
    }

    /// The size the sash of `dock` draws while it is held.
    pub fn sizing(&self, dock: Dock) -> Option<u32> {
        self.sizing.filter(|(held, _)| *held == dock).map(|(_, size)| size)
    }

    /// Follows a press of a tab or head, unless the daemon is gone and a
    /// press would reach no one.
    pub fn press(&mut self, at: Point<Pixels>, held: Held, cx: &mut Context<Self>) {
        if !self.heard.read(cx).held() {
            self.drag.press(at, held);
        }
    }

    /// Follows a press of `dock`'s sash, `drawn` pixels big now.
    pub fn press_sash(&mut self, dock: Dock, drawn: f32, at: Point<Pixels>, cx: &mut Context<Self>) {
        let grab = sash::grab(dock, drawn, at, self.work.get(), self.center.get());
        self.press(at, Held::Sash { dock, grab }, cx);
    }

    fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// Where a panel dropped at `at` would land.
    fn target(&self, at: Point<Pixels>, stage: bool) -> Option<Target> {
        drop_target::target_at(at, self.work.get(), self.center.get(), stage)
    }

    fn follow(&mut self, held: &Held) {
        if let (Held::Sash { dock, grab }, Some(at)) = (held, self.drag.at()) {
            self.sizing = Some((*dock, sash::held(*dock, *grab, at, self.work.get(), self.center.get())));
        }
    }

    /// The area a panel dropped here would fill, said in words, and the
    /// panel's name under the pointer.
    fn carrying(&self, ledger: &Ledger) -> Option<impl IntoElement> {
        let Some(Held::Panel { title, stage, .. }) = self.drag.payload() else { return None };
        if !self.drag.is_dragging() {
            return None;
        }
        let at = self.drag.at()?;
        let (work, center) = (self.work.get(), self.center.get());
        let preview = self.target(at, *stage).map(|target| {
            let land = drop_target::landing(target, work, center);
            let words = SharedString::from(drop_target::words(target));
            div()
                .id("drop-preview")
                .test_support()
                .aria_label(words.clone())
                .absolute()
                .left(land.origin.x - work.origin.x)
                .top(land.origin.y - work.origin.y)
                .w(land.size.width)
                .h(land.size.height)
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .border_2()
                .border_color(ledger.accent)
                .bg(ledger.accent.opacity(0.14))
                .child(div().px_2().py_1().rounded(px(4.0)).bg(ledger.raised).text_sm().text_color(ledger.foreground).child(words))
        });
        let ghost = deferred(
            anchored().position(at + point(px(12.0), px(12.0))).snap_to_window().child(
                div()
                    .id("drag-ghost")
                    .test_support()
                    .aria_label(SharedString::from(title.clone()))
                    .px_2()
                    .py_1()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(ledger.line_strong)
                    .bg(ledger.raised)
                    .text_sm()
                    .text_color(ledger.foreground)
                    .child(SharedString::from(title.clone())),
            ),
        );
        // Over the whole area, so the preview's own offsets are from its corner.
        Some(div().absolute().top_0().left_0().size_full().children(preview).child(ghost))
    }
}

impl Dragger for WorkView {
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

    fn dragged(&mut self, step: Step<Held>, _: &mut Window, cx: &mut Context<Self>) {
        match step {
            Step::Began(held) | Step::Moved(held) => self.follow(&held),
            Step::Clicked(Held::Panel { panel, click: true, .. }) => {
                self.ask(Call::Docks { gesture: DockGesture::Show { panel } }, cx)
            }
            Step::Clicked(_) => {}
            Step::Dropped(Held::Panel { panel, stage, .. }, at) => {
                if let Some(target) = self.target(at, stage) {
                    self.ask(drop_target::call(target, panel), cx);
                }
            }
            Step::Dropped(Held::Sash { dock, grab }, at) => {
                // What was drawn is let go: the dock is Rust's size again,
                // until the `Docks` event that answers this call.
                self.sizing = None;
                let size = sash::held(dock, grab, at, self.work.get(), self.center.get());
                self.ask(Call::Docks { gesture: DockGesture::Size { dock, size } }, cx);
            }
            Step::Cancelled(_) => self.sizing = None,
        }
        // The page marks the body `resizing` while a sash is held, so a pane
        // waits for the size the gesture settles on (SHELL-EDGE-1).
        let sizing = self.sizing.is_some();
        if self.heard.read(cx).resizing != sizing {
            self.heard.update(cx, |heard, cx| {
                heard.resizing = sizing;
                cx.notify();
            });
        }
    }
}

/// Records the area its parent was laid out in.
fn measure(into: Rc<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    canvas(move |bounds, _, _| into.set(bounds), |_, _, _, _| {}).absolute().size_full()
}

impl Render for WorkView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        // The Console stage and fullscreen give the console the stage: the
        // Workspace's pane has none then (`ConsolePage::showing`).
        let on_stage = matches!(self.heard.read(cx).console.place, ConsolePlace::Tab | ConsolePlace::Fullscreen);
        let stage = if on_stage { self.console.clone() } else { self.stage.clone() };
        h_flex()
            .id("work")
            .key_context("Work")
            .track_focus(&self.keys)
            // GPUI hands a tracked element the keys on any press inside it,
            // which would take them from the pane; the drag takes them itself,
            // when it begins, and gives them back.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| drag::escape(this, event, window, cx)))
            .relative()
            .size_full()
            .min_h_0()
            .child(measure(self.work.clone()))
            .child(drag::watch(cx.weak_entity()))
            .child(self.left.clone())
            .child(
                v_flex()
                    .id("center")
                    .relative()
                    .flex_1()
                    .min_w(px(dock::FLOOR_WIDTH))
                    .h_full()
                    .child(measure(self.center.clone()))
                    .child(div().id("work-stage").test_support().flex_1().min_h(px(dock::FLOOR_HEIGHT)).child(stage))
                    .child(self.bottom.clone()),
            )
            .child(self.right.clone())
            .children(self.carrying(&ledger))
    }
}

#[cfg(test)]
mod tests;
