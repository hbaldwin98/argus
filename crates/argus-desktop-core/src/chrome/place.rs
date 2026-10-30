//! Where the person was in each host, workspace and project, for switching
//! back to land there, and for the next run to open there.
//!
//! Everything is keyed by names, never ids: the daemon hands its ids out
//! anew when it restarts, and a name is what the person would recognise in
//! the file. Panes are the exception, kept by id with their checkout's path
//! beside, so a pane that has closed since can be replaced by another of
//! its checkout rather than by nothing. A name the tree no longer holds is
//! left in the file: the project may be added back, and it costs a line.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The stages whose state needs nothing the daemon has to send first, so
/// they can be put up again without waiting on a board or a listing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    #[default]
    Workspace,
    Feature,
    Checkouts,
}

/// What the window showed in one project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spot {
    /// The repository chosen on the rail, by name.
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub stage: Stage,
    /// The Workspace's pane, and the path of its checkout.
    #[serde(default)]
    pub pane: Option<u64>,
    #[serde(default)]
    pub checkout: Option<String>,
    /// The panes split beside it, in the order they opened.
    #[serde(default)]
    pub beside: Vec<u64>,
    /// The grid's pane that had the keys.
    #[serde(default)]
    pub keys: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Workspace {
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    projects: BTreeMap<String, Spot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Host {
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    workspaces: BTreeMap<String, Workspace>,
}

/// Every place the window has been. The local machine's daemon is the host
/// named by an empty string.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Places {
    hosts: BTreeMap<String, Host>,
}

impl Places {
    /// The workspace open last on `host`.
    pub fn workspace(&self, host: &str) -> Option<&str> {
        self.hosts.get(host)?.workspace.as_deref()
    }

    /// The project open last in `workspace`.
    pub fn project(&self, host: &str, workspace: &str) -> Option<&str> {
        self.hosts.get(host)?.workspaces.get(workspace)?.project.as_deref()
    }

    pub fn spot(&self, host: &str, workspace: &str, project: &str) -> Option<&Spot> {
        self.hosts.get(host)?.workspaces.get(workspace)?.projects.get(project)
    }

    /// Notes the person is in `project`, showing `spot`. Whether that
    /// changed anything the file holds, which is when it is worth saving.
    pub fn record(&mut self, host: &str, workspace: &str, project: &str, spot: Spot) -> bool {
        let host = self.hosts.entry(host.to_string()).or_default();
        let mut changed = host.workspace.as_deref() != Some(workspace);
        host.workspace = Some(workspace.to_string());
        let workspace = host.workspaces.entry(workspace.to_string()).or_default();
        changed |= workspace.project.as_deref() != Some(project);
        workspace.project = Some(project.to_string());
        if workspace.projects.get(project) != Some(&spot) {
            workspace.projects.insert(project.to_string(), spot);
            changed = true;
        }
        changed
    }

    /// Notes the person is in `workspace`, wherever in it they are.
    pub fn record_workspace(&mut self, host: &str, workspace: &str) -> bool {
        let host = self.hosts.entry(host.to_string()).or_default();
        let changed = host.workspace.as_deref() != Some(workspace);
        host.workspace = Some(workspace.to_string());
        host.workspaces.entry(workspace.to_string()).or_default();
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_place_is_read_back_by_host_workspace_and_project() {
        let mut places = Places::default();
        let spot = Spot { pane: Some(7), stage: Stage::Feature, ..Spot::default() };
        assert!(places.record("", "default", "argus", spot.clone()));
        assert!(!places.record("", "default", "argus", spot.clone()), "the same place changes nothing");
        assert_eq!(places.workspace(""), Some("default"));
        assert_eq!(places.project("", "default"), Some("argus"));
        assert_eq!(places.spot("", "default", "argus"), Some(&spot));
        assert_eq!(places.spot("box", "default", "argus"), None, "another host is another place");
    }

    #[test]
    fn places_saved_are_the_places_loaded() {
        // The file is the person's across window restarts (the module header).
        let mut places = Places::default();
        places.record("box", "work", "web", Spot { repository: Some("site".into()), beside: vec![2, 3], ..Spot::default() });
        let json = serde_json::to_string(&places).unwrap();
        assert_eq!(serde_json::from_str::<Places>(&json).unwrap(), places);
    }
}
