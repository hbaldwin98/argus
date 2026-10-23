//! The shells that belong to no checkout: the daemon's list of them, which
//! one the stage shows, and asking for another.
//!
//! Consoles belong to no project and so do not travel in the tree: the
//! selection cannot name one by its place there: it holds
//! the console picked on the rail by id. The list is only ever the daemon's
//! latest `ServerMsg::Consoles`, so a console asked for is on the rail once
//! the daemon says it is running, and one closed is gone once it says so.

use crate::app::*;

impl App {
    /// The console on the stage: the one picked on the rail, while the
    /// workspace is up and the rail's cursor is not on a row of the tree.
    /// Everywhere else the tree's selection is what the keys act on — the
    /// Feature stage writes to the selected checkout, and a console has none.
    pub fn current_console(&self) -> Option<&PaneInfo> {
        let id = self.console?;
        let on_tree_row = matches!(
            self.focus,
            Focus::Projects | Focus::Repositories | Focus::Checkouts
        );
        if self.view != View::Workspace || on_tree_row {
            return None;
        }
        self.consoles.iter().find(|console| console.id == id)
    }

    /// Where the console on the stage sits in the list the rail draws.
    pub(crate) fn console_index(&self) -> Option<usize> {
        let id = self.current_console()?.id;
        self.consoles.iter().position(|console| console.id == id)
    }

    /// Puts the rail's cursor on the console at `index`, which the stage
    /// then shows.
    pub(in crate::app) fn pick_console(&mut self, index: usize) {
        if let Some(console) = self.consoles.get(index) {
            self.console = Some(console.id);
            self.focus = Focus::Panes;
        }
        self.clamp();
    }

    /// The selection has moved to a row of the tree. Forgotten rather than
    /// left behind, so a later return to the pane rows lands on the tree's
    /// pane and not on a console picked long before.
    pub(in crate::app) fn forget_console(&mut self) {
        self.console = None;
    }

    /// `c`, or `s` on a console's row: asks the daemon for a console and
    /// takes the keys to it once its list holds it.
    pub(in crate::app) fn spawn_console(&mut self) {
        if !self.consoles_offered {
            let (argusd, fix) = self.daemon_and_its_fix();
            self.alert(format!("{argusd} predates consoles; {fix}"));
            return;
        }
        let request_id = self.awaited.ask(Then::FocusPane { floating: false });
        let _ = self.out.send(ClientMsg::SpawnConsole { request_id });
    }

    /// Whether the daemon's list holds the pane `id`.
    pub(in crate::app) fn is_console(&self, id: PaneId) -> bool {
        self.consoles.iter().any(|console| console.id == id)
    }

    /// The daemon's list, replacing the last. A console on the stage that
    /// has gone gives its row to its neighbour, as a pane does, but takes
    /// the keys back to the rail: typing must never go on into a shell
    /// nobody chose. With no console left, the cursor goes back to the tree.
    pub(in crate::app) fn receive_consoles(&mut self, consoles: Vec<PaneInfo>) {
        let shown = self.current_console().is_some();
        let at = self
            .console
            .and_then(|id| self.consoles.iter().position(|console| console.id == id));
        self.consoles = consoles;
        // A daemon that lists consoles keeps them, whichever of this and
        // its greeting the client happens to read first.
        self.consoles_offered = true;
        let gone = self.console.is_some_and(|id| !self.is_console(id));
        if gone {
            self.console = at
                .and_then(|at| self.consoles.get(at).or(self.consoles.last()))
                .map(|console| console.id);
            if shown {
                self.leader_pending = false;
                self.pane_fullscreen = false;
                self.focus = Focus::Panes;
                if self.console.is_none() {
                    match self.rail_rows().last().copied() {
                        Some(row) => self.put_rail_cursor(row),
                        None => self.focus = Focus::Projects,
                    }
                }
            }
        }
        self.settle_awaited();
        self.clamp();
    }

    /// Whether the daemon now on the other end keeps consoles. One that
    /// does not has none to list, whatever an earlier daemon had.
    pub(in crate::app) fn offer_consoles(&mut self, offered: bool) {
        self.consoles_offered = offered;
        if !offered {
            self.consoles.clear();
            self.console = None;
        }
    }
}
