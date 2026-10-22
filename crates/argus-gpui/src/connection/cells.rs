//! What the Workspace's grid is, as the rail last said it: the pane in each cell by its number,
//! and whether the grid is on screen.
//!
//! A cell is numbered as the rail numbers it (`GridPage`): 0 is the Workspace's own pane, `n + 1`
//! is `Beside(n)`. The cells and the drags that land on them read these and never count the rail
//! themselves.

use argus_desktop_core::connection::page::Side;

use super::heard::Heard;

impl Heard {
    /// The pane in each cell, the Workspace's first; none for a cell the tree no longer names.
    pub fn cell_panes(&self) -> Vec<Option<u64>> {
        let own = self.rail.shown.as_ref().map(|shown| shown.pane);
        std::iter::once(own).chain(self.rail.row.beside.iter().map(|shown| shown.as_ref().map(|shown| shown.pane))).collect()
    }

    /// Whether the grid is drawn: the Workspace's stage is up, no console covers it, and the Git
    /// stage is not standing in its place.
    pub fn grid_up(&self) -> bool {
        self.rail.workspace_shown && !self.git_up()
    }

    /// The cell that has the session's keys, if one does.
    pub fn keyed_cell(&self) -> Option<u8> {
        self.rail.row.keyed
    }

    /// Whether `side` is a cell of the grid that has the keys now. The Workspace's own cell has
    /// them unless the keys are on a split or typing in a console.
    pub fn cell_has_keys(&self, side: Side) -> bool {
        match side {
            Side::Workspace => self.console.typing.is_none() && self.rail.row.keyed.is_none_or(|cell| cell == 0),
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
        assert_eq!(heard.cell_panes(), [Some(1), Some(5), None, Some(7)]);
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
