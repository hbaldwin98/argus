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
use gpui_kit::{Context, Entity, KeyDownEvent, Window};

use super::picked::{settled, Picked};
use super::RailView;
use crate::connection::heard::Heard;

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

    /// A key while the rail has the keys: only a picked row answers one, and
    /// every key is held (SHELL-KEYS-16) so none is typed into the shell.
    pub(super) fn press(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let held = self.heard.read(cx).held();
        let Some(picked) = self.picked else { return };
        match event.keystroke.key.as_str() {
            "escape" => self.give_back(window, cx),
            "enter" if picked.pane.is_some() => self.give_back(window, cx),
            "x" if picked.pane.is_some() && !held => {
                let pane = picked.pane.expect("checked");
                self.caller.call(Call::Close { pane });
                self.picked = Some(Picked { checkout: picked.checkout, pane: None });
                cx.notify();
            }
            _ => {}
        }
    }

    /// Unpicks and gives the stage the keys (SHELL-RAIL-24, 27).
    fn give_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picked = None;
        window.focus(&self.stage, cx);
        cx.notify();
    }
}
