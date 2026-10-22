//! The composer: what to start, where, and the agent's first message (COMPOSE-1..15).
//!
//! Which repository, checkout and agent are picked, the line saying where the start will be, and the
//! call a start sends are core's (`agents::composer`); the draft (the checkout, the agent and the
//! message) is held here until a pane the start made is shown. The repository is the rail's, so
//! picking another chooses it there and the rail follows. A press changes nothing on screen: the
//! composer stays up until the daemon makes the pane, so a refused start keeps what was typed.
//!
//! The message takes the keys when the stage first comes up. Ctrl Enter (the `Writing` place's
//! `Save`) starts the agent; every other key types, Enter and Escape included. A rail update for
//! another pane never touches the controls: each picker is written to only when its list or its
//! pick changed (`picker`), and the message is never written while it is typed in.

mod picker;
mod view;

use argus_desktop_core::agents::composer::{ComposerPage, Draft};
use argus_desktop_core::connection::page::{Rail, Stage};
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::input::{TextareaState};
use gpui_kit::{prelude::*, App, Context, Entity, Focusable as _, Subscription, Window};

use super::ask::ask;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use picker::Picker;

/// The rows of the message's text area.
const ROWS: usize = 4;

pub struct ComposeView {
    heard: Entity<Heard>,
    caller: Caller,
    draft: Draft,
    text: Entity<TextareaState>,
    repository: Picker<u64>,
    checkout: Picker<u64>,
    agent: Picker<String>,
    /// The rail as last heard, to tell a start the daemon made from any other change.
    before: Rail,
    /// Whether the stage was up at the last update, to give the message the keys once as it appears.
    was_up: bool,
    _following: Subscription,
    _field: Registered,
}

impl ComposeView {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx).rows(ROWS).placeholder("Describe the work, or leave it empty to start the agent idle"));
        let me = cx.weak_entity();
        let (on_repository, on_checkout, on_agent) = (me.clone(), me.clone(), me.clone());
        let repository = Picker::new(move |id: &u64, _, cx| drop(on_repository.update(cx, |this, cx| this.pick_repository(*id, cx))), window, cx);
        let checkout = Picker::new(move |id: &u64, _, cx| drop(on_checkout.update(cx, |this, cx| this.pick_checkout(*id, cx))), window, cx);
        let agent = Picker::new(move |name: &String, _, cx| drop(on_agent.update(cx, |this, cx| this.pick_agent(name, cx))), window, cx);
        // Ctrl Enter starts the agent; every other key types, Escape included.
        let starting = cx.weak_entity();
        let field = Fields::of(cx).register_place(window, text.focus_handle(cx), Place::Writing, move |action, _, cx| {
            if action != Action::Save {
                return false;
            }
            starting.update(cx, |this, cx| this.start(true, cx)).is_ok()
        });
        let following = cx.observe_in(&heard, window, |this, _, window, cx| this.follow(window, cx));
        let mut view = Self { heard, caller, draft: Draft::default(), text, repository, checkout, agent, before: Rail::default(), was_up: false, _following: following, _field: field };
        view.follow(window, cx);
        view
    }

    /// The composer as the rail and the draft make it.
    fn page(&self, cx: &App) -> ComposerPage {
        ComposerPage::of(&self.heard.read(cx).rail, &self.draft)
    }

    /// Follows the session: the pickers show what Rust says, a start the daemon made ends the
    /// message, and the stage taking the screen gives the message the keys.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rail = self.heard.read(cx).rail.clone();
        // What is typed is the draft's message, which a start made by the daemon ends.
        self.draft.text = self.text.read(cx).value().to_string();
        let before = std::mem::replace(&mut self.before, rail.clone());
        let held = self.draft.text.clone();
        self.draft.settle(&before, &rail);
        if self.draft.text != held {
            self.text.update(cx, |text, cx| text.set_value("", window, cx));
        }
        let page = ComposerPage::of(&rail, &self.draft);
        self.repository.sync(page.repositories.iter().map(|pick| (pick.value, pick.label.clone())).collect(), page.repository.as_ref(), window, cx);
        self.checkout.sync(page.checkouts.iter().map(|pick| (pick.value, pick.label.clone())).collect(), page.checkout.as_ref(), window, cx);
        self.agent.sync(page.templates.iter().map(|name| (name.clone(), name.clone())).collect(), page.template.as_ref(), window, cx);
        let up = rail.stage == Stage::Composer;
        if up && !self.was_up {
            self.text.update(cx, |text, cx| text.focus(window, cx));
        }
        self.was_up = up;
        cx.notify();
    }

    fn pick_repository(&mut self, id: u64, cx: &mut Context<Self>) {
        let rail = self.heard.read(cx).rail.clone();
        if let Some(call) = self.draft.pick_repository(&rail, id) {
            ask(&self.heard, &self.caller, call, cx);
        }
        cx.notify();
    }

    /// Choosing a checkout stores the draft and redraws locally: no call.
    fn pick_checkout(&mut self, id: u64, cx: &mut Context<Self>) {
        self.draft.pick_checkout(id);
        cx.notify();
    }

    fn pick_agent(&mut self, name: &str, cx: &mut Context<Self>) {
        self.draft.pick_template(name);
        cx.notify();
    }

    /// Starts an agent with the message, or a shell that ignores it.
    fn start(&mut self, agent: bool, cx: &mut Context<Self>) {
        self.draft.text = self.text.read(cx).value().to_string();
        let rail = self.heard.read(cx).rail.clone();
        if let Some(call) = self.draft.start(&rail, agent) {
            ask(&self.heard, &self.caller, call, cx);
        }
    }

    #[cfg(test)]
    pub(crate) fn draft(&self) -> &Draft {
        &self.draft
    }
}

#[cfg(test)]
mod tests;
