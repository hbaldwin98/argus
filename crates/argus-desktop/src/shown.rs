//! The panes this window shows: one, or two side by side while the console
//! is split, each with its subscription, size and view on this connection;
//! which of them has the keys; whether the one shown fills the window; and
//! whether the leader is waiting on the next key typed.
//!
//! Which panes are shown is the window's own choice, as the terminal
//! client's selection is; the session checks the tree or the consoles hold
//! one before asking. The daemon sizes each pane on its own, so each side
//! has its own room and asks its own size, sent only when it changes on
//! this connection. A lone pane is always on the first side: that is the
//! canvas the Workspace and the console hand between them.

use argus_protocol::pane_view::{Effect, PaneView};
use argus_protocol::{ClientMsg, MouseTracking, PaneId, ServerMsg};

use crate::page::{Frame, Side};
use crate::screen::Screen;

#[derive(Default)]
struct Slot {
    pane: Option<PaneId>,
    screen: Option<Screen>,
    /// The rows and columns the page has room for on this side.
    capacity: Option<(u16, u16)>,
    /// The size last asked for this side's pane on this connection.
    sized: Option<(u16, u16)>,
}

impl Slot {
    fn subscribe(&mut self, pane: PaneId, out: &mut Vec<ClientMsg>) {
        self.pane = Some(pane);
        self.screen = Some(Screen::new(pane));
        self.sized = None;
        out.push(ClientMsg::Subscribe { pane });
        self.send_size(out);
    }

    /// Lets the pane go; `unsubscribe` is false when the daemon already
    /// has, with the tree or the consoles that dropped it.
    fn clear(&mut self, unsubscribe: bool, out: &mut Vec<ClientMsg>) {
        if let (Some(pane), true) = (self.pane.take(), unsubscribe) {
            out.push(ClientMsg::Unsubscribe { pane });
        }
        self.screen = None;
        self.sized = None;
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
}

#[derive(Default)]
pub struct ShownPane {
    /// The first side, then the second.
    sides: [Slot; 2],
    keys: Side,
    /// The person asked for the pane to fill the window.
    filled: bool,
    /// The leader was pressed in the pane: the next key is the window's.
    leader: bool,
}

fn at(side: Side) -> usize {
    match side {
        Side::First => 0,
        Side::Second => 1,
    }
}

pub fn other(side: Side) -> Side {
    match side {
        Side::First => Side::Second,
        Side::Second => Side::First,
    }
}

impl ShownPane {
    /// The pane with the keys.
    pub fn pane(&self) -> Option<PaneId> {
        self.on(self.keys)
    }

    pub fn on(&self, side: Side) -> Option<PaneId> {
        self.sides[at(side)].pane
    }

    pub fn keys(&self) -> Side {
        self.keys
    }

    /// Every pane on screen, the first side's first.
    pub fn panes(&self) -> Vec<PaneId> {
        self.sides.iter().filter_map(|slot| slot.pane).collect()
    }

    fn side_of(&self, pane: PaneId) -> Option<Side> {
        [Side::First, Side::Second].into_iter().find(|&side| self.on(side) == Some(pane))
    }

    /// Shows `pane` alone, with the keys, letting any other go. False when
    /// it already was the one shown.
    pub fn show(&mut self, pane: PaneId, out: &mut Vec<ClientMsg>) -> bool {
        let split = self.on(Side::Second).is_some();
        let changed = self.show_on(Side::First, pane, out);
        self.close(Side::Second, out);
        changed || split
    }

    /// Shows `pane` on `side`, letting the one there go. A pane already on
    /// the other side moves over with its screen rather than subscribing
    /// twice. False when it was there already.
    pub fn show_on(&mut self, side: Side, pane: PaneId, out: &mut Vec<ClientMsg>) -> bool {
        if self.on(side) == Some(pane) {
            return false;
        }
        // As in the terminal client, filling and the leader are for the
        // pane they were asked on.
        self.filled = false;
        self.leader = false;
        self.sides[at(side)].clear(true, out);
        if self.on(other(side)) == Some(pane) {
            self.move_over(other(side), side, out);
        } else {
            self.sides[at(side)].subscribe(pane, out);
        }
        true
    }

    /// The pane on `from` goes to `to`'s room: drawn whole there, and
    /// sized to it.
    fn move_over(&mut self, from: Side, to: Side, out: &mut Vec<ClientMsg>) {
        let from = &mut self.sides[at(from)];
        let (pane, screen) = (from.pane.take(), from.screen.take());
        from.sized = None;
        let to = &mut self.sides[at(to)];
        to.pane = pane;
        to.screen = screen;
        if let Some(screen) = &mut to.screen {
            screen.refresh();
        }
        to.sized = None;
        to.send_size(out);
    }

    /// Lets `side`'s pane go. When the first side goes the second's pane
    /// moves over to it, and either way the first has the keys.
    pub fn close(&mut self, side: Side, out: &mut Vec<ClientMsg>) {
        self.drop_side(side, true, out);
    }

    fn drop_side(&mut self, side: Side, unsubscribe: bool, out: &mut Vec<ClientMsg>) {
        if self.on(side).is_none() {
            return;
        }
        self.sides[at(side)].clear(unsubscribe, out);
        if side == Side::First && self.on(Side::Second).is_some() {
            self.move_over(Side::Second, Side::First, out);
        }
        self.keys = Side::First;
        self.filled = false;
        self.leader = false;
    }

    /// Gives the keys to the side showing `pane`; false when neither does.
    pub fn focus(&mut self, pane: PaneId) -> bool {
        let Some(side) = self.side_of(pane) else {
            return false;
        };
        if self.keys != side {
            self.keys = side;
            self.leader = false;
        }
        true
    }

    /// Shows nothing. As in the terminal client, filling is for the pane it
    /// was asked on: another pane comes back with the rail beside it.
    pub fn hide(&mut self, out: &mut Vec<ClientMsg>) {
        for slot in &mut self.sides {
            slot.clear(true, out);
        }
        self.keys = Side::First;
        self.filled = false;
        self.leader = false;
    }

    /// The tree or the consoles no longer hold `pane`, and the daemon's
    /// subscriptions let it go with it: nothing to unsubscribe.
    pub fn gone(&mut self, pane: PaneId, out: &mut Vec<ClientMsg>) {
        if let Some(side) = self.side_of(pane) {
            self.drop_side(side, false, out);
        }
    }

    /// The room the page has on `side`. While another client is the one
    /// the person was last at the grid may come back another size, and the
    /// page pads or crops it.
    pub fn resize(&mut self, side: Side, rows: u16, cols: u16, out: &mut Vec<ClientMsg>) {
        let slot = &mut self.sides[at(side)];
        slot.capacity = Some((rows.max(1), cols.max(1)));
        slot.send_size(out);
    }

    /// A new connection knows nothing this window asked of the last: each
    /// pane is subscribed to again, at its size, on a fresh screen.
    pub fn reconnected(&mut self, out: &mut Vec<ClientMsg>) {
        for slot in &mut self.sides {
            if let Some(pane) = slot.pane {
                slot.subscribe(pane, out);
            }
        }
    }

    /// A message about a shown pane's view; `None` for anything else.
    pub fn receive(&mut self, msg: &ServerMsg) -> Option<Effect> {
        self.sides.iter_mut().find_map(|slot| slot.screen.as_mut()?.receive(msg))
    }

    fn keyed(&self) -> Option<&Screen> {
        self.sides[at(self.keys)].screen.as_ref()
    }

    /// A gesture on the view of the pane with the keys, as `PaneView`
    /// decides it.
    pub fn gesture(&mut self, gesture: impl FnOnce(&mut PaneView) -> Effect) -> Option<Effect> {
        Some(self.sides[at(self.keys)].screen.as_mut()?.gesture(gesture))
    }

    /// A gesture on a shown pane's view, whichever side it is on: the wheel
    /// scrolls the pane under the pointer, keys or not.
    pub fn gesture_on(&mut self, pane: PaneId, gesture: impl FnOnce(&mut PaneView) -> Effect) -> Option<Effect> {
        let side = self.side_of(pane)?;
        Some(self.sides[at(side)].screen.as_mut()?.gesture(gesture))
    }

    /// The mouse modes the keyed pane's child asked for.
    pub fn mouse(&self) -> MouseTracking {
        self.keyed().map(|screen| screen.view().mouse()).unwrap_or_default()
    }

    /// The keyed pane's text between two cells; empty with no pane.
    pub fn text(&self, from: (u16, u16), to: (u16, u16)) -> String {
        self.keyed().map(|screen| screen.text(from, to)).unwrap_or_default()
    }

    /// Each side's changed rows, named for the side that draws them.
    pub fn take_frames(&mut self) -> Vec<Frame> {
        let sides = [Side::First, Side::Second].into_iter().zip(&mut self.sides);
        sides
            .filter_map(|(side, slot)| Some(Frame { side, ..slot.screen.as_mut()?.take_frame()? }))
            .collect()
    }

    /// The page reloaded: every screen goes whole with the next frames.
    pub fn refresh(&mut self) {
        for screen in self.sides.iter_mut().filter_map(|slot| slot.screen.as_mut()) {
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
        self.leader = self.pane().is_some();
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
        self.keyed().map(Screen::view)
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
        assert!(sent(|out| shown.resize(Side::First, 30, 100, out)).is_empty(), "nothing shown to size");
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
        assert_eq!(sent(|out| shown.resize(Side::First, 30, 100, out)), ["resize 1 30x100"]);
        assert!(sent(|out| shown.resize(Side::First, 30, 100, out)).is_empty());
        assert_eq!(sent(|out| shown.resize(Side::First, 31, 100, out)), ["resize 1 31x100"]);
    }

    #[test]
    fn a_reconnect_subscribes_again_at_the_same_size() {
        let mut shown = ShownPane::default();
        shown.resize(Side::First, 30, 100, &mut Vec::new());
        shown.show(PaneId(1), &mut Vec::new());
        assert_eq!(sent(|out| shown.reconnected(out)), ["subscribe 1", "resize 1 30x100"]);
        shown.gone(PaneId(1), &mut Vec::new());
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

    /// Pane 1 on the first side at 30x50 and pane 2 on the second at
    /// 30x49, the first with the keys.
    fn split(shown: &mut ShownPane) {
        let mut out = Vec::new();
        shown.resize(Side::First, 30, 50, &mut out);
        shown.resize(Side::Second, 30, 49, &mut out);
        shown.show(PaneId(1), &mut out);
        shown.show_on(Side::Second, PaneId(2), &mut out);
    }

    #[test]
    fn each_side_is_sized_to_its_own_room() {
        let mut shown = ShownPane::default();
        shown.resize(Side::First, 30, 100, &mut Vec::new());
        shown.show(PaneId(1), &mut Vec::new());
        assert_eq!(sent(|out| shown.resize(Side::First, 30, 50, out)), ["resize 1 30x50"]);
        assert!(sent(|out| shown.resize(Side::Second, 30, 49, out)).is_empty(), "nothing beside yet");
        assert_eq!(sent(|out| { shown.show_on(Side::Second, PaneId(2), out); }), ["subscribe 2", "resize 2 30x49"]);
        assert_eq!(sent(|out| shown.resize(Side::Second, 20, 49, out)), ["resize 2 20x49"], "the first is left alone");
        assert_eq!(shown.panes(), [PaneId(1), PaneId(2)]);
    }

    #[test]
    fn the_keys_follow_the_side_they_are_given_to() {
        let mut shown = ShownPane::default();
        split(&mut shown);
        assert_eq!(shown.pane(), Some(PaneId(1)), "showing beside does not take the keys");
        assert!(shown.focus(PaneId(2)));
        assert_eq!((shown.keys(), shown.pane()), (Side::Second, Some(PaneId(2))));
        assert!(!shown.focus(PaneId(9)), "on neither side");
        assert_eq!(shown.pane(), Some(PaneId(2)));
    }

    #[test]
    fn the_first_side_going_moves_the_second_over_at_its_size() {
        let mut shown = ShownPane::default();
        split(&mut shown);
        shown.focus(PaneId(2));
        assert_eq!(sent(|out| shown.close(Side::First, out)), ["unsubscribe 1", "resize 2 30x50"]);
        assert_eq!((shown.panes(), shown.keys()), (vec![PaneId(2)], Side::First));

        let mut shown = ShownPane::default();
        split(&mut shown);
        assert!(sent(|out| shown.gone(PaneId(2), out)).is_empty(), "the daemon let it go");
        assert_eq!(shown.panes(), [PaneId(1)]);
    }

    #[test]
    fn showing_one_pane_puts_the_split_away() {
        let mut shown = ShownPane::default();
        split(&mut shown);
        assert_eq!(
            sent(|out| { shown.show(PaneId(3), out); }),
            ["unsubscribe 1", "subscribe 3", "resize 3 30x50", "unsubscribe 2"]
        );
        let mut shown = ShownPane::default();
        split(&mut shown);
        assert_eq!(sent(|out| { shown.show(PaneId(2), out); }), ["unsubscribe 1", "resize 2 30x50"], "moved, not subscribed again");
    }
}
