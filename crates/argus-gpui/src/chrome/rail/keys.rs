//! Who has the keys: the rail takes them when a row is clicked and holds them while a row is
//! picked, and gives them back to the stage.
//!
//! The pick and the + menu are the session's (`chrome::cursor`), so a key the rail answers is sent
//! as it is (`RailKey`, `StartKey`) and the session says what it did in the next rail event; the
//! window only decides which keys are the rail's (SHELL-RAIL-13, 19, SHELL-SEAM-9 f, g): `x` closes a
//! picked pane, Enter or Escape hands them back to the stage, and so does a pane newly shown, or any
//! stage change with a pane shown, while nothing is picked (not into the composer or the Feature
//! stage, which SEAM-9 d and e keep for their own). A press anywhere outside the rail drops the pick
//! and hands the stage the keys too (SHELL-SEAM-14): a press on a header or a button focuses
//! nothing, so a rail that kept the keys would hold every one unheard. A key it has no action for is
//! held, not typed into the shell (SHELL-KEYS-16).

use argus_desktop_core::chrome::cursor;
use argus_desktop_core::connection::page::{Call, Stage};
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, Context, Entity, WeakEntity, Window};

use super::slice::Slice;
use crate::theme::Tokens;
use super::RailView;
use crate::connection::heard::Heard;
use crate::keys::extension::{self, ANYWHERE, CHECKOUT, PANE};
use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::keys::run;

impl RailView {
    /// The tree changed: what the rail draws is read again, and the keys go to the pane on screen
    /// when it is the one thing that should have them.
    pub(super) fn follow(&mut self, heard: &Entity<Heard>, window: &mut Window, cx: &mut Context<Self>) {
        let slice = Slice::of(heard.read(cx), Some(&self.slice));
        let rail = &slice.rail;
        let (pick, shown, stage, git) = (rail.cursor.pick, rail.shown.as_ref().map(|shown| shown.pane), rail.stage, slice.git_up);
        // A pane newly shown, or any shown pane after a stage change, takes the keys unless a row
        // holds them (SEAM-9 f, g). The composer and the Feature stage keep them for their own text
        // (SEAM-9 d, e). Git takes them itself as it appears (its observer runs after this one) and
        // they come back to the pane through this once it goes (CROSS-11).
        let moved = shown != self.shown || stage != self.last_stage || git != self.last_git;
        // Enter on a pane or Escape let the pick go while the rail still has the keys: they go to
        // the stage. A pick let go because the keys went elsewhere is no business of the rail's.
        let given_back = self.last_pick.is_some() && pick.is_none() && self.focus.contains_focused(window, cx);
        if shown.is_some() && (moved || given_back) && pick.is_none() && !matches!(stage, Stage::Composer | Stage::Feature) {
            window.focus(&self.stage, cx);
        }
        self.shown = shown;
        self.last_stage = stage;
        self.last_git = git;
        self.last_pick = pick;
        self.scroll_to(shown);
        // Rust says how wide the rail is as it does after a drag is let go.
        let usual = Tokens::global(cx).rail_w;
        let width = rail.width.map_or(usual, |width| width as f32);
        self.split.update(cx, |split, cx| split.follow(width, cx));
        if slice != self.slice {
            self.slice = slice;
            #[cfg(test)]
            self.changes.set(self.changes.get() + 1);
            cx.notify();
        }
    }

    /// A press outside the rail: the pick goes, and the keys go to the pane on screen
    /// (SHELL-SEAM-14).
    pub(super) fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.let_go();
        if self.shown.is_some() {
            window.focus(&self.stage, cx);
        }
    }

    /// Lets the pick go. The cursor is the session's own, so it is told even while the daemon is
    /// gone: nothing is asked of the daemon by it.
    pub(super) fn let_go(&self) {
        if self.slice.rail.cursor.pick.is_some() {
            self.caller.call(Call::RailUnpick);
        }
    }

    /// A key while a row is picked: its action in the places the rail hears, `Anywhere` then
    /// `Rail` (SHELL-KEYS-16). What the cursor answers is sent to it; an action of the window's is run
    /// after (the stage switches); a key it has no action for is held, so none is typed into the
    /// shell, unless it is an extension command's, heard on the picked row (SHELL-KEYS-21).
    fn answer(&mut self, press: &Press, places: &[Place], cx: &mut Context<Self>) -> Answer {
        let held = self.heard.read(cx).held();
        let Some(picked) = self.slice.rail.cursor.pick else { return Answer::Held };
        match action_for(places, press.page) {
            Some(action) if cursor::answers(action) => {
                // What goes no further than the window may be done with the daemon gone.
                if !held || matches!(action, Action::CloseLayer | Action::Choose) {
                    self.caller.call(Call::RailKey { action });
                }
            }
            Some(action) => return Answer::Run(action),
            // A pane's commands are heard only on a picked pane.
            None => {
                let scopes: &[&str] = if picked.pane.is_some() { &[ANYWHERE, CHECKOUT, PANE] } else { &[ANYWHERE, CHECKOUT] };
                if !extension::run(scopes, press.page, Some(picked.checkout), picked.pane, cx) {
                    return Answer::Held;
                }
            }
        }
        Answer::Taken
    }
}

/// What the picked row made of a key.
enum Answer {
    Held,
    Taken,
    /// An action of the window's, for the root to run once the rail is let go of.
    Run(Action),
}

/// The rail's layer of the key walk: up while a row is picked and the rail has the keys.
pub struct RailKeys {
    pub rail: WeakEntity<RailView>,
}

impl Handler for RailKeys {
    fn open(&self, window: &Window, cx: &App) -> bool {
        self.rail.upgrade().is_some_and(|rail| {
            let rail = rail.read(cx);
            rail.slice.rail.cursor.pick.is_some() && rail.focus.contains_focused(window, cx)
        })
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some(rail) = self.rail.upgrade() else { return Verdict::Pass };
        match rail.update(cx, |rail, cx| rail.answer(press, places, cx)) {
            Answer::Held => Verdict::Held,
            Answer::Taken => Verdict::Taken,
            Answer::Run(action) => {
                if run::run(action, window, cx) {
                    Verdict::Taken
                } else {
                    Verdict::Held
                }
            }
        }
    }
}

/// The + menu's layer of the key walk: up while the session says a menu is open. It holds every key
/// it does not answer (`startKeys` in the page).
pub struct StartKeys {
    pub rail: WeakEntity<RailView>,
}

impl Handler for StartKeys {
    fn open(&self, _: &Window, cx: &App) -> bool {
        self.rail.upgrade().is_some_and(|rail| rail.read(cx).slice.rail.cursor.start.is_some())
    }

    fn keys(&self, press: &Press, places: &[Place], _: &mut Window, cx: &mut App) -> Verdict {
        let Some(rail) = self.rail.upgrade() else { return Verdict::Pass };
        let Some(action) = action_for(places, press.page).filter(|action| cursor::answers_start(*action)) else { return Verdict::Held };
        rail.update(cx, |rail, cx| {
            // Starting something asks the daemon; moving through the menu and shutting it do not.
            let held = rail.heard.read(cx).held();
            if !held || matches!(action, Action::CloseLayer | Action::Next | Action::Previous) {
                rail.caller.call(Call::StartKey { action });
            }
        });
        Verdict::Taken
    }
}

