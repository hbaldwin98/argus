//! What the project switcher says: the entries of the menu under the brand, how a project's tally
//! reads, and the words of the question before a project is removed (the page's
//! `chrome/projects.ts`).
//!
//! The switcher offers the tree's projects, then what can be done about them. Which of them are
//! there, in what order, and what each says is decided here, so the page and the GPUI window list
//! the same menu; the way a list unfolds is the surface's own. Choosing asks the session and
//! nothing else: the menu shows a project, workspace or host as chosen when the daemon says so.

use crate::connection::page::{Call, HostChoice, ProjectChoice, Rail, Removing, WorkspaceChoice};

/// `1 repository`, `2 repositories`.
pub fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// How loudly a tally says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Somebody is needed.
    Loud,
    /// Work is under way.
    Active,
    Muted,
}

/// What a project's row says of its agents: who needs you, else who is working, else that it is
/// quiet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tally {
    pub words: String,
    pub tone: Tone,
}

pub fn tally(choice: &ProjectChoice) -> Tally {
    if choice.needs_you > 0 {
        Tally { words: format!("{} need you", choice.needs_you), tone: Tone::Loud }
    } else if choice.working > 0 {
        Tally { words: format!("{} working", choice.working), tone: Tone::Active }
    } else {
        Tally { words: "quiet".to_owned(), tone: Tone::Muted }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub id: u64,
    pub name: String,
    pub open: bool,
    /// `3 projects`.
    pub count: String,
}

impl WorkspaceRow {
    /// Opening a workspace is the daemon's and every client follows it: the one already open asks
    /// nothing.
    pub fn choose(&self) -> Option<Call> {
        (!self.open).then_some(Call::OpenWorkspace { workspace: self.id })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRow {
    pub name: String,
    /// The host to ask for; `None` is this machine.
    pub host: Option<String>,
    pub current: bool,
}

impl HostRow {
    /// Choosing the host being reached asks nothing.
    pub fn choose(&self) -> Option<Call> {
        (!self.current).then(|| Call::SetHost { host: self.host.clone() })
    }
}

/// What the hosts' list says when the ssh config names none.
pub const NO_HOSTS: &str = "No hosts in ~/.ssh/config, or set ARGUS_DESKTOP_HOST";

/// What a press on Connect to a host... explains, as its tooltip.
pub const HOSTS_TIP: &str = "Reach the daemon on another machine over ssh, as argus --host does";

/// The end of the workspace list: the row that makes another.
pub const NEW_WORKSPACE: &str = "New workspace\u{2026}";

/// What the new-workspace dialog says: its title, the field's name and what its keys do.
pub const NEW_WORKSPACE_TITLE: &str = "New workspace";
pub const NEW_WORKSPACE_PLACEHOLDER: &str = "work";
pub const NEW_WORKSPACE_HINT: &str = "Enter creates and opens it \u{b7} Esc cancels";

/// One entry of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// `Projects`, only when there are projects.
    Label(&'static str),
    Project { id: u64, name: String, current: bool, tally: Tally },
    Rule,
    AddProject,
    /// Only when the daemon lists workspaces; `open` is the open one's name.
    Workspaces { open: Option<String>, rows: Vec<WorkspaceRow> },
    Hosts { connected: Option<String>, rows: Vec<HostRow> },
    /// Only with a project: asks before it removes.
    Remove { project: u64, name: String },
}

/// The menu for what the rail last said. It is there once the tree has loaded, with or without a
/// project, since it is how the first one is added.
pub fn menu(rail: &Rail) -> Vec<Entry> {
    let current = rail.project.as_ref();
    let mut entries = Vec::new();
    if !rail.projects.is_empty() {
        entries.push(Entry::Label("Projects"));
    }
    entries.extend(rail.projects.iter().map(|choice| Entry::Project {
        id: choice.id,
        name: choice.name.clone(),
        current: Some(choice.id) == current.map(|project| project.id),
        tally: tally(choice),
    }));
    if !rail.projects.is_empty() {
        entries.push(Entry::Rule);
    }
    entries.push(Entry::AddProject);
    if !rail.workspaces.is_empty() {
        entries.push(Entry::Workspaces { open: rail.workspaces.iter().find(|w| w.open).map(|w| w.name.clone()), rows: workspaces(&rail.workspaces) });
    }
    entries.push(Entry::Hosts { connected: rail.hosts.iter().find(|host| host.current).map(|host| host.name.clone()), rows: hosts(&rail.hosts) });
    if let Some(project) = current {
        entries.push(Entry::Rule);
        entries.push(Entry::Remove { project: project.id, name: project.name.clone() });
    }
    entries
}

fn workspaces(listed: &[WorkspaceChoice]) -> Vec<WorkspaceRow> {
    listed
        .iter()
        .map(|w| WorkspaceRow { id: w.id, name: w.name.clone(), open: w.open, count: counted(w.projects, "project", "projects") })
        .collect()
}

/// This machine first, checked when no remote is current, then each host.
fn hosts(listed: &[HostChoice]) -> Vec<HostRow> {
    let remote = listed.iter().any(|host| host.current);
    std::iter::once(HostRow { name: "This machine".to_owned(), host: None, current: !remote })
        .chain(listed.iter().map(|host| HostRow { name: host.name.clone(), host: Some(host.name.clone()), current: host.current }))
        .collect()
}

/// The question before a project is removed, in its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub title: String,
    pub body: String,
    /// Panes still running in it, which argusd waits for.
    pub warning: Option<String>,
}

pub fn question(removing: &Removing) -> Question {
    let panes = removing.panes;
    Question {
        title: format!("Remove {}?", removing.name),
        body: format!(
            "Its {} leave the sidebar. The folders stay where they are, and adding it again brings them back.",
            counted(removing.repositories, "repository", "repositories")
        ),
        warning: (panes > 0).then(|| format!("{panes} pane{} running in it; argusd removes it only once they are closed.", if panes == 1 { " is" } else { "s are" })),
    }
}

#[cfg(test)]
mod tests;
