//! What the stage header says of the Workspace's grid, kept as frames arrive: told to whoever
//! draws the header when its words change (`panes/size_label`).

use argus_desktop_core::connection::page::Side;
use gpui_kit::Context;

use super::{Labelled, Terminal};
use crate::panes::size_label::REASK_AFTER;

impl Terminal {
    /// A frame changed this side's screen: the Workspace's header may have news of its size.
    pub(super) fn framed(&mut self, cx: &mut Context<Self>) {
        self.expect(cx);
        if self.side != Side::Workspace {
            return;
        }
        let now = cx.background_executor().now();
        let heard = self.heard.read(cx);
        let screen = heard.screen_of(self.side);
        let (rows, cols) = screen.size();
        let showing = self.label.frame(screen.pane(), rows, cols, screen.parked(), now);
        if showing {
            // Asked again when it ends, once: a timer only while there is something to end.
            self.relabelling = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(REASK_AFTER).await;
                this.update(cx, |this, cx| this.relabel(cx)).ok();
            }));
        }
        self.relabel(cx);
    }

    fn relabel(&mut self, cx: &mut Context<Self>) {
        let words = self.label.text(cx.background_executor().now());
        if words != self.labelled {
            self.labelled = words.clone();
            cx.emit(Labelled(words));
        }
    }
}
