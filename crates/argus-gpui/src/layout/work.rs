//! The work area under the header: the stage in the middle, a dock on its
//! left, right and bottom, and the one drag that moves a panel or a pane or sizes a dock.
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
//! (`kit::drag`) so the 5 px threshold and the cancel are not written here. A pane is carried by
//! the same drag, from a cell's header or a rail row, to a cell, the stage or a checkout
//! (`layout/carry`); the places it can land are published by the views that draw them (`kit/spots`).

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{Dock, DockGesture, Panel};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    div, prelude::*, px, AnyView, App, Bounds, Context, Entity, FocusHandle, KeyDownEvent,
    MouseButton, Pixels, Point, SharedString, Subscription, Window,
};

use crate::connection::caller::Caller;
use crate::kit::area;
use crate::chrome::stage::route::{frame, surface, Surface};
use crate::connection::heard::Heard;
use crate::kit::spots::Spots;
use crate::layout::carry::landing::{call as carried_call, landing, Grid, Landing};
use crate::layout::carry::preview;
use crate::layout::carry::{Carrier, Carried};
use crate::layout::dock::{self, DockView};
use crate::layout::drop_target::{self, Target};
use crate::layout::holders::Holder;
use crate::layout::sash;
use crate::kit::drag::{self, Drag, Dragger, Step};
use crate::theme::Tokens;

/// What is being pressed.
#[derive(Debug, Clone, PartialEq)]
pub enum Held {
    /// A dock tab or a single panel's head: carried to a dock or the stage.
    /// A press that never travels is a click only on a tab, which shows it.
    Panel { panel: Panel, title: String, stage: bool, click: bool },
    /// A dock's sash, grabbed `grab` pixels from the dock's edge.
    Sash { dock: Dock, grab: f32 },
    /// A pane carried by a cell's header or from the rail, to a cell, the stage or a checkout
    /// (`layout/carry`). A press that never travels is the row's own click.
    Pane(Carried),
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
        // The cell headers and the rail's rows press panes into this drag.
        Carrier::of(cx).serve(cx.weak_entity());
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

    fn follow(&mut self, held: &Held, cx: &mut Context<Self>) {
        if let (Held::Sash { dock, grab }, Some(at)) = (held, self.drag.at()) {
            self.sizing = Some((*dock, sash::held(*dock, *grab, at, self.work.get(), self.center.get())));
        }
        if let Held::Pane(carried) = held {
            self.carry(Some(carried), cx);
        }
    }

    /// Where a pane carried to the pointer would land.
    fn landed(&self, carried: &Carried, at: Point<Pixels>, cx: &App) -> Landing {
        let heard = self.heard.read(cx);
        let panes = heard.cell_panes();
        landing(carried, at, Grid { up: heard.grid_up(), panes: &panes }, &Spots::peek(cx))
    }

    /// Says what is carried, or that nothing is, to the views that draw by it: the grid keeps its
    /// layout while a pane is carried, and the rail lights the checkout it would move to.
    fn carry(&mut self, carried: Option<&Carried>, cx: &mut Context<Self>) {
        let spots = Spots::of(cx);
        let lit = match (carried, self.drag.at()) {
            (Some(carried), Some(at)) => match self.landed(carried, at, cx) {
                Landing::Checkout(checkout) => Some(checkout),
                _ => None,
            },
            _ => None,
        };
        // The rail is not this view's child, so a window redraw is what tells it.
        if spots.carry(carried.is_some()) | spots.light(lit) {
            cx.defer(|cx| cx.refresh_windows());
        }
    }

    /// The area a panel or a pane dropped here would fill, said in words, and the name of what is
    /// carried under the pointer.
    fn carrying(&self, tokens: &Tokens, cx: &App) -> Option<impl IntoElement> {
        let held = self.drag.payload()?;
        if !self.drag.is_dragging() {
            return None;
        }
        let at = self.drag.at()?;
        let (work, center) = (self.work.get(), self.center.get());
        let (title, lit) = match held {
            Held::Panel { title, stage, .. } => {
                let lit = self.target(at, *stage).map(|target| {
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
                        .rounded(px(tokens.r_sm))
                        .border_2()
                        .border_color(tokens.accent)
                        .bg(tokens.accent.opacity(tokens.share_landing))
                        .child(div().px_2().py_1().rounded(px(tokens.r_sm)).bg(tokens.bg_surface).text_sm().text_color(tokens.fg).child(words))
                        .into_any_element()
                });
                (title.as_str(), lit)
            }
            Held::Pane(carried) => {
                let landed = self.landed(carried, at, cx);
                (carried.title(), preview::lit(landed, work.origin, &Spots::peek(cx), tokens))
            }
            Held::Sash { .. } => return None,
        };
        // Over the whole area, so the preview's own offsets are from its corner.
        Some(div().absolute().top_0().left_0().size_full().children(lit).child(preview::ghost(title, at, tokens)))
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
            Step::Began(held) | Step::Moved(held) => self.follow(&held, cx),
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
            Step::Dropped(Held::Pane(carried), at) => {
                self.carry(None, cx);
                let landed = self.landed(&carried, at, cx);
                // Nothing moves here: the grid is what the session says next, and a refusal is a notice.
                if let Some(call) = carried_call(&carried, landed) {
                    self.ask(call, cx);
                }
            }
            Step::Cancelled(held) => {
                self.sizing = None;
                if matches!(held, Held::Pane(_)) {
                    self.carry(None, cx);
                }
            }
        }
        // The page marks the body `resizing` while a sash is held, so a pane
        // waits for the size the gesture settles on (SHELL-EDGE-1).
        // Each dock holds under its own name, and only these are this area's
        // to lift: another drag's hold stays.
        let sizing = self.sizing.map(|(dock, _)| Holder::Dock(dock));
        self.heard.update(cx, |heard, cx| {
            let mut changed = false;
            for dock in [Dock::Left, Dock::Right, Dock::Bottom] {
                let holder = Holder::Dock(dock);
                changed |= if sizing == Some(holder) { heard.hold_resize(holder) } else { heard.release_resize(holder) };
            }
            if changed {
                cx.notify();
            }
        });
    }
}

impl Render for WorkView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        // The Console stage and fullscreen give the console the stage: the Workspace's pane has
        // none then (`ConsolePage::showing`). Which one fills it is the router's to say, and a
        // fullscreen console puts the docks away (`chrome/stage/route`).
        let (surface, furniture) = {
            let heard = self.heard.read(cx);
            (surface(&heard.rail, heard.console.place, heard.git_up()), frame(&heard.rail, heard.console.place).furniture)
        };
        let stage = if surface == Surface::Console { self.console.clone() } else { self.stage.clone() };
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
            .child(area::measure(self.work.clone()))
            .child(drag::watch(cx.weak_entity()))
            .when(furniture, |work| work.child(self.left.clone()))
            .child(
                v_flex()
                    .id("center")
                    .relative()
                    .flex_1()
                    .min_w(px(dock::FLOOR_WIDTH))
                    .h_full()
                    .child(area::measure(self.center.clone()))
                    .child(div().id("work-stage").test_support().flex_1().min_h(px(dock::FLOOR_HEIGHT)).child(stage))
                    .when(furniture, |center| center.child(self.bottom.clone())),
            )
            .when(furniture, |work| work.child(self.right.clone()))
            .children(self.carrying(&tokens, cx))
    }
}

#[cfg(test)]
mod tests;
