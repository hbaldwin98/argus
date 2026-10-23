//! What the Workspace's grid is, as the rail last said it: what each cell holds by its number, a
//! pane or a panel, and whether the grid is on screen.
//!
//! A cell is numbered as the rail numbers it (`GridPage`): 0 is the Workspace's own pane, `n + 1`
//! is `Beside(n)`. The cells and the drags that land on them read these and never count the rail
//! themselves.

use argus_desktop_core::connection::page::Side;
use argus_desktop_core::panes::console::ConsolePlace;
use argus_desktop_core::panes::shown::workspace_grid::Occupant;

use super::heard::Heard;

impl Heard {
    /// What each cell holds, the Workspace's pane first; none for a cell the tree no longer names.
    pub fn cell_panes(&self) -> Vec<Option<Occupant>> {
        let row = &self.rail.row;
        let own = self.rail.shown.as_ref().map(|shown| Occupant::Pane(shown.pane));
        let beside = row.beside.iter().enumerate().map(|(n, shown)| match (shown, row.panels.get(n).cloned().flatten()) {
            (_, Some(panel)) => Some(Occupant::Panel(panel)),
            (Some(shown), None) => Some(Occupant::Pane(shown.pane)),
            (None, None) => None,
        });
        std::iter::once(own).chain(beside).collect()
    }

    /// Whether the grid is drawn: the Workspace's stage is up, no console covers it, and the Git
    /// stage is not standing in its place.
    pub fn grid_up(&self) -> bool {
        // The session's rail says the console covers the Workspace too, but its `Console` event and
        // its `Rail` event arrive apart: until both have, a cell must not be a target under it.
        let covered = matches!(self.console.place, ConsolePlace::Tab | ConsolePlace::Fullscreen);
        self.rail.workspace_shown && !covered && !self.git_up()
    }

    /// The cell that has the session's keys, if one does.
    pub fn keyed_cell(&self) -> Option<u8> {
        self.rail.row.keyed
    }

    /// Whether `side` is a cell of the grid that has the keys now. The Workspace's own cell has
    /// them unless the keys are on a split, typing in a console, or in an extension's console view.
    pub fn cell_has_keys(&self, side: Side) -> bool {
        match side {
            Side::Workspace => self.console.typing.is_none() && !self.view_has_keys() && self.rail.row.keyed.is_none_or(|cell| cell == 0),
            Side::Beside(n) => self.rail.row.keyed == Some(n + 1),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::{Event, Rail, Shown};

    fn shown(pane: u64) -> Shown {
        Shown {
            pane,
            repository: "r".into(),
            checkout: "c".into(),
            branch: "b".into(),
            agent: false,
            browser: false,
            title: format!("pane {pane}"),
            status: "idle",
            resumed: None,
        }
    }

    fn heard_with(own: Option<u64>, beside: Vec<Option<u64>>, keyed: Option<u8>) -> Heard {
        let mut heard = Heard::new();
        let mut rail = Rail { loaded: true, workspace_shown: true, shown: own.map(shown), ..Rail::default() };
        rail.row.beside = beside.into_iter().map(|pane| pane.map(shown)).collect();
        rail.row.keyed = keyed;
        heard.hear(Event::Rail(Box::new(rail)));
        heard
    }

    #[test]
    fn the_cells_are_numbered_the_workspaces_first_then_each_split_in_the_order_opened() {
        let heard = heard_with(Some(1), vec![Some(5), None, Some(7)], None);
        let pane = |pane| Some(Occupant::Pane(pane));
        assert_eq!(heard.cell_panes(), [pane(1), pane(5), None, pane(7)]);
    }

    #[test]
    fn a_cell_with_a_panel_holds_the_panel() {
        use argus_desktop_core::layout::docks::Panel;
        let mut heard = heard_with(Some(1), vec![None, Some(7)], None);
        let mut rail = heard.rail.clone();
        rail.row.panels = vec![Some(Panel::Browsers), None];
        heard.hear(Event::Rail(Box::new(rail)));
        assert_eq!(heard.cell_panes(), [Some(Occupant::Pane(1)), Some(Occupant::Panel(Panel::Browsers)), Some(Occupant::Pane(7))]);
    }

    #[test]
    fn a_workspace_with_no_pane_has_one_empty_cell() {
        assert_eq!(heard_with(None, vec![], None).cell_panes(), [None]);
    }

    #[test]
    fn the_grid_is_up_only_while_the_workspaces_stage_is() {
        let mut heard = heard_with(Some(1), vec![], None);
        assert!(heard.grid_up());
        let mut rail = heard.rail.clone();
        rail.workspace_shown = false;
        heard.hear(Event::Rail(Box::new(rail)));
        assert!(!heard.grid_up());
    }

    #[test]
    fn a_console_over_the_workspace_makes_its_cells_no_drop_targets_whatever_the_rail_last_said() {
        use argus_desktop_core::panes::console::ConsolePage;
        for place in [ConsolePlace::Tab, ConsolePlace::Fullscreen] {
            let mut heard = heard_with(Some(1), vec![Some(5)], None);
            assert!(heard.grid_up());
            heard.hear(Event::Console(Box::new(ConsolePage { place, ..ConsolePage::default() })));
            assert!(heard.rail.workspace_shown, "the rail has not caught up");
            assert!(!heard.grid_up(), "{place:?}");
        }
        let mut heard = heard_with(Some(1), vec![], None);
        for place in [ConsolePlace::Docked, ConsolePlace::Hidden] {
            heard.hear(Event::Console(Box::new(ConsolePage { place, ..ConsolePage::default() })));
            assert!(heard.grid_up(), "a docked or hidden console is beside the grid, not over it");
        }
    }

    #[test]
    fn the_keys_are_in_the_cell_the_rail_numbers_and_in_no_other() {
        let heard = heard_with(Some(1), vec![Some(5), Some(6)], Some(2));
        assert!(heard.cell_has_keys(Side::Beside(1)));
        assert!(!heard.cell_has_keys(Side::Beside(0)));
        assert!(!heard.cell_has_keys(Side::Workspace));
        let own = heard_with(Some(1), vec![Some(5)], Some(0));
        assert!(own.cell_has_keys(Side::Workspace) && !own.cell_has_keys(Side::Beside(0)));
    }

    #[test]
    fn with_the_keys_off_the_grid_the_workspace_cell_stands_for_them() {
        let heard = heard_with(Some(1), vec![Some(5)], None);
        assert!(heard.cell_has_keys(Side::Workspace), "the console's typing is what takes them");
        assert!(!heard.cell_has_keys(Side::Beside(0)));
    }
}
