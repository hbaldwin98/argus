//! A checkout's + menu, open under its row: the kit's start menu (`kit/start`) positioned in the
//! row and asking the session's cursor what a press or a choice comes to.
//!
//! A click on an item starts it without picking the row; a press anywhere else shuts the menu.

use argus_desktop_core::connection::page::{Call, Checkout, Rail};
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{deferred, prelude::*, Context};

use super::RailView;
use crate::keys::caps;
use crate::kit::start::{menu, Start};
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn start_menu(&self, rail: &Rail, repository: &str, checkout: &Checkout, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, held) = (checkout.id, self.heard.read(cx).held());
        let keymap = &self.heard.read(cx).keymap;
        let caps_of = |action| caps::presses(keymap, action, Place::Start).into_iter().next().unwrap_or_default();
        let start = Start {
            checkout: id,
            label: format!("Start on {repository} {}", checkout.name),
            at: rail.cursor.start.map_or(0, |menu| menu.at),
            templates: &rail.templates,
            shell_keys: caps_of(Action::StartShell),
            agent_keys: caps_of(Action::StartAgent),
        };
        let (rail_out, rail_chose) = (cx.weak_entity(), cx.weak_entity());
        let plus = self.plus_hover.clone();
        let menu = menu(
            start,
            // A press anywhere else shuts it; the + that opened it toggles it itself.
            move |_, cx| {
                if !plus.get() {
                    rail_out.update(cx, |this, _| this.ask(held, Call::StartKey { action: Action::CloseLayer })).ok();
                }
            },
            move |template, _, cx| {
                rail_chose.update(cx, |this, _| this.ask(held, Call::StartOn { checkout: id, template })).ok();
            },
            tokens,
        );
        deferred(menu.absolute().top(rem(tokens.menu_top)).right(rem(tokens.sp_8)))
    }
}
