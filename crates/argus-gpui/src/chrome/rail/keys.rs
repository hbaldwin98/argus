//! Who has the keys: the rail takes them when a row is clicked and holds them while a row is
//! picked, and gives them back to the stage.
//!
//! The rail takes the keys when a row is clicked, and holds them while a row is picked
//! (SHELL-RAIL-13, 19, SHELL-SEAM-9 f, g): `x` closes a picked pane, Enter or Escape hands them
//! back to the stage, and so does a pane newly shown, or any stage change with a pane shown, while
//! nothing is picked (not into the composer or the Feature stage, which SEAM-9 d and e keep for
//! their own). A press anywhere outside the rail drops the pick and hands the stage the keys too
//! (SHELL-SEAM-14): a press on a header or a button focuses nothing, so a rail that kept the keys
//! would hold every one unheard. A key it has no action for is held, not typed into the shell
//! (SHELL-KEYS-16).

use argus_desktop_core::connection::page::{Call, Stage};
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, Context, Entity, WeakEntity, Window};

use super::picked::{settled, Picked};
use super::RailView;
use crate::connection::heard::Heard;
use crate::keys::extension::{self, ANYWHERE, CHECKOUT, PANE};
use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::keys::run;

impl RailView {
    /// The tree changed: the pick follows it, and the keys go to the pane on screen when it is the
    /// one thing that should have them.
    pub(super) fn follow(&mut self, heard: &Entity<Heard>, window: &mut Window, cx: &mut Context<Self>) {
        let (picked, shown, stage, git) = {
            let heard = heard.read(cx);
            let rail = &heard.rail;
            (settled(self.picked, rail), rail.shown.as_ref().map(|shown| shown.pane), rail.stage, heard.git_up())
        };
        self.picked = picked;
        // A pane newly shown, or any shown pane after a stage change,
        // takes the keys unless a row holds them (SEAM-9 f, g). The
        // composer and the Feature stage keep them for their own
        // text (SEAM-9 d, e). Git takes them itself as it appears (its observer runs
        // after this one) and they come back to the pane through this once
        // it goes (CROSS-11).
        let moved = shown != self.shown || stage != self.last_stage || git != self.last_git;
        if shown.is_some() && moved && self.picked.is_none() && !matches!(stage, Stage::Composer | Stage::Feature) {
            window.focus(&self.stage, cx);
        }
        self.shown = shown;
        self.last_stage = stage;
        self.last_git = git;
        cx.notify();
    }

    /// A press outside the rail: the pick goes, and the keys go to the pane
    /// on screen (SHELL-SEAM-14).
    pub(super) fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picked = None;
        if self.shown.is_some() {
            window.focus(&self.stage, cx);
        }
        cx.notify();
    }

    /// A key while a row is picked: its action in the places the rail hears, `Anywhere` then
    /// `Rail` (SHELL-KEYS-16). Escape, or Enter on a pane, hands the keys back; `x` closes a picked
    /// pane; an action of the window's is run after (the stage switches). A key it has no action
    /// for is held, so none is typed into the shell.
    fn answer(&mut self, press: &Press, places: &[Place], window: &mut Window, cx: &mut Context<Self>) -> Answer {
        let held = self.heard.read(cx).held();
        let Some(picked) = self.picked else { return Answer::Held };
        match action_for(places, press.page) {
            Some(Action::CloseLayer) => self.give_back(window, cx),
            Some(Action::Choose) if picked.pane.is_some() => self.give_back(window, cx),
            Some(Action::Close) if picked.pane.is_some() && !held => {
                let pane = picked.pane.expect("checked");
                self.caller.call(Call::Close { pane });
                self.picked = Some(Picked { checkout: picked.checkout, pane: None });
                cx.notify();
            }
            Some(action) => return Answer::Run(action),
            // A key the window binds nothing to may be an extension command's, heard on the
            // picked row: a pane's too when a pane is picked (SHELL-KEYS-21).
            None => {
                let scopes: &[&str] = if picked.pane.is_some() { &[ANYWHERE, CHECKOUT, PANE] } else { &[ANYWHERE, CHECKOUT] };
                if !extension::run(scopes, press.page, Some(picked.checkout), picked.pane, cx) {
                    return Answer::Held;
                }
            }
        }
        Answer::Taken
    }

    /// Unpicks and gives the stage the keys (SHELL-RAIL-24, 27).
    fn give_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picked = None;
        window.focus(&self.stage, cx);
        cx.notify();
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
            rail.picked.is_some() && rail.focus.contains_focused(window, cx)
        })
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some(rail) = self.rail.upgrade() else { return Verdict::Pass };
        match rail.update(cx, |rail, cx| rail.answer(press, places, window, cx)) {
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
