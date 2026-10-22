//! Which browser pane has the keys, and what a press in a cell does to
//! give them.
//!
//! The page asked this of its rail (`keyedBrowser` in `main.ts`, `keyPane`
//! in `browser/pane.ts`); it is the same question for any window that draws
//! the Workspace's grid, so it is answered here from the rail's own data.

use crate::connection::page::{Call, Shown};
use crate::panes::shown::workspace_grid::GridPage;

/// The pane with the keys, the first cell's or one beside it; `None` while
/// the keys are off the grid, or the cell they name has no pane.
pub fn keyed_pane<'a>(shown: Option<&'a Shown>, row: &'a GridPage) -> Option<&'a Shown> {
    match row.keyed? {
        0 => shown,
        cell => row.beside.get(cell as usize - 1)?.as_ref(),
    }
}

/// The browser pane with the keys, if the pane that has them is a browser.
/// While there is one, the window types no bytes into the hidden text
/// area and the cell's canvas takes the keys (SHELL-BROWSER-15).
pub fn keyed_browser(shown: Option<&Shown>, row: &GridPage) -> Option<u64> {
    keyed_pane(shown, row).filter(|pane| pane.browser).map(|pane| pane.pane)
}

/// The call that gives `pane`'s cell the keys, as a press on a terminal
/// does, so the pane commands the keymap runs are aimed at it. `None` when
/// the cell already has them or `pane` is in no cell (SHELL-BROWSER-15).
pub fn key_pane(shown: Option<&Shown>, row: &GridPage, pane: u64) -> Option<Call> {
    let cell = if shown.is_some_and(|first| first.pane == pane) {
        0
    } else {
        row.beside.iter().position(|side| side.as_ref().is_some_and(|side| side.pane == pane))? + 1
    };
    if row.keyed == Some(cell as u8) {
        return None;
    }
    Some(if cell == 0 { Call::Select { pane } } else { Call::FocusView { pane } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(id: u64, browser: bool) -> Shown {
        Shown {
            pane: id,
            repository: String::new(),
            checkout: String::new(),
            branch: String::new(),
            agent: false,
            browser,
            title: String::new(),
            status: "idle",
            resumed: None,
        }
    }

    fn grid(keyed: Option<u8>, beside: Vec<Option<Shown>>) -> GridPage {
        GridPage { beside, keyed, ..GridPage::default() }
    }

    #[test]
    fn the_keyed_cell_is_the_first_or_one_beside() {
        let first = pane(1, false);
        let row = grid(Some(0), vec![Some(pane(2, true))]);
        assert_eq!(keyed_pane(Some(&first), &row).map(|p| p.pane), Some(1));
        let row = grid(Some(1), vec![Some(pane(2, true))]);
        assert_eq!(keyed_pane(Some(&first), &row).map(|p| p.pane), Some(2));
        assert_eq!(keyed_browser(Some(&first), &row), Some(2));
        assert_eq!(keyed_browser(Some(&first), &grid(Some(0), Vec::new())), None, "a terminal is not a browser");
        assert_eq!(keyed_pane(Some(&first), &grid(None, Vec::new())).map(|p| p.pane), None, "the keys are off the grid");
        assert_eq!(keyed_pane(Some(&first), &grid(Some(2), vec![None, None])).map(|p| p.pane), None, "a cell the tree no longer names");
    }

    #[test]
    fn a_press_gives_a_cell_the_keys_only_when_it_lacks_them() {
        let first = pane(1, false);
        let beside = vec![Some(pane(2, true))];
        assert!(matches!(key_pane(Some(&first), &grid(Some(1), beside.clone()), 1), Some(Call::Select { pane: 1 })));
        assert!(matches!(key_pane(Some(&first), &grid(Some(0), beside.clone()), 2), Some(Call::FocusView { pane: 2 })));
        assert!(key_pane(Some(&first), &grid(Some(1), beside.clone()), 2).is_none(), "it has them already");
        assert!(key_pane(Some(&first), &grid(Some(0), beside.clone()), 1).is_none());
        assert!(key_pane(Some(&first), &grid(Some(0), beside), 9).is_none(), "a pane in no cell");
    }
}
