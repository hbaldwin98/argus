//! What the rail asks of the daemon: a call unless the sidebar is held, and the close of Git that
//! goes before a pane or another stage comes up.

use argus_desktop_core::connection::page::Call;
use gpui_kit::Context;

use super::RailView;

impl RailView {
    /// Sends `call` unless the sidebar is held.
    pub(super) fn ask(&self, held: bool, call: Call) {
        if !held {
            self.caller.call(call);
        }
    }

    /// The close of whatever the session has open of Git, for a press that
    /// goes somewhere else.
    pub(super) fn leave_git(&mut self, cx: &mut Context<Self>) -> Vec<Call> {
        self.heard.update(cx, |heard, cx| {
            let open = heard.open();
            let calls = heard.git.leave(open);
            cx.notify();
            calls
        })
    }
}
