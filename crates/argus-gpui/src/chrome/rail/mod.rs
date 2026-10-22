//! The sidebar: the nav, the projects, the picked project's repositories with their checkouts and
//! panes, and the foot.
//!
//! Our own list rather than `gpui-kit`'s `Sidebar`, whose selection a test cannot observe
//! (`GPUI.md`, what the spike found). Everything it draws is the latest `Rail` event, and a press
//! only sends the `Call` the page sends: the rail changes when the daemon's tree says it did.
//!
//! While the daemon is gone the list is dimmed and answers no press (`Heard::held`): no call, and
//! no pick either.

pub mod brand;
pub mod calls;
pub mod checkout;
pub mod keys;
pub mod listing;
pub mod pane;
pub mod picked;
pub mod projects;
pub mod repository;
pub mod row;
pub mod stages;
pub mod status;

use std::rc::Rc;

use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::keys::keymap::{Layer, Place};
use argus_desktop_core::panes::console::ConsolePlace;
use gpui_kit::component::{v_flex, ActiveTheme as _};
use gpui_kit::{prelude::*, px, Context, Entity, FocusHandle, Subscription, TestSupportExt as _, Window};

use crate::chrome::foot::{foot, Console};
use crate::chrome::stage::route::frame;
use crate::chrome::nav::Nav;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::caps;
use crate::keys::layer::Layers;
use crate::kit::spots::{Spot, Spots};
use crate::theme::Tokens;
use keys::RailKeys;
use picked::Picked;

pub struct RailView {
    heard: Entity<Heard>,
    caller: Caller,
    picked: Option<Picked>,
    focus: FocusHandle,
    /// The stage's keys, which the rail hands back.
    stage: FocusHandle,
    /// The pane on screen when the tree last changed.
    shown: Option<u64>,
    /// The stage when the tree last changed.
    last_stage: Stage,
    /// Whether the Git stage was up when the tree last changed.
    last_git: bool,
    _subscriptions: Vec<Subscription>,
}

impl RailView {
    pub fn new(heard: Entity<Heard>, caller: Caller, stage: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.observe_in(&heard, window, |this, heard, window, cx| this.follow(&heard, window, cx)),
            // A press outside the rail lets the pick go (SHELL-RAIL-19).
            cx.on_focus_out(&focus, window, |this, _, _, cx| {
                this.picked = None;
                cx.notify();
            }),
        ];
        // The rail's layer of the key walk: it holds the keys while a row is picked.
        Layers::of(cx).register(Layer::Rail, Rc::new(RailKeys { rail: cx.weak_entity() }));
        let (shown, last_stage, last_git) = {
            let heard = heard.read(cx);
            (heard.rail.shown.as_ref().map(|shown| shown.pane), heard.rail.stage, heard.git_up())
        };
        Self { heard, caller, picked: None, focus, stage, shown, last_stage, last_git, _subscriptions: subscriptions }
    }
}

impl Render for RailView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        // A row folded away or gone is no place to drop on: each says where it is again as it paints.
        Spots::of(cx).forget(|spot| matches!(spot, Spot::Checkout(_)));
        let mono = cx.theme().mono_font_family.clone();
        let (rail, git_up, held, link, alarm, keys, chime, console, furniture) = {
            let heard = self.heard.read(cx);
            let keys: Vec<Vec<String>> = Nav::ALL
                .iter()
                .map(|nav| {
                    nav.action()
                        .and_then(|action| caps::presses(&heard.keymap, action, Place::Anywhere).into_iter().next())
                        .unwrap_or_default()
                })
                .collect();
            (
                heard.rail.clone(),
                heard.git_up(),
                heard.held(),
                heard.link(),
                heard.status.as_ref().and_then(|s| s.alarm.clone()),
                keys,
                heard.status.as_ref().is_none_or(|s| s.chime),
                Console { shown: heard.console.place != ConsolePlace::Hidden, count: heard.console.tabs.len() },
                frame(&heard.rail, heard.console.place).furniture,
            )
        };
        let body = v_flex()
            .flex_1()
            .min_h_0()
            .when(furniture, |body| body.child(self.nav(&rail, git_up, held, &keys, &mono, &tokens, cx)))
            .when(rail.projects.len() > 1, |body| body.child(self.projects(&rail, held, &tokens, cx)))
            .child(self.listing(&rail, held, &tokens, cx));
        v_flex()
            .id("rail")
            .test_support()
            .key_context("Rail")
            .track_focus(&self.focus)
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.release(window, cx)))
            .w(px(tokens.rail_w))
            .flex_shrink_0()
            .h_full()
            .bg(tokens.bg_chrome)
            .border_r_1()
            .border_color(tokens.line)
            .text_color(tokens.fg_muted)
            .text_size(px(tokens.fs_md))
            .child(self.brand(&rail, &tokens))
            .child(body)
            // A fullscreen console puts the nav and the foot away and keeps the projects.
            .when(furniture, |column| column.child(foot(&link, &rail, alarm.as_deref(), chime, console, &self.caller, held, &tokens)))
    }
}

#[cfg(test)]
mod tests;
