//! The room a screen reports to the session: how many rows and columns its element holds, said
//! once it has held still, and only when it changed (SHELL-SCREEN-11).
//!
//! A window being dragged would otherwise reflow the pane at every step, and a sash being held
//! reports nothing until it is let go.

use std::time::Duration;

use argus_desktop_core::connection::page::Call;
use gpui_kit::Context;

use super::Terminal;

/// How long a size stays unreported after it last changed.
pub const SETTLE: Duration = Duration::from_millis(50);

impl Terminal {
    /// The element's size now holds `rows` by `cols`. Reported to the session
    /// once it has held still for [`SETTLE`], and only when it differs from
    /// the last report.
    pub fn room(&mut self, rows: u16, cols: u16, cx: &mut Context<Self>) {
        self.wanted = Some((rows, cols));
        self.settling = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |this, cx| this.settle(cx)).ok();
        }));
    }

    /// Sends the settled room, unless a sash is held: the stage is then being
    /// resized by a gesture and reports when it is let go (see `observe` in `new`).
    pub(super) fn settle(&mut self, cx: &Context<Self>) {
        if self.heard.read(cx).resizing() {
            self.waiting = true;
            return;
        }
        self.waiting = false;
        if self.wanted == self.sent {
            return;
        }
        let Some((rows, cols)) = self.wanted else {
            return;
        };
        self.sent = self.wanted;
        self.caller.call(Call::Resize { side: self.side, rows, cols });
    }
}
