//! The panes this window shows — the Workspace's, the console's one or
//! two sides, and a screen for each extension console view on screen —
//! each with its subscription, size and view on this
//! connection; which of them has the keys; whether the Workspace's fills
//! the window; and whether the leader is waiting on the next key typed.
//!
//! Which panes are shown is the window's own choice, as the terminal
//! client's selection is; the session checks the tree or the consoles hold
//! one before asking. The daemon sizes each pane on its own, so each side
//! has its own room and asks its own size, sent only when it changes on
//! this connection. The Workspace's side never lends its screen to the
//! console: an agent and a docked console are both on screen, and the keys
//! are handed between them.

use argus_protocol::pane_view::{Effect, PaneView};
use argus_protocol::{ClientMsg, KeyboardMode, MouseTracking, PaneId, ServerMsg};

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

/// An extension's console view's screen. The daemon owns its pane, so the
/// window follows what the view says rather than choosing it.
struct ViewSlot {
    /// The view's panel name, `extension/view`.
    name: String,
    slot: Slot,
}

#[derive(Default)]
pub struct ShownPane {
    /// The Workspace's, then the console's first and second.
    sides: [Slot; 3],
    /// Extension console views' screens, as many as are on screen. A
    /// `Side::View(n)` is the `n`th, and a freed one is reused, so a side
    /// the page still names is never another view's for long.
    views: Vec<Option<ViewSlot>>,
    keys: Side,
    /// The person asked for the Workspace's pane to fill the window.
    filled: bool,
    /// The leader was pressed in the pane: the next key is the window's.
    leader: bool,
}

const SIDES: [Side; 3] = [Side::Workspace, Side::First, Side::Second];

/// The index of one of the three fixed sides. A view's side has no place
/// among them: it is given and taken only by `sync_views`.
fn at(side: Side) -> usize {
    match side {
        Side::Workspace => 0,
        Side::First => 1,
        Side::Second => 2,
        Side::View(_) => unreachable!("a view's side is kept by sync_views"),
    }
}

impl ShownPane {
    /// The pane with the keys.
    pub fn pane(&self) -> Option<PaneId> {
        self.on(self.keys)
    }

    pub fn on(&self, side: Side) -> Option<PaneId> {
        self.slot(side)?.pane
    }

    fn slot(&self, side: Side) -> Option<&Slot> {
        match side {
            Side::View(n) => self.views.get(usize::from(n))?.as_ref().map(|v| &v.slot),
            fixed => Some(&self.sides[at(fixed)]),
        }
    }

    fn slot_mut(&mut self, side: Side) -> Option<&mut Slot> {
        match side {
            Side::View(n) => self.views.get_mut(usize::from(n))?.as_mut().map(|v| &mut v.slot),
            fixed => Some(&mut self.sides[at(fixed)]),
        }
    }

    /// Every side with its slot, the Workspace's first.
    fn each(&mut self) -> impl Iterator<Item = (Side, &mut Slot)> {
        let [workspace, first, second] = &mut self.sides;
        let views = self.views.iter_mut().enumerate().filter_map(|(n, v)| Some((Side::View(n as u8), &mut v.as_mut()?.slot)));
        [(Side::Workspace, workspace), (Side::First, first), (Side::Second, second)].into_iter().chain(views)
    }

    pub fn keys(&self) -> Side {
        self.keys
    }

    /// Every pane the window chose to show, the Workspace's first. An
    /// extension console's is not among them: the daemon holds it with its
    /// view, so the tree and the consoles never say whether it is there.
    pub fn panes(&self) -> Vec<PaneId> {
        self.sides.iter().filter_map(|slot| slot.pane).collect()
    }

    fn side_of(&self, pane: PaneId) -> Option<Side> {
        let views = (0..self.views.len()).map(|n| Side::View(n as u8));
        SIDES.into_iter().chain(views).find(|&side| self.on(side) == Some(pane))
    }

    /// The extension console views' screens, in the order they were given.
    pub fn view_sides(&self) -> Vec<Side> {
        (0..self.views.len()).map(|n| Side::View(n as u8)).filter(|&side| self.slot(side).is_some()).collect()
    }

    /// The screen of the console view `name`, while it has one.
    pub fn view_side(&self, name: &str) -> Option<Side> {
        let n = self.views.iter().position(|v| v.as_ref().is_some_and(|v| v.name == name))?;
        Some(Side::View(n as u8))
    }

    /// Makes the extension console views on screen the ones in `wanted`,
    /// each by its name and the pane it runs in, once it has one. A view
    /// that went off screen lets its pane go; one whose pane changed — the
    /// extension published it again after it ended — subscribes the new one.
    pub fn sync_views(&mut self, wanted: &[(String, Option<PaneId>)], out: &mut Vec<ClientMsg>) {
        for entry in &mut self.views {
            if entry.as_ref().is_some_and(|v| !wanted.iter().any(|(name, _)| *name == v.name)) {
                if let Some(mut gone) = entry.take() {
                    gone.slot.clear(true, out);
                }
            }
        }
        for (name, pane) in wanted {
            let at = match self.views.iter().position(|v| v.as_ref().is_some_and(|v| v.name == *name)) {
                Some(at) => at,
                None => {
                    let at = self.views.iter().position(Option::is_none).unwrap_or(self.views.len());
                    let slot = ViewSlot { name: name.clone(), slot: Slot::default() };
                    match self.views.get_mut(at) {
                        Some(free) => *free = Some(slot),
                        None => self.views.push(Some(slot)),
                    }
                    at
                }
            };
            let Some(view) = self.views[at].as_mut() else { continue };
            if view.slot.pane != *pane {
                view.slot.clear(true, out);
                if let Some(pane) = *pane {
                    view.slot.subscribe(pane, out);
                }
            }
        }
        while self.views.last().is_some_and(Option::is_none) {
            self.views.pop();
        }
        self.settle_keys();
    }

    /// Whether `side` still has a pane to type into, and if not, hands the
    /// keys on: to the console's first side, or else the Workspace's.
    fn settle_keys(&mut self) {
        if self.pane().is_none() {
            self.keys = [Side::First, Side::Workspace].into_iter().find(|&s| self.on(s).is_some()).unwrap_or_default();
            self.leader = false;
        }
    }

    /// Draws `side` whole with the next frame: the page just made its
    /// screen, or gave it another side, and has none of the rows.
    pub fn refresh_side(&mut self, side: Side) {
        if let Some(screen) = self.slot_mut(side).and_then(|slot| slot.screen.as_mut()) {
            screen.refresh();
        }
    }

    /// Shows `pane` on `side`, letting the one there go. A pane already on
    /// the console's other side moves over with its screen rather than
    /// subscribing twice. False when it was there already.
    pub fn show_on(&mut self, side: Side, pane: PaneId, out: &mut Vec<ClientMsg>) -> bool {
        if self.on(side) == Some(pane) {
            return false;
        }
        // As in the terminal client, filling and the leader are for the
        // pane they were asked on.
        if side == Side::Workspace {
            self.filled = false;
        }
        if side == self.keys {
            self.leader = false;
        }
        self.sides[at(side)].clear(true, out);
        match self.side_of(pane) {
            Some(from) => self.move_over(from, side, out),
            None => self.sides[at(side)].subscribe(pane, out),
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

    /// Lets `side`'s pane go. When the console's first side goes its second
    /// moves over to it. Keys left on nothing go to the console's first
    /// side, or else the Workspace's.
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
            if self.keys == Side::Second {
                self.keys = Side::First;
            }
        }
        if side == Side::Workspace {
            self.filled = false;
        }
        self.settle_keys();
    }

    /// Gives the keys to the side showing `pane`; false when none does.
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
        // A side the page names that is gone: it measured before it heard.
        let Some(slot) = self.slot_mut(side) else { return };
        slot.capacity = Some((rows.max(1), cols.max(1)));
        slot.send_size(out);
    }

    /// A new connection knows nothing this window asked of the last: each
    /// pane is subscribed to again, at its size, on a fresh screen. A
    /// console view's pane was the last daemon's, and the new one names its
    /// own with the views it publishes, so those screens go until it does.
    pub fn reconnected(&mut self, out: &mut Vec<ClientMsg>) {
        self.views.clear();
        for slot in &mut self.sides {
            if let Some(pane) = slot.pane {
                slot.subscribe(pane, out);
            }
        }
        self.settle_keys();
    }

    /// A message about a shown pane's view; `None` for anything else.
    pub fn receive(&mut self, msg: &ServerMsg) -> Option<Effect> {
        self.each().find_map(|(_, slot)| slot.screen.as_mut()?.receive(msg))
    }

    fn keyed(&self) -> Option<&Screen> {
        self.slot(self.keys)?.screen.as_ref()
    }

    /// A gesture on the view of the pane with the keys, as `PaneView`
    /// decides it.
    pub fn gesture(&mut self, gesture: impl FnOnce(&mut PaneView) -> Effect) -> Option<Effect> {
        Some(self.slot_mut(self.keys)?.screen.as_mut()?.gesture(gesture))
    }

    /// A gesture on a shown pane's view, whichever side it is on: the wheel
    /// scrolls the pane under the pointer, keys or not.
    pub fn gesture_on(&mut self, pane: PaneId, gesture: impl FnOnce(&mut PaneView) -> Effect) -> Option<Effect> {
        let side = self.side_of(pane)?;
        Some(self.slot_mut(side)?.screen.as_mut()?.gesture(gesture))
    }

    /// The mouse modes the keyed pane's child asked for.
    pub fn mouse(&self) -> MouseTracking {
        self.keyed().map(|screen| screen.view().mouse()).unwrap_or_default()
    }

    /// The kitty keyboard flags the keyed pane's child has pushed.
    pub fn keyboard(&self) -> KeyboardMode {
        self.keyed().map(|screen| screen.view().keyboard()).unwrap_or_default()
    }

    /// The keyed pane's text between two cells; empty with no pane.
    pub fn text(&self, from: (u16, u16), to: (u16, u16)) -> String {
        self.keyed().map(|screen| screen.text(from, to)).unwrap_or_default()
    }

    /// Each side's changed rows, named for the side that draws them.
    pub fn take_frames(&mut self) -> Vec<Frame> {
        self.each().filter_map(|(side, slot)| Some(Frame { side, ..slot.screen.as_mut()?.take_frame()? })).collect()
    }

    /// The page reloaded: every screen goes whole with the next frames.
    pub fn refresh(&mut self) {
        for screen in self.each().filter_map(|(_, slot)| slot.screen.as_mut()) {
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
        assert!(sent(|out| shown.resize(Side::Workspace, 30, 100, out)).is_empty(), "nothing shown to size");
        assert_eq!(sent(|out| { shown.show_on(Side::Workspace, PaneId(1), out); }), ["subscribe 1", "resize 1 30x100"]);
        assert!(!shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new()), "already shown");
        assert_eq!(
            sent(|out| { shown.show_on(Side::Workspace, PaneId(2), out); }),
            ["unsubscribe 1", "subscribe 2", "resize 2 30x100"]
        );
    }

    #[test]
    fn a_resize_is_sent_only_when_the_size_changes() {
        let mut shown = ShownPane::default();
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        assert_eq!(sent(|out| shown.resize(Side::Workspace, 30, 100, out)), ["resize 1 30x100"]);
        assert!(sent(|out| shown.resize(Side::Workspace, 30, 100, out)).is_empty());
        assert_eq!(sent(|out| shown.resize(Side::Workspace, 31, 100, out)), ["resize 1 31x100"]);
    }

    #[test]
    fn a_reconnect_subscribes_again_at_the_same_size() {
        let mut shown = ShownPane::default();
        shown.resize(Side::Workspace, 30, 100, &mut Vec::new());
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        assert_eq!(sent(|out| shown.reconnected(out)), ["subscribe 1", "resize 1 30x100"]);
        shown.gone(PaneId(1), &mut Vec::new());
        assert!(sent(|out| shown.reconnected(out)).is_empty(), "nothing to come back to");
    }

    #[test]
    fn another_pane_comes_back_unfilled_and_without_the_leader() {
        let mut shown = ShownPane::default();
        shown.await_leader();
        assert!(!shown.leader_waiting(), "no pane to type into");
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        shown.toggle_fill();
        shown.await_leader();
        assert!(shown.leader_waiting());
        shown.show_on(Side::Workspace, PaneId(2), &mut Vec::new());
        assert!(!shown.filled() && !shown.leader_waiting());
    }

    /// Pane 1 on the Workspace at 20x100, and the console split with pane
    /// 2 on its first side at 10x50 and pane 3 on its second at 10x49; the
    /// Workspace has the keys.
    fn split(shown: &mut ShownPane) {
        let mut out = Vec::new();
        shown.resize(Side::Workspace, 20, 100, &mut out);
        shown.resize(Side::First, 10, 50, &mut out);
        shown.resize(Side::Second, 10, 49, &mut out);
        shown.show_on(Side::Workspace, PaneId(1), &mut out);
        shown.show_on(Side::First, PaneId(2), &mut out);
        shown.show_on(Side::Second, PaneId(3), &mut out);
    }

    #[test]
    fn the_workspace_and_the_console_are_on_screen_at_once_each_at_its_size() {
        let mut shown = ShownPane::default();
        shown.resize(Side::Workspace, 20, 100, &mut Vec::new());
        shown.resize(Side::First, 10, 100, &mut Vec::new());
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        assert_eq!(sent(|out| { shown.show_on(Side::First, PaneId(2), out); }), ["subscribe 2", "resize 2 10x100"], "the agent stays");
        assert_eq!(sent(|out| shown.resize(Side::First, 10, 50, out)), ["resize 2 10x50"]);
        assert!(sent(|out| shown.resize(Side::Second, 10, 49, out)).is_empty(), "nothing beside yet");
        assert_eq!(sent(|out| { shown.show_on(Side::Second, PaneId(3), out); }), ["subscribe 3", "resize 3 10x49"]);
        assert_eq!(shown.panes(), [PaneId(1), PaneId(2), PaneId(3)]);
        assert_eq!(shown.pane(), Some(PaneId(1)), "showing does not take the keys");
    }

    #[test]
    fn the_keys_follow_the_side_they_are_given_to() {
        let mut shown = ShownPane::default();
        split(&mut shown);
        assert!(shown.focus(PaneId(3)));
        assert_eq!((shown.keys(), shown.pane()), (Side::Second, Some(PaneId(3))));
        assert!(!shown.focus(PaneId(9)), "on no side");
        assert_eq!(shown.pane(), Some(PaneId(3)));
        assert!(shown.focus(PaneId(1)));
        assert_eq!(shown.keys(), Side::Workspace);
    }

    #[test]
    fn the_first_side_going_moves_the_second_over_at_its_size_with_its_keys() {
        let mut shown = ShownPane::default();
        split(&mut shown);
        shown.focus(PaneId(3));
        assert_eq!(sent(|out| shown.close(Side::First, out)), ["unsubscribe 2", "resize 3 10x50"]);
        assert_eq!((shown.panes(), shown.pane()), (vec![PaneId(1), PaneId(3)], Some(PaneId(3))));

        let mut shown = ShownPane::default();
        split(&mut shown);
        assert!(sent(|out| shown.gone(PaneId(3), out)).is_empty(), "the daemon let it go");
        assert_eq!((shown.panes(), shown.pane()), (vec![PaneId(1), PaneId(2)], Some(PaneId(1))), "the Workspace kept its keys");
    }

    #[test]
    fn keys_left_on_nothing_go_to_the_console_or_else_the_workspace() {
        let mut shown = ShownPane::default();
        split(&mut shown);
        shown.close(Side::Workspace, &mut Vec::new());
        assert_eq!(shown.pane(), Some(PaneId(2)));
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        shown.close(Side::Second, &mut Vec::new());
        shown.close(Side::First, &mut Vec::new());
        assert_eq!(shown.pane(), Some(PaneId(1)));
    }

    #[test]
    fn a_console_chosen_for_the_other_side_moves_rather_than_subscribing_again() {
        let mut shown = ShownPane::default();
        split(&mut shown);
        assert_eq!(sent(|out| { shown.show_on(Side::First, PaneId(3), out); }), ["unsubscribe 2", "resize 3 10x50"]);
        assert_eq!(shown.panes(), [PaneId(1), PaneId(3)]);
    }

    fn wanted(views: &[(&str, Option<u64>)]) -> Vec<(String, Option<PaneId>)> {
        views.iter().map(|&(name, pane)| (name.to_string(), pane.map(PaneId))).collect()
    }

    #[test]
    fn a_console_view_on_screen_subscribes_at_its_own_size_and_off_screen_lets_go() {
        let mut shown = ShownPane::default();
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        let at = |shown: &ShownPane| shown.view_side("ops/dev");
        assert!(sent(|out| shown.sync_views(&wanted(&[("ops/dev", None)]), out)).is_empty(), "no pane yet: only a screen");
        let side = at(&shown).unwrap();
        assert!(matches!(side, Side::View(_)));
        assert!(sent(|out| shown.resize(side, 12, 70, out)).is_empty(), "nothing to size");
        assert_eq!(sent(|out| shown.sync_views(&wanted(&[("ops/dev", Some(7))]), out)), ["subscribe 7", "resize 7 12x70"]);
        assert!(sent(|out| shown.sync_views(&wanted(&[("ops/dev", Some(7))]), out)).is_empty(), "again changes nothing");
        assert_eq!(shown.on(side), Some(PaneId(7)));
        assert_eq!(shown.panes(), [PaneId(1)], "the tree and the consoles do not hold it");
        assert_eq!(sent(|out| shown.sync_views(&[], out)), ["unsubscribe 7"]);
        assert_eq!(at(&shown), None);
    }

    #[test]
    fn a_console_view_published_again_with_a_new_pane_subscribes_the_new_one() {
        let mut shown = ShownPane::default();
        shown.sync_views(&wanted(&[("ops/dev", Some(7))]), &mut Vec::new());
        let side = shown.view_side("ops/dev").unwrap();
        shown.resize(side, 12, 70, &mut Vec::new());
        assert_eq!(
            sent(|out| shown.sync_views(&wanted(&[("ops/dev", Some(8))]), out)),
            ["unsubscribe 7", "subscribe 8", "resize 8 12x70"]
        );
        assert_eq!(shown.view_side("ops/dev"), Some(side), "the same screen");
    }

    #[test]
    fn each_console_view_has_a_screen_and_a_freed_one_is_reused() {
        let mut shown = ShownPane::default();
        shown.sync_views(&wanted(&[("a/one", Some(5)), ("a/two", Some(6))]), &mut Vec::new());
        let (one, two) = (shown.view_side("a/one").unwrap(), shown.view_side("a/two").unwrap());
        assert_ne!(one, two);
        shown.sync_views(&wanted(&[("a/two", Some(6))]), &mut Vec::new());
        assert_eq!((shown.view_side("a/two"), shown.on(one)), (Some(two), None));
        shown.sync_views(&wanted(&[("a/two", Some(6)), ("a/three", Some(9))]), &mut Vec::new());
        assert_eq!(shown.view_side("a/three"), Some(one));
    }

    #[test]
    fn the_keys_go_to_a_console_view_and_come_back_when_it_leaves() {
        let mut shown = ShownPane::default();
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        shown.sync_views(&wanted(&[("ops/dev", Some(7))]), &mut Vec::new());
        assert!(shown.focus(PaneId(7)));
        assert_eq!(shown.pane(), Some(PaneId(7)));
        assert_eq!(shown.keys(), shown.view_side("ops/dev").unwrap());
        shown.await_leader();
        assert!(shown.leader_waiting());
        shown.sync_views(&[], &mut Vec::new());
        assert_eq!((shown.keys(), shown.pane(), shown.leader_waiting()), (Side::Workspace, Some(PaneId(1)), false));
    }

    #[test]
    fn a_reconnect_lets_the_console_views_go_until_the_new_daemon_names_theirs() {
        let mut shown = ShownPane::default();
        shown.resize(Side::Workspace, 30, 100, &mut Vec::new());
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        shown.sync_views(&wanted(&[("ops/dev", Some(7))]), &mut Vec::new());
        shown.focus(PaneId(7));
        assert_eq!(sent(|out| shown.reconnected(out)), ["subscribe 1", "resize 1 30x100"], "not the old daemon's pane");
        assert_eq!((shown.view_side("ops/dev"), shown.pane()), (None, Some(PaneId(1))));
        assert_eq!(sent(|out| shown.sync_views(&wanted(&[("ops/dev", Some(3))]), out)), ["subscribe 3"]);
    }

    #[test]
    fn a_size_for_a_side_that_went_is_ignored() {
        let mut shown = ShownPane::default();
        assert!(sent(|out| shown.resize(Side::View(4), 10, 10, out)).is_empty());
    }

    #[test]
    fn a_console_shown_or_closed_leaves_the_workspace_filled() {
        let mut shown = ShownPane::default();
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        shown.toggle_fill();
        shown.show_on(Side::First, PaneId(2), &mut Vec::new());
        shown.close(Side::First, &mut Vec::new());
        assert!(shown.filled());
    }
}
