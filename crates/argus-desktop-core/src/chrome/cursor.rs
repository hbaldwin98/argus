//! Which row of the rail holds the keys, and the checkout's + menu open under a row: the rail's
//! cursor.
//!
//! The page keeps these in its own state (`railPick`, `state.start`); the session keeps them here so
//! the GPUI window and the keymap tests share one rule. The cursor follows the tree and never
//! predicts a request: a pane the pick steps onto is shown by a call, and a pane closed leaves its
//! row until a tree without it arrives.

use serde::{Deserialize, Serialize};

use crate::connection::page::Rail;
use crate::keys::keymap::Action;

/// The row that holds the keys: a checkout, or a pane and the checkout it is under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    pub checkout: u64,
    pub pane: Option<u64>,
}

/// Where a checkout's + menu was pressed: the Checkouts table shares the rail's one menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum StartFrom {
    Rail,
    Table,
}

/// An open + menu: its checkout, where it was pressed, and the highlighted item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartMenu {
    pub checkout: u64,
    pub from: StartFrom,
    pub at: usize,
}

/// What the cursor says of the rail, as the rail event carries it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RailCursorPage {
    pub pick: Option<Pick>,
    pub start: Option<StartMenu>,
}

/// One item of a + menu: a shell, or an agent from a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub label: String,
    /// The template; none for a shell.
    pub agent: Option<String>,
}

/// What a + menu offers, in its order: a shell, then each of the daemon's agent templates, the first
/// its default.
pub fn starts(templates: &[String]) -> Vec<Start> {
    std::iter::once(Start { label: "Shell".into(), agent: None })
        .chain(templates.iter().map(|name| Start { label: format!("Agent \u{b7} {name}"), agent: Some(name.clone()) }))
        .collect()
}

/// What the session does for a key the cursor answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Then {
    Nothing,
    /// The pick stepped onto this pane: it comes on screen, as a click on it does.
    Show(u64),
    /// Start a shell, or an agent from `agent`, on `checkout`; the menu is shut and the pick let go.
    Start { checkout: u64, agent: Option<String> },
    /// An agent was asked for and the daemon offers none.
    NoAgent,
    Review(u64),
    History(u64),
    /// Fold or unfold the checkout's panes.
    Fold(u64),
    /// Remove the checkout (the Checkouts stage asks first).
    Remove(u64),
    /// Close the pane.
    Close(u64),
}

/// Whether the cursor answers `action` while a row is picked.
pub fn answers(action: Action) -> bool {
    matches!(
        action,
        Action::CloseLayer
            | Action::Next
            | Action::Previous
            | Action::StartShell
            | Action::StartAgent
            | Action::OpenReview
            | Action::OpenHistory
            | Action::Fold
            | Action::Close
            | Action::Choose
    )
}

/// Whether the cursor answers `action` while a + menu is open.
pub fn answers_start(action: Action) -> bool {
    matches!(
        action,
        Action::CloseLayer | Action::StartShell | Action::StartAgent | Action::Next | Action::Previous | Action::Choose
    )
}

#[derive(Debug, Default)]
pub struct RailCursor {
    pick: Option<Pick>,
    start: Option<StartMenu>,
}

impl RailCursor {
    pub fn page(&self) -> RailCursorPage {
        RailCursorPage { pick: self.pick, start: self.start }
    }

    pub fn picked(&self) -> Option<Pick> {
        self.pick
    }

    pub fn start_open(&self) -> bool {
        self.start.is_some()
    }

    pub fn pick(&mut self, checkout: u64, pane: Option<u64>) {
        self.pick = Some(Pick { checkout, pane });
    }

    pub fn unpick(&mut self) {
        self.pick = None;
    }

    /// A checkout's panes are about to be folded: a pane picked under it gives the pick to the
    /// checkout, since its row is about to go.
    pub fn folding(&mut self, checkout: u64) {
        if self.pick.is_some_and(|p| p.checkout == checkout && p.pane.is_some()) {
            self.pick = Some(Pick { checkout, pane: None });
        }
    }

    /// Opens a checkout's + menu where it was pressed, with the first item highlighted, or shuts it
    /// if that same one is open.
    pub fn toggle_start(&mut self, checkout: u64, from: StartFrom) {
        let same = self.start.is_some_and(|open| open.checkout == checkout && open.from == from);
        self.start = (!same).then_some(StartMenu { checkout, from, at: 0 });
    }

    pub fn close_start(&mut self) {
        self.start = None;
    }

    /// The tree changed under the pick: only a pane no longer listed (closed, or its checkout
    /// folded) moves it, to its checkout. A checkout that is gone keeps the pick, which then marks
    /// no row. Whether it moved.
    pub fn settle(&mut self, rail: &Rail) -> bool {
        let Some(Pick { checkout, pane: Some(pane) }) = self.pick else { return false };
        let listed = rail
            .project
            .iter()
            .flat_map(|p| &p.repositories)
            .filter(|r| r.open)
            .flat_map(|r| &r.checkouts)
            .find(|c| c.id == checkout)
            .is_some_and(|c| c.open && c.panes.iter().any(|p| p.id == pane));
        if !listed {
            self.pick = Some(Pick { checkout, pane: None });
        }
        !listed
    }

    /// A key while a row is picked: `x` closes a picked pane, Enter on a checkout opens its menu.
    /// Nothing is done when no row is picked.
    pub fn key(&mut self, action: Action, rail: &Rail) -> Then {
        let Some(pick) = self.pick else { return Then::Nothing };
        match action {
            Action::CloseLayer => {
                self.pick = None;
                Then::Nothing
            }
            Action::Next | Action::Previous => self.step(if action == Action::Next { 1 } else { -1 }, rail),
            Action::StartShell => self.start_on(pick.checkout, None),
            Action::StartAgent => self.start_agent(pick.checkout, rail),
            Action::OpenReview => Then::Review(pick.checkout),
            Action::OpenHistory => Then::History(pick.checkout),
            Action::Fold => Then::Fold(pick.checkout),
            Action::Close => match pick.pane {
                // On a checkout, the Checkouts stage opens on it and asks first.
                None => Then::Remove(pick.checkout),
                Some(pane) => {
                    // The rail drops the pane when the daemon's tree does; the cursor falls back to
                    // its checkout.
                    self.pick = Some(Pick { checkout: pick.checkout, pane: None });
                    Then::Close(pane)
                }
            },
            Action::Choose => {
                if pick.pane.is_none() {
                    self.toggle_start(pick.checkout, StartFrom::Rail);
                } else {
                    self.pick = None;
                }
                Then::Nothing
            }
            _ => Then::Nothing,
        }
    }

    /// A key while a + menu is open.
    pub fn start_key(&mut self, action: Action, rail: &Rail) -> Then {
        let Some(open) = self.start else { return Then::Nothing };
        let items = starts(&rail.templates);
        match action {
            Action::CloseLayer => {
                self.start = None;
                Then::Nothing
            }
            Action::StartShell => self.start_on(open.checkout, None),
            Action::StartAgent => self.start_agent(open.checkout, rail),
            Action::Next | Action::Previous => {
                let by = if action == Action::Next { 1 } else { items.len() - 1 };
                self.start = Some(StartMenu { at: (open.at + by) % items.len(), ..open });
                Then::Nothing
            }
            Action::Choose => match items.into_iter().nth(open.at) {
                Some(item) => self.start_on(open.checkout, item.agent),
                None => Then::Nothing,
            },
            _ => Then::Nothing,
        }
    }

    /// Starts something on a checkout: the menu shuts and the pick is let go, since the new pane
    /// takes the keys when it comes on screen.
    pub fn start_on(&mut self, checkout: u64, agent: Option<String>) -> Then {
        self.start = None;
        self.pick = None;
        Then::Start { checkout, agent }
    }

    fn start_agent(&mut self, checkout: u64, rail: &Rail) -> Then {
        match rail.templates.first() {
            Some(agent) => self.start_on(checkout, Some(agent.clone())),
            None => Then::NoAgent,
        }
    }

    /// Moves the pick `by` rows through every checkout and its panes in the rail's order, stopping at
    /// the ends. A pane picked comes on screen, as a click on it does.
    fn step(&mut self, by: isize, rail: &Rail) -> Then {
        let Some(pick) = self.pick else { return Then::Nothing };
        let rows: Vec<Pick> = rail
            .project
            .iter()
            .flat_map(|p| &p.repositories)
            .flat_map(|r| &r.checkouts)
            .flat_map(|c| {
                std::iter::once(Pick { checkout: c.id, pane: None })
                    .chain(c.panes.iter().map(|p| Pick { checkout: c.id, pane: Some(p.id) }))
            })
            .collect();
        if rows.is_empty() {
            return Then::Nothing;
        }
        let at = rows.iter().position(|row| *row == pick).map_or(-1, |at| at as isize);
        // The page's `Math.max(0, Math.min(len - 1, at + by))`: a pick on no row steps to the first.
        let next = rows[(at + by).min(rows.len() as isize - 1).max(0) as usize];
        self.pick = Some(next);
        next.pane.map_or(Then::Nothing, Then::Show)
    }
}

#[cfg(test)]
mod tests;
