//! The panes this window shows — the Workspace's grid (`workspace_grid`),
//! the console's one or two sides, and a screen for each extension console
//! view on screen — each with its subscription, size and view on this
//! connection; which of them has the keys; whether the Workspace's pane
//! fills the window; and whether the leader is waiting on the next key typed.
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
use crate::split_grid::Dir;

pub mod workspace_grid;

use workspace_grid::{Refusal, WorkspaceGrid};

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
    /// The Workspace's pane and those split beside it. Kept here and not
    /// with the stage, so the grid survives a visit to another stage.
    grid: WorkspaceGrid,
    /// The console's first and second sides.
    console: [Slot; 2],
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
            Side::Workspace | Side::Beside(_) => self.grid.cell(side),
            Side::First => Some(&self.console[0]),
            Side::Second => Some(&self.console[1]),
            Side::View(n) => self.views.get(usize::from(n))?.as_ref().map(|v| &v.slot),
        }
    }

    fn slot_mut(&mut self, side: Side) -> Option<&mut Slot> {
        match side {
            Side::Workspace | Side::Beside(_) => self.grid.cell_mut(side),
            Side::First => Some(&mut self.console[0]),
            Side::Second => Some(&mut self.console[1]),
            Side::View(n) => self.views.get_mut(usize::from(n))?.as_mut().map(|v| &mut v.slot),
        }
    }

    /// Every side with its slot: the grid's in opening order, then the
    /// console's, then the views'.
    fn each(&mut self) -> impl Iterator<Item = (Side, &mut Slot)> {
        let [first, second] = &mut self.console;
        let views = self.views.iter_mut().enumerate().filter_map(|(n, v)| Some((Side::View(n as u8), &mut v.as_mut()?.slot)));
        self.grid.cells_mut().chain([(Side::First, first), (Side::Second, second)]).chain(views)
    }

    pub fn keys(&self) -> Side {
        self.keys
    }

    /// Every pane the window chose to show: the grid's, the Workspace's
    /// first, then the console's. An extension console's is not among them:
    /// the daemon holds it with its view, so the tree and the consoles never
    /// say whether it is there.
    pub fn panes(&self) -> Vec<PaneId> {
        self.grid.panes().chain(self.console.iter().filter_map(|slot| slot.pane)).collect()
    }

    fn side_of(&self, pane: PaneId) -> Option<Side> {
        let views = (0..self.views.len()).map(|n| Side::View(n as u8));
        self.grid.side_of(pane).or_else(|| [Side::First, Side::Second].into_iter().chain(views).find(|&side| self.on(side) == Some(pane)))
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
    /// keys on: to the console's first side, or else the Workspace's grid.
    /// Keys on a split that went were handed left as the grid closed it.
    fn settle_keys(&mut self) {
        if self.pane().is_none() {
            let mut sides = [Side::First, Side::Workspace].into_iter().chain(self.grid.sides());
            self.keys = sides.find(|&s| self.on(s).is_some()).unwrap_or_default();
            self.leader = false;
        }
    }

    /// A grid cell may have been emptied: the grid takes it out and
    /// numbers the rest down, and keys left on nothing are handed on.
    fn vacated(&mut self, out: &mut Vec<ClientMsg>) {
        self.grid.close_empty(&mut self.keys, out);
        self.settle_keys();
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
    /// subscribing twice. False when it was there already, and for a
    /// split's side: a split is made only by `open_beside`, so the side a
    /// caller names is never renumbered under it.
    pub fn show_on(&mut self, side: Side, pane: PaneId, out: &mut Vec<ClientMsg>) -> bool {
        if matches!(side, Side::Beside(_)) || self.on(side) == Some(pane) {
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
        let Some(slot) = self.slot_mut(side) else { return false };
        slot.clear(true, out);
        match self.side_of(pane) {
            Some(from) => self.move_over(from, side, out),
            None => {
                if let Some(slot) = self.slot_mut(side) {
                    slot.subscribe(pane, out);
                }
            }
        }
        self.vacated(out);
        true
    }

    /// The pane on `from` goes to `to`'s room: drawn whole there, and
    /// sized to it.
    fn move_over(&mut self, from: Side, to: Side, out: &mut Vec<ClientMsg>) {
        let Some(from) = self.slot_mut(from) else { return };
        let (pane, screen) = (from.pane.take(), from.screen.take());
        from.sized = None;
        let Some(to) = self.slot_mut(to) else { return };
        to.pane = pane;
        to.screen = screen;
        if let Some(screen) = &mut to.screen {
            screen.refresh();
        }
        to.sized = None;
        to.send_size(out);
    }

    /// Lets `side`'s pane go. When the console's first side goes its second
    /// moves over to it, and when the Workspace's goes the first split
    /// takes its place. Keys left on nothing go to the console's first
    /// side, or else the Workspace's.
    pub fn close(&mut self, side: Side, out: &mut Vec<ClientMsg>) {
        self.drop_side(side, true, out);
    }

    fn drop_side(&mut self, side: Side, unsubscribe: bool, out: &mut Vec<ClientMsg>) {
        if self.on(side).is_none() {
            return;
        }
        if let Some(slot) = self.slot_mut(side) {
            slot.clear(unsubscribe, out);
        }
        if side == Side::First && self.on(Side::Second).is_some() {
            self.move_over(Side::Second, Side::First, out);
            if self.keys == Side::Second {
                self.keys = Side::First;
            }
        }
        if side == Side::Workspace {
            self.filled = false;
        }
        self.vacated(out);
    }

    pub fn grid(&self) -> &WorkspaceGrid {
        &self.grid
    }

    /// The grid to arrange: only its layout-shaping half is the session's.
    pub fn grid_mut(&mut self) -> &mut WorkspaceGrid {
        &mut self.grid
    }

    /// Splits `pane` off into the grid; nothing is subscribed on a refusal.
    /// A pane on the console is refused before the grid is asked, so it is
    /// never subscribed twice.
    pub fn open_beside(&mut self, pane: PaneId, out: &mut Vec<ClientMsg>) -> Result<Side, Refusal> {
        if self.side_of(pane).is_some_and(|side| !matches!(side, Side::Workspace | Side::Beside(_))) {
            return Err(Refusal::OnConsole);
        }
        self.grid.join(pane, out)
    }

    /// Takes the split on `side` out of the grid; its pane keeps running.
    /// False for any side but a split's: the Workspace's own pane is not
    /// taken out of its grid.
    pub fn close_split(&mut self, side: Side, out: &mut Vec<ClientMsg>) -> bool {
        if !matches!(side, Side::Beside(_)) || self.on(side).is_none() {
            return false;
        }
        self.close(side, out);
        true
    }

    /// The pane with the keys trades places with its neighbour in `dir`.
    /// False when the keys are not on the grid.
    pub fn step_pane(&mut self, dir: Dir) -> bool {
        self.grid.step(self.keys, dir)
    }

    /// The pane next to the keys' in `dir`, which then has them.
    pub fn focus_towards(&mut self, dir: Dir) -> bool {
        let Some(next) = self.grid.neighbour(self.keys, dir) else { return false };
        self.keys = next;
        self.leader = false;
        true
    }

    /// Moves the keys `by` panes along the grid in reading order, wrapping
    /// round. False when the keys are not in it or it has only one pane.
    pub fn step_keys(&mut self, by: isize) -> bool {
        let row: Vec<Side> = self.grid.sides().into_iter().filter(|&s| self.on(s).is_some()).collect();
        let Some(at) = row.iter().position(|&s| s == self.keys) else { return false };
        if row.len() < 2 {
            return false;
        }
        self.keys = row[(at as isize + by).rem_euclid(row.len() as isize) as usize];
        self.leader = false;
        true
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
        for slot in self.grid.cells_mut().map(|(_, slot)| slot).chain(&mut self.console) {
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
    use crate::split_grid::Zone;

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

    /// Which cells sit in each row of the grid, as the page reads them.
    fn rows(shown: &ShownPane) -> Vec<Vec<u8>> {
        shown.grid().page(shown.keys(), |_| None).grid
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

    /// Pane 1 on the Workspace and 4, 5 and 6 split beside it, in rooms of
    /// 20x30 each: split before the page measures, which a grid 30 columns
    /// wide would refuse.
    fn row(shown: &mut ShownPane) {
        let mut out = Vec::new();
        shown.show_on(Side::Workspace, PaneId(1), &mut out);
        for pane in [4, 5, 6] {
            shown.open_beside(PaneId(pane), &mut out).unwrap();
        }
        for side in shown.grid().sides() {
            shown.resize(side, 20, 30, &mut out);
        }
    }

    #[test]
    fn a_pane_on_the_console_is_refused_and_nothing_is_subscribed() {
        let mut shown = ShownPane::default();
        split(&mut shown);
        shown.sync_views(&wanted(&[("ops/dev", Some(7))]), &mut Vec::new());
        for pane in [2, 3, 7] {
            let mut out = Vec::new();
            assert_eq!(shown.open_beside(PaneId(pane), &mut out), Err(Refusal::OnConsole));
            assert!(out.is_empty());
        }
        assert_eq!(shown.grid().sides(), [Side::Workspace]);
    }

    #[test]
    fn a_pane_is_split_off_only_by_joining_the_grid() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        assert!(sent(|out| assert!(!shown.show_on(Side::Beside(0), PaneId(9), out))).is_empty());
        assert!(sent(|out| assert!(!shown.show_on(Side::Beside(1), PaneId(1), out))).is_empty());
        let sides = [Side::Workspace, Side::Beside(0), Side::Beside(1), Side::Beside(2)];
        assert_eq!(sides.map(|side| shown.on(side)), [1, 4, 5, 6].map(|pane| Some(PaneId(pane))));
    }

    #[test]
    fn a_workspace_pane_moved_to_the_console_leaves_no_empty_cell() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        shown.show_on(Side::First, PaneId(1), &mut Vec::new());
        assert_eq!((shown.on(Side::Workspace), shown.on(Side::First)), (Some(PaneId(4)), Some(PaneId(1))));
        assert_eq!(shown.grid().sides().len(), 3);
    }

    #[test]
    fn closing_a_split_keeps_the_others_shifts_them_down_and_hands_the_keys_to_its_neighbour() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        shown.focus(PaneId(5));
        assert_eq!(shown.keys(), Side::Beside(1));
        assert_eq!(sent(|out| shown.close(Side::Beside(1), out)), ["unsubscribe 5"]);
        assert_eq!((shown.on(Side::Beside(0)), shown.on(Side::Beside(1))), (Some(PaneId(4)), Some(PaneId(6))));
        assert_eq!(shown.keys(), Side::Beside(0), "the neighbour on its left");
        assert_eq!(rows(&shown), [vec![0, 1], vec![2]], "the rest are numbered down");
    }

    #[test]
    fn closing_the_first_split_with_the_keys_on_it_hands_them_to_the_workspace() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        shown.focus(PaneId(4));
        shown.gone(PaneId(4), &mut Vec::new());
        assert_eq!((shown.keys(), shown.pane()), (Side::Workspace, Some(PaneId(1))));
    }

    #[test]
    fn the_workspace_going_moves_the_first_split_over_at_its_size_with_its_keys() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        assert_eq!(sent(|out| shown.close(Side::Workspace, out)), ["unsubscribe 1"], "every room is the same");
        assert_eq!((shown.on(Side::Workspace), shown.on(Side::Beside(0)), shown.on(Side::Beside(1))), (Some(PaneId(4)), Some(PaneId(5)), Some(PaneId(6))));
        assert_eq!(shown.keys(), Side::Workspace);
        assert_eq!(rows(&shown), [vec![0], vec![1, 2]]);
    }

    #[test]
    fn a_pane_renumbered_onto_another_side_is_sized_to_that_sides_room() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        for (n, cols) in [(0, 31), (1, 32), (2, 33)] {
            shown.resize(Side::Beside(n), 20, cols, &mut Vec::new());
        }
        assert_eq!(sent(|out| shown.close(Side::Beside(0), out)), ["unsubscribe 4", "resize 5 20x31", "resize 6 20x32"]);
    }

    #[test]
    fn the_first_split_taking_the_workspaces_side_takes_its_room() {
        let mut shown = ShownPane::default();
        shown.resize(Side::Workspace, 20, 60, &mut Vec::new());
        shown.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        let side = shown.open_beside(PaneId(4), &mut Vec::new()).unwrap();
        shown.resize(side, 20, 40, &mut Vec::new());
        assert_eq!(sent(|out| shown.close(Side::Workspace, out)), ["unsubscribe 1", "resize 4 20x60"]);
        assert_eq!(sent(|out| shown.resize(Side::Workspace, 20, 40, out)), ["resize 4 20x40"]);
    }

    #[test]
    fn the_keys_step_along_the_row_and_wrap() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        assert!(shown.step_keys(1) && shown.keys() == Side::Beside(0));
        shown.focus(PaneId(6));
        assert!(shown.step_keys(1) && shown.keys() == Side::Workspace, "wraps to the first");
        assert!(shown.step_keys(-1) && shown.keys() == Side::Beside(2), "and back to the last");
        let mut alone = ShownPane::default();
        alone.show_on(Side::Workspace, PaneId(1), &mut Vec::new());
        assert!(!alone.step_keys(1));
    }

    #[test]
    fn only_a_split_is_taken_out_of_the_grid() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        shown.show_on(Side::First, PaneId(2), &mut Vec::new());
        assert!(sent(|out| assert!(!shown.close_split(Side::Workspace, out))).is_empty());
        assert!(sent(|out| assert!(!shown.close_split(Side::First, out))).is_empty());
        assert!(sent(|out| assert!(!shown.close_split(Side::Beside(7), out))).is_empty(), "no such split");
        assert_eq!(sent(|out| assert!(shown.close_split(Side::Beside(0), out))), ["unsubscribe 4"]);
        assert_eq!(shown.panes(), [PaneId(1), PaneId(5), PaneId(6), PaneId(2)]);
    }

    #[test]
    fn a_pane_dropped_on_another_moves_in_the_grid_and_the_keys_read_in_the_new_order() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        assert!(shown.grid_mut().place(PaneId(6), PaneId(1), Zone::Left));
        assert_eq!(shown.grid().sides(), [Side::Beside(2), Side::Workspace, Side::Beside(0), Side::Beside(1)]);
        assert!(!shown.grid_mut().place(PaneId(9), PaneId(1), Zone::Left), "not in the grid");
        shown.focus(PaneId(6));
        assert!(shown.step_keys(1) && shown.keys() == Side::Workspace, "reading order, not opening order");
    }

    #[test]
    fn the_focused_pane_trades_places_with_its_neighbour_and_the_keys_move_spatially() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        shown.focus(PaneId(4));
        assert!(shown.focus_towards(Dir::Down));
        assert_eq!(shown.keys(), Side::Beside(2), "pane 6 is under pane 4");
        assert!(!shown.focus_towards(Dir::Right), "nothing further");
        assert!(shown.step_pane(Dir::Left));
        assert_eq!(rows(&shown)[1], [3, 2]);
        assert!(shown.focus_towards(Dir::Up));
        assert_eq!(shown.keys(), Side::Workspace, "pane 6 is now under pane 1");
    }

    #[test]
    fn a_reconnect_subscribes_the_splits_again_at_their_sizes() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        assert_eq!(sent(|out| shown.reconnected(out)).len(), 8);
    }

    #[test]
    fn a_pane_shown_on_the_workspace_that_was_a_split_leaves_no_empty_split() {
        let mut shown = ShownPane::default();
        row(&mut shown);
        shown.show_on(Side::Workspace, PaneId(5), &mut Vec::new());
        assert_eq!(shown.panes(), [PaneId(5), PaneId(4), PaneId(6)]);
        assert_eq!(rows(&shown), [vec![0, 1], vec![2]]);
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
