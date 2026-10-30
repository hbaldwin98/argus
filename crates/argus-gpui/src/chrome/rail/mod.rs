//! The sidebar: an icon strip of the stages down its left edge, and beside it the panel with the
//! project, the filter chips and the picked project's repositories with their checkouts and panes,
//! as wide as the person made it and folded to the strip alone when it is narrow.
//!
//! Our own list rather than `gpui-kit`'s `Sidebar`, whose selection a test cannot observe
//! (`GPUI.md`, what the spike found). Everything it draws is the latest `Rail` event, read through a
//! slice it compares (`slice`), and a press only sends the `Call` the page sends: the rail changes
//! when the daemon's tree says it did. Which row is picked and which + menu is open are the
//! session's (`chrome::cursor`), so a press asks and the row is marked when the event comes back.
//! The filter chip and the quiet row's fold are the window's own, since they are ways of looking.
//!
//! While the daemon is gone the list is dimmed and answers no press (`Heard::held`): no call, and
//! no pick either.

pub mod chips;
pub mod calls;
pub mod checkout;
pub mod counts;
pub mod filter;
pub mod header;
pub mod keys;
pub mod listing;
pub mod menus;
pub mod pane;
pub mod popup;
pub mod quiet;
pub mod repository;
pub mod row;
pub mod scroll;
pub mod server;
pub mod slice;
pub mod stages;
pub mod strip;
pub mod switcher;
pub mod start;
pub mod status;
pub mod tail;

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::keys::keymap::Layer;
use argus_desktop_core::layout::dragged_width::{rail_drawn, rail_icons, RAIL, RAIL_FOLDED};
use gpui_kit::component::v_flex;
use gpui_kit::{prelude::*, Bounds, Context, Entity, FocusHandle, Pixels, ScrollHandle, Subscription, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::split::Split;
use crate::keys::layer::Layers;
use crate::layout::holders::Holder;
use crate::theme::scale::{css, rem};
use crate::theme::Tokens;
use keys::{RailKeys, StartKeys};
use row::Ctx;
use slice::Slice;
use filter::Filter;
use tail::ViewsFold;

pub struct RailView {
    heard: Entity<Heard>,
    caller: Caller,
    /// What the rail draws of what was heard, compared on every event so only a change redraws.
    slice: Slice,
    focus: FocusHandle,
    /// The stage's keys, which the rail hands back.
    stage: FocusHandle,
    /// The pane on screen when the tree last changed.
    shown: Option<u64>,
    /// The stage when the tree last changed.
    last_stage: Stage,
    /// Whether the Git stage was up when the tree last changed.
    last_git: bool,
    /// Whether a row was picked when the tree last changed.
    last_pick: Option<argus_desktop_core::chrome::cursor::Pick>,
    /// The pane last scrolled to, so it is scrolled to once.
    scrolled_to: Option<u64>,
    pending_scroll: bool,
    scroll: ScrollHandle,
    /// Where the row of the pane on screen was drawn.
    shown_row: Rc<Cell<Bounds<Pixels>>>,
    /// Whether the pointer is on a + button: a press on the one that opened a menu toggles it, and
    /// must not also be a press outside it.
    plus_hover: Rc<Cell<bool>>,
    /// Which filter chip is on: a way of looking at the tree, the window's own.
    filter: Filter,
    /// Whether the quiet-repositories row is open.
    quiet_open: bool,
    /// What only the window knows of the Views fold.
    views: ViewsFold,
    /// The edge the rail is dragged by, which holds the width until Rust says what it kept.
    split: Entity<Split>,
    #[cfg(test)]
    changes: Cell<usize>,
    _subscriptions: Vec<Subscription>,
}

impl RailView {
    pub fn new(heard: Entity<Heard>, caller: Caller, stage: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.observe_in(&heard, window, |this, heard, window, cx| this.follow(&heard, window, cx)),
            // A press outside the rail lets the pick go (SHELL-RAIL-19).
            cx.on_focus_out(&focus, window, |this, _, _, _| this.let_go()),
        ];
        // The rail's layers of the key walk: the + menu while it is open, and the rail while a row is
        // picked and it has the keys.
        let layers = Layers::of(cx);
        layers.register(Layer::Rail, std::rc::Rc::new(RailKeys { rail: cx.weak_entity() }));
        layers.register(Layer::Start, std::rc::Rc::new(StartKeys { rail: cx.weak_entity() }));
        let slice = Slice::of(heard.read(cx), None);
        let (shown, last_stage, last_git, last_pick) =
            (slice.rail.shown.as_ref().map(|shown| shown.pane), slice.rail.stage, slice.git_up, slice.rail.cursor.pick);
        let tokens = *Tokens::global(cx);
        let (holding, rail) = (heard.clone(), cx.weak_entity());
        let report = caller.clone();
        let split = cx.new(|cx| {
            Split::new("rail-sash", "Resize the sidebar", tokens.rail_w, RAIL_FOLDED as f32, RAIL.max as f32, cx)
                .reporting(move |width, _, _| report.call(argus_desktop_core::connection::page::Call::RailWidth { width }))
                .holding(move |on, cx| {
                    holding.update(cx, |heard, cx| {
                        let changed = if on { heard.hold_resize(Holder::Rail) } else { heard.release_resize(Holder::Rail) };
                        if changed {
                            cx.notify();
                        }
                    });
                    rail.update(cx, |_, cx| cx.notify()).ok();
                })
        });
        Self {
            heard,
            caller,
            slice,
            focus,
            stage,
            shown,
            last_stage,
            last_git,
            last_pick,
            scrolled_to: shown,
            pending_scroll: shown.is_some(),
            scroll: ScrollHandle::new(),
            shown_row: Rc::default(),
            plus_hover: Rc::default(),
            filter: Filter::All,
            quiet_open: false,
            views: ViewsFold { listed: false, all: false, unfolded: HashSet::new() },
            split,
            #[cfg(test)]
            changes: Cell::new(0),
            _subscriptions: subscriptions,
        }
    }

    /// How often what the rail draws has changed, for a test that says an unrelated event does not.
    #[cfg(test)]
    pub(crate) fn changes(&self) -> usize {
        self.changes.get()
    }

    /// The row that holds the keys, as the session last said.
    pub fn picked(&self) -> Option<argus_desktop_core::chrome::cursor::Pick> {
        self.slice.rail.cursor.pick
    }
}

impl Render for RailView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let slice = self.slice.clone();
        let viewport = window.viewport_size();
        // The page's pixels: a window zoomed in is narrower than it is wide.
        let wide = css(viewport.width, window.rem_size());
        let narrow = crate::chrome::stage::route::narrow(wide, tokens.narrow_below);
        // A window this narrow folds the rail whatever it was dragged to, and its sash goes with its words.
        let width = rail_drawn(Some(self.split.read(cx).width().round() as u32), tokens.rail_w, wide, tokens.narrow_below);
        let icons = rail_icons(width);
        let ctx = Ctx {
            held: slice.held,
            offstage: slice.rail.stage != Stage::Workspace || slice.git_up,
            icons,
            height: f32::from(viewport.height),
        };
        self.schedule_scroll(window, cx);
        let rail = slice.rail.clone();
        let list = self.listing(&slice, ctx, &tokens, cx);
        // A folded rail is the strip alone, its pane tiles under the stages; a wide one is the strip and,
        // beside it, the panel.
        let (middle, panel) = if icons {
            (Some(list.into_any_element()), None)
        } else {
            let panel = v_flex()
                .id("rail-panel")
                .test_support()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(self.header(&rail, &slice, ctx, &tokens, cx))
                .child({ let (needs_you, working) = counts::in_view(&rail); self.chips(needs_you, working, &tokens, cx) })
                .child(list);
            (None, Some(panel))
        };
        gpui_kit::component::h_flex()
            .id("rail")
            .test_support()
            .key_context("Rail")
            .track_focus(&self.focus)
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.release(window, cx)))
            .relative()
            .w(rem(width))
            .flex_shrink_0()
            .h_full()
            .bg(tokens.see_chrome)
            .border_r_1()
            .border_color(tokens.line)
            .text_color(tokens.fg_muted)
            .text_size(rem(tokens.fs_md))
            .child(self.strip(&slice, ctx, middle, &tokens, cx))
            .children(panel)
            // The edge: dragged to size the rail, a double-click gives back the usual width. A narrow
            // window has no room to size it.
            .when(!narrow, |column| {
                column.child(gpui_kit::div().absolute().top_0().bottom_0().right_0().w(rem(tokens.sp_6)).child(self.split.clone()))
            })
    }
}

#[cfg(test)]
mod tests;
