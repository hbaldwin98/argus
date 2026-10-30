//! What the combine-projects dialog is to the page: the gestures made in it and the page it is
//! told, with the project folded into, the projects offered to fold in and what the tick would do.
//!
//! The state behind them is `combine`; this is only the shape that crosses to the page and to the
//! GPUI window, which read the same page and send the same gestures.

use serde::{Deserialize, Serialize};

use super::add_page::ProjectRef;

/// What was done in the combine-projects dialog.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum CombineGesture {
    /// Fold into this project instead.
    Into { project: u64 },
    /// Tick or untick a project to fold in.
    Tick { project: u64 },
    /// Ask the daemon.
    Combine,
    Close,
}

/// The combine-projects dialog: which project is folded into, the others of the open workspace to
/// fold in, and what ticking them would hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CombinePage {
    pub open: bool,
    /// The project that keeps its name and settings.
    pub into: Option<ProjectRef>,
    /// Every project of the open workspace, to choose the one folded into from.
    pub choices: Vec<IntoChoice>,
    /// Every other project, ticked or not.
    pub rows: Vec<CombineRow>,
    /// What `into` would hold once the ticked projects are folded in.
    pub holds: Vec<String>,
    /// Repositories `into` would hold: the paths, not the names, so one both hold counts once.
    pub repositories: usize,
    /// Agents running in the ticked projects, which carry on in their moved repositories.
    pub agents: usize,
    /// The ask is with the daemon.
    pub combining: bool,
    /// What the daemon said when it refused the last ask.
    pub refusal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct IntoChoice {
    pub id: u64,
    pub name: String,
    pub repositories: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CombineRow {
    pub id: u64,
    pub name: String,
    pub repositories: usize,
    pub needs_you: usize,
    pub working: usize,
    /// The project has a folder it scans for repositories.
    pub scans_folder: bool,
    pub ticked: bool,
}
