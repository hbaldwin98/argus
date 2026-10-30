//! The Git stage: Changes and History under one header, drawn where the Workspace's pane is while
//! the session has a review or a history open.
//!
//! Core has no Git stage (`review::entry` is how the window enters and leaves it), so this draws
//! what the session says: the review as the Changes tab, the history as the History tab, and
//! nothing of either before its event. The title bar switches the tab and counts the files
//! (`tabs`); the stage's strip names the branch and has the refresh and the close. Each sends a
//! call, and the stage changes when the session's event says it did.
//!
//! The stage holds the keys while it is up: the keys of the open tab are heard here when no field
//! has them (`review::keys`; `/` takes the history's filter field), every other key is held and
//! not typed into the pane under it, and a field in the stage (the jump field, the comment box)
//! hears only its own. The stage takes the keys as it appears and the rail gives them back to the
//! pane when it goes.

pub mod keys;
pub mod strip;
pub mod tabs;

use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Layer;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Entity, FocusHandle, SharedString, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::layer::Layers;
use crate::review::changes::Changes;
use crate::review::entry::{checkout_here, Tab};
use crate::review::filter::Filter;
use crate::review::history::History;
use keys::StageKeys;
use crate::theme::Tokens;

pub struct GitStage {
    heard: Entity<Heard>,
    caller: Caller,
    changes: Entity<Changes>,
    history: Entity<History>,
    /// The history's filter field, which sits in the strip while History is the tab.
    filter: Entity<Filter>,
    focus: FocusHandle,
    /// Whether the stage was up at the last update, to take the keys once as it appears.
    was_up: bool,
    /// Whether a field of the stage (the jump field, the comment box) was open
    /// at the last update, to give the keys back to the stage when it shuts.
    had_field: bool,
    /// Whether the session had a history open at the last update, to give the
    /// keys back to the stage when it goes with the filter field holding them.
    had_history: bool,
    _subscriptions: [Subscription; 1],
}

impl GitStage {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let changes = cx.new(|cx| Changes::new(heard.clone(), caller.clone(), window, cx));
        let history = cx.new(|cx| History::new(heard.clone(), caller.clone(), cx));
        let focus = cx.focus_handle();
        let filter = cx.new(|cx| Filter::new(heard.clone(), caller.clone(), focus.clone(), window, cx));
        let following = cx.observe_in(&heard, window, |this, _, window, cx| this.follow(window, cx));
        // The two tabs are two layers of the key walk, up while the stage has the keys.
        let layers = Layers::of(cx);
        for layer in [Layer::Review, Layer::History] {
            layers.register(layer, Rc::new(StageKeys { stage: cx.weak_entity(), layer }));
        }
        tabs::offer(cx.weak_entity(), heard.clone(), cx);
        let mut stage = Self {
            heard,
            caller,
            changes,
            history,
            filter,
            focus,
            was_up: false,
            had_field: false,
            had_history: false,
            _subscriptions: [following],
        };
        // A window opened on a session that already has a review hears nothing
        // more to follow it by, so it follows what it was given: the diff is
        // handed its rows here rather than staying blank until the next event.
        stage.follow(window, cx);
        stage
    }

    /// Follows the session. The stage takes the keys as it appears, then the
    /// tab follows its review, which may hand them to an open field.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (up, review, history) = {
            let heard = self.heard.read(cx);
            (heard.git_open(), heard.review.clone(), heard.history.clone())
        };
        if up && !self.was_up {
            self.take_keys(window, cx);
        }
        self.was_up = up;
        let field = review.as_ref().is_some_and(|page| page.comment.is_some() || page.jump.is_some());
        if let Some(review) = review {
            self.changes.update(cx, |changes, cx| changes.follow(&review, window, cx));
        }
        let historical = history.is_some();
        let filtering = history.as_ref().map(|page| page.filter.as_str());
        self.filter.update(cx, |filter, cx| filter.follow(filtering, window, cx));
        if let Some(history) = history {
            self.history.update(cx, |list, cx| list.follow(&history, cx));
        }
        // A history that goes while its filter held the keys leaves them on nothing.
        if self.had_history && !historical && up {
            self.take_keys(window, cx);
        }
        self.had_history = historical;
        // A field that shuts leaves the keys on nothing: the stage has them back,
        // so the review's own keys are heard again (CROSS-11).
        if self.had_field && !field && up {
            self.take_keys(window, cx);
        }
        self.had_field = field;
        cx.notify();
    }

    /// Gives the keys to whichever is on screen: the pane beside the work, else the stage.
    fn take_keys(&self, window: &mut Window, cx: &mut Context<Self>) {
        let heard = self.heard.read(cx);
        if heard.changes_beside() && heard.review.is_some() && heard.history.is_none() {
            let focus = self.changes.read(cx).focus.clone();
            window.focus(&focus, cx);
        } else {
            window.focus(&self.focus, cx);
        }
    }

    /// The Changes pane, which is also the changes' panel's body beside the work.
    pub fn changes(&self) -> &Entity<Changes> {
        &self.changes
    }

    /// Whether the keys are the stage's or the pane's itself, wherever the pane is drawn; a part of
    /// either that took them (a field, a drag) hears them first.
    pub(super) fn has_keys(&self, window: &Window, cx: &gpui_kit::App) -> bool {
        self.focus.is_focused(window) || self.changes.read(cx).focus.is_focused(window)
    }

    pub(super) fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// A tab pressed: the other one's close, then this one's open.
    pub(super) fn show(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.heard.read(cx).held() {
            return;
        }
        let calls = self.heard.update(cx, |heard, cx| {
            let (open, here) = (heard.open(), checkout_here(&heard.rail, None));
            let calls = heard.git.show(tab, open, here);
            cx.notify();
            calls
        });
        for call in calls {
            self.caller.call(call);
        }
    }

    /// The stage shut: the close of whatever the session has open.
    pub(super) fn close(&mut self, cx: &mut Context<Self>) {
        if self.heard.read(cx).held() {
            return;
        }
        let calls = self.heard.update(cx, |heard, cx| {
            let open = heard.open();
            let calls = heard.git.leave(open);
            cx.notify();
            calls
        });
        for call in calls {
            self.caller.call(call);
        }
    }

    /// A quiet line standing where a body is not drawn.
    fn said(id: &'static str, words: &'static str, tokens: &Tokens) -> gpui_kit::AnyElement {
        div()
            .id(id)
            .test_support()
            .aria_label(SharedString::from(words))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(tokens.fg_faint)
            .child(words)
            .into_any_element()
    }
}

impl Render for GitStage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let (tab, reviewing, historical, held) = {
            let heard = self.heard.read(cx);
            (heard.git.tab(heard.open()), heard.review.is_some(), heard.history.is_some(), heard.held())
        };
        // What the session says is open is drawn; with nothing open the stage
        // is up on the window's word alone, saying what to do.
        let body = match tab {
            Tab::Changes if reviewing => self.changes.clone().into_any_element(),
            Tab::History if historical => self.history.clone().into_any_element(),
            _ => Self::said("git-pick", "Pick a checkout in the sidebar, then press Git.", &tokens),
        };
        v_flex()
            .id("git")
            .test_support()
            .key_context("Git")
            .track_focus(&self.focus)
            .size_full()
            // The dialog's surface: the files and the commits sit on it, the diff
            // is sunk below it.
            .bg(tokens.bg_surface)
            .text_color(tokens.fg_muted)
            .when(held,|stage| stage.opacity(0.5))
            .child(self.strip(tab, &tokens, cx))
            .child(div().id("git-body").flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests;
