//! What the rail asks of the daemon: a call unless the sidebar is held, the close of Git that goes
//! before a pane or another stage comes up, and what a choice of a menu does.
//!
//! The pick and the + menu are the session's (`RailCursor`), so a press that picks or opens only
//! asks: the row is marked when the rail event that answers arrives, and never before.

use argus_desktop_core::connection::page::Call;
use gpui_kit::{Context, Window};

use super::menus::Does;
use super::RailView;
use crate::panes::console::rename;

impl RailView {
    /// Sends `call` unless the sidebar is held.
    pub(super) fn ask(&self, held: bool, call: Call) {
        if !held {
            self.caller.call(call);
        }
    }

    /// The close of whatever the session has open of Git, for a press that
    /// goes somewhere else. Git beside the work stays: it was put there to
    /// be kept while the stage changes.
    pub(super) fn leave_git(&mut self, cx: &mut Context<Self>) -> Vec<Call> {
        self.heard.update(cx, |heard, cx| {
            if heard.git_beside() {
                return Vec::new();
            }
            let open = heard.open();
            let calls = heard.git.leave(open);
            cx.notify();
            calls
        })
    }

    /// Brings a pane on screen as a click on its row does: the row is picked, Git goes first, and
    /// the keys stay on the rail until Enter or a click on the terminal (SHELL-RAIL-13).
    pub(super) fn show(&mut self, checkout: u64, pane: u64, held: bool, cx: &mut Context<Self>) {
        if held {
            return;
        }
        self.caller.call(Call::RailPick { checkout, pane: Some(pane) });
        for call in self.leave_git(cx) {
            self.caller.call(call);
        }
        self.caller.call(Call::Select { pane });
    }

    /// What a choice of a rail menu does, run after the menu has gone.
    pub(super) fn perform(&mut self, does: &Does, window: &mut Window, cx: &mut Context<Self>) {
        let held = self.heard.read(cx).held();
        match does {
            Does::Send(calls) => calls.iter().for_each(|call| self.ask(held, call.clone())),
            Does::Go(calls) => {
                if held {
                    return;
                }
                for call in self.leave_git(cx).into_iter().chain(calls.iter().cloned()) {
                    self.caller.call(call);
                }
            }
            Does::Rename { pane, title } => rename::open(*pane, title.clone(), self.caller.clone(), self.heard.clone(), window, cx),
        }
    }
}
