//! The pane this window shows: which one, its subscription and size on
//! this connection, its view, whether it fills the window, and whether the
//! leader is waiting on the next key typed into it.
//!
//! Which pane is shown is the window's own choice, as the terminal
//! client's selection is; the session checks the tree holds it before
//! asking. The daemon sizes a pane to the client the person was last at,
//! so the size asked for is sent only when it changes on this connection.

use argus_protocol::pane_view::{Effect, PaneView};
use argus_protocol::{ClientMsg, MouseTracking, PaneId, ServerMsg};

use crate::page::Frame;
use crate::screen::Screen;

#[derive(Default)]
pub struct ShownPane {
    pane: Option<PaneId>,
    screen: Option<Screen>,
    /// The rows and columns the page has room for.
    capacity: Option<(u16, u16)>,
    /// The size last asked for the shown pane on this connection.
    sized: Option<(u16, u16)>,
    /// The person asked for the pane to fill the window.
    filled: bool,
    /// The leader was pressed in the pane: the next key is the window's.
    leader: bool,
}

impl ShownPane {
    pub fn pane(&self) -> Option<PaneId> {
        self.pane
    }

    /// Shows `pane`, letting the one before go. False when it was already
    /// the one shown.
    pub fn show(&mut self, pane: PaneId, out: &mut Vec<ClientMsg>) -> bool {
        if self.pane == Some(pane) {
            return false;
        }
        self.hide(out);
        self.pane = Some(pane);
        self.screen = Some(Screen::new(pane));
        out.push(ClientMsg::Subscribe { pane });
        self.send_size(out);
        true
    }

    /// Shows nothing. As in the terminal client, filling is for the pane it
    /// was asked on: another pane comes back with the rail beside it.
    pub fn hide(&mut self, out: &mut Vec<ClientMsg>) {
        if let Some(pane) = self.pane.take() {
            out.push(ClientMsg::Unsubscribe { pane });
        }
        self.filled = false;
        self.leader = false;
        self.screen = None;
        self.sized = None;
    }

    /// The tree no longer holds the pane, and the daemon's subscriptions
    /// let it go with it: nothing to unsubscribe.
    pub fn gone(&mut self) {
        self.pane = None;
        self.screen = None;
        self.sized = None;
    }

    /// The room the page has for a screen. While another client is the one
    /// the person was last at the grid may come back another size, and the
    /// page pads or crops it.
    pub fn resize(&mut self, rows: u16, cols: u16, out: &mut Vec<ClientMsg>) {
        self.capacity = Some((rows.max(1), cols.max(1)));
        self.send_size(out);
    }

    /// A new connection knows nothing this window asked of the last: the
    /// pane is subscribed to again, at its size, on a fresh screen.
    pub fn reconnected(&mut self, out: &mut Vec<ClientMsg>) {
        self.sized = None;
        if let Some(pane) = self.pane {
            self.screen = Some(Screen::new(pane));
            out.push(ClientMsg::Subscribe { pane });
            self.send_size(out);
        }
    }

    fn send_size(&mut self, out: &mut Vec<ClientMsg>) {
        let (Some(pane), Some(size)) = (self.pane, self.capacity) else {
            return;
        };
        if self.sized == Some(size) {
            return;
        }
        self.sized = Some(size);
        out.push(ClientMsg::Resize { pane, rows: size.0, cols: size.1 });
    }

    /// A message about the shown pane's view; `None` for anything else.
    pub fn receive(&mut self, msg: &ServerMsg) -> Option<Effect> {
        self.screen.as_mut()?.receive(msg)
    }

    /// A gesture on the shown pane's view, as `PaneView` decides it.
    pub fn gesture(&mut self, gesture: impl FnOnce(&mut PaneView) -> Effect) -> Option<Effect> {
        Some(self.screen.as_mut()?.gesture(gesture))
    }

    /// The mouse modes the shown pane's child asked for.
    pub fn mouse(&self) -> MouseTracking {
        self.screen.as_ref().map(|screen| screen.view().mouse()).unwrap_or_default()
    }

    /// The shown pane's text between two cells; empty with no pane.
    pub fn text(&self, from: (u16, u16), to: (u16, u16)) -> String {
        self.screen.as_ref().map(|screen| screen.text(from, to)).unwrap_or_default()
    }

    pub fn take_frame(&mut self) -> Option<Frame> {
        self.screen.as_mut()?.take_frame()
    }

    /// The page reloaded: the whole screen goes with the next frame.
    pub fn refresh(&mut self) {
        if let Some(screen) = &mut self.screen {
            screen.refresh();
        }
    }

    pub fn toggle_fill(&mut self) {
        self.filled = !self.filled;
    }

    pub fn filled(&self) -> bool {
        self.filled
    }

    /// The leader was pressed: the next key is the window's, while there is
    /// a pane to type the ones after into.
    pub fn await_leader(&mut self) {
        self.leader = self.pane.is_some();
    }

    pub fn leader_waiting(&self) -> bool {
        self.leader
    }

    /// Whether the leader was waiting on this key, which it then stops.
    pub fn take_leader(&mut self) -> bool {
        std::mem::take(&mut self.leader)
    }

    #[cfg(test)]
    pub fn view(&self) -> Option<&PaneView> {
        self.screen.as_ref().map(Screen::view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(msgs: &[ClientMsg]) -> Vec<String> {
        msgs.iter()
            .map(|m| match m {
                ClientMsg::Subscribe { pane } => format!("subscribe {pane}"),
                ClientMsg::Unsubscribe { pane } => format!("unsubscribe {pane}"),
                ClientMsg::Resize { pane, rows, cols } => format!("resize {pane} {rows}x{cols}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    fn sent(change: impl FnOnce(&mut Vec<ClientMsg>)) -> Vec<String> {
        let mut out = Vec::new();
        change(&mut out);
        names(&out)
    }

    #[test]
    fn showing_subscribes_then_sizes_and_another_unsubscribes_the_first() {
        let mut shown = ShownPane::default();
        assert!(sent(|out| shown.resize(30, 100, out)).is_empty(), "nothing shown to size");
        assert_eq!(sent(|out| { shown.show(PaneId(1), out); }), ["subscribe 1", "resize 1 30x100"]);
        assert!(!shown.show(PaneId(1), &mut Vec::new()), "already shown");
        assert_eq!(
            sent(|out| { shown.show(PaneId(2), out); }),
            ["unsubscribe 1", "subscribe 2", "resize 2 30x100"]
        );
    }

    #[test]
    fn a_resize_is_sent_only_when_the_size_changes() {
        let mut shown = ShownPane::default();
        shown.show(PaneId(1), &mut Vec::new());
        assert_eq!(sent(|out| shown.resize(30, 100, out)), ["resize 1 30x100"]);
        assert!(sent(|out| shown.resize(30, 100, out)).is_empty());
        assert_eq!(sent(|out| shown.resize(31, 100, out)), ["resize 1 31x100"]);
    }

    #[test]
    fn a_reconnect_subscribes_again_at_the_same_size() {
        let mut shown = ShownPane::default();
        shown.resize(30, 100, &mut Vec::new());
        shown.show(PaneId(1), &mut Vec::new());
        assert_eq!(sent(|out| shown.reconnected(out)), ["subscribe 1", "resize 1 30x100"]);
        shown.gone();
        assert!(sent(|out| shown.reconnected(out)).is_empty(), "nothing to come back to");
    }

    #[test]
    fn another_pane_comes_back_unfilled_and_without_the_leader() {
        let mut shown = ShownPane::default();
        shown.await_leader();
        assert!(!shown.leader_waiting(), "no pane to type into");
        shown.show(PaneId(1), &mut Vec::new());
        shown.toggle_fill();
        shown.await_leader();
        assert!(shown.leader_waiting());
        shown.show(PaneId(2), &mut Vec::new());
        assert!(!shown.filled() && !shown.leader_waiting());
    }
}
