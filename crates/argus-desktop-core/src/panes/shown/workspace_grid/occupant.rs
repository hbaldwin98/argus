//! What a cell of the grid holds, a pane or a panel, as a drop names the cell it was let go over.

use argus_protocol::PaneId;
use serde::{Deserialize, Serialize};

use super::super::Slot;
use crate::layout::docks::Panel;

/// What a cell holds, as a drop names the cell it was let go over: a pane
/// by its number, a panel by its name. The page has always sent a pane's
/// number alone, so neither is tagged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(untagged)]
pub enum Occupant {
    Pane(u64),
    Panel(Panel),
}

impl Slot {
    pub(super) fn holds(&self, occupant: &Occupant) -> bool {
        match occupant {
            Occupant::Pane(pane) => self.pane == Some(PaneId(*pane)),
            Occupant::Panel(panel) => self.panel.as_ref() == Some(panel),
        }
    }

    pub(super) fn filled(&self) -> bool {
        self.pane.is_some() || self.panel.is_some()
    }
}
