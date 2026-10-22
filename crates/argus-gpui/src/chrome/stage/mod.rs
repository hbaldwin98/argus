//! What fills the stage under the header, by the one choice `route::surface` makes: the Git stage
//! while it is up, the Workspace's live pane (with the bar of a pane whose program ended over its
//! foot) or, with none on screen, the empty stage; and for a stage whose surface is not in the
//! window yet, a line that says so. The console is drawn by the work area (`layout/work`), which
//! asks the same router.
//!
//! The Browsers stage is `browser/stage`, drawn here like the Checkouts and Code stages.
//!
//! The session has no Git stage (`review::entry`), so the choice is the window's, made from what
//! the session says is open. The pane is kept, not dropped, under the empty stage: its screen and
//! its subscription are the session's, and the keys still go through it. While the daemon is gone
//! the body shows what was last heard, dimmed (SHELL-STAGE-14).

pub mod empty;
pub mod route;
pub mod running;
pub mod stopped;

use argus_desktop_core::chrome::empty as core_empty;
use argus_desktop_core::chrome::strip::stage_name;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, AnyView, Context, Entity, Subscription, Window};

use super::ended;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::extensions::bodies::Bodies as ExtensionBodies;
use crate::keys::caps;
use crate::layout::bodies::Contents;
use crate::layout::panel;
use crate::theme::Tokens;
use route::{surface, Surface};

/// The views the stage swaps between, one for each surface that is a view of its own.
pub struct Bodies {
    pub pane: AnyView,
    pub git: AnyView,
    pub checkouts: AnyView,
    pub code: AnyView,
    pub browsers: AnyView,
}

pub struct StageView {
    heard: Entity<Heard>,
    caller: Caller,
    pane: AnyView,
    git: AnyView,
    checkouts: AnyView,
    code: AnyView,
    browsers: AnyView,
    _observing: Subscription,
}

impl StageView {
    pub fn new(heard: Entity<Heard>, caller: Caller, bodies: Bodies, cx: &mut Context<Self>) -> Self {
        let Bodies { pane, git, checkouts, code, browsers } = bodies;
        let observing = cx.observe(&heard, |_, _, cx| cx.notify());
        Self { heard, caller, pane, git, checkouts, code, browsers, _observing: observing }
    }
}

impl Render for StageView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        // An extension's page is its own element, which a dock would draw as well (`extensions/bodies`).
        let extension = ExtensionBodies::of(cx).page(&self.heard, &self.caller, window, cx);
        let heard = self.heard.read(cx);
        let held = heard.held();
        let body: AnyElement = match surface(&heard.rail, heard.console.place, heard.git_up()) {
            Surface::Git => self.git.clone().into_any_element(),
            Surface::Checkouts => self.checkouts.clone().into_any_element(),
            Surface::Code => self.code.clone().into_any_element(),
            Surface::Browsers => self.browsers.clone().into_any_element(),
            Surface::Extension => extension.map_or_else(|| div().into_any_element(), IntoElement::into_any_element),
            Surface::Pane => div()
                .relative()
                .size_full()
                .child(self.pane.clone())
                .children(ended::of(&heard.rail).map(|ended| ended::bar(ended, held, &self.caller, &tokens)))
                .into_any_element(),
            Surface::Empty => {
                let status = heard.status.as_ref();
                let said = core_empty::of(&heard.rail, status.and_then(|s| s.host.as_deref()), status.and_then(|s| s.detail.as_deref()));
                let ways = empty::Ways {
                    caller: &self.caller,
                    held,
                    compose: caps::presses(&heard.keymap, Action::Compose, Place::Anywhere).into_iter().next(),
                };
                div().relative().size_full().child(self.pane.clone()).children(said.map(|said| empty::stage(said, &ways, &tokens))).into_any_element()
            }
            Surface::Pending(stage) => match Contents::peek(cx).stage(stage) {
                Some(page) => page.into_any_element(),
                None => panel::body(stage_name(stage), &tokens).into_any_element(),
            },
            // The work area puts the console here itself.
            Surface::Console => div().into_any_element(),
        };
        // Named by what it shows, so a held body says it is the last heard (and a test can tell).
        div()
            .id("stage-body")
            .test_support()
            .aria_label(if held { "Stage, as last heard" } else { "Stage" })
            .relative()
            .size_full()
            .when(held, |stage| stage.opacity(tokens.opacity_dim))
            .child(body)
    }
}

#[cfg(test)]
mod tests;
