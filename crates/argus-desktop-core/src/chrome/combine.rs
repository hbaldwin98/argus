//! What the combine-projects dialog holds — the project folded into, the projects ticked to fold in
//! and the ask waiting on the daemon — and what the page last heard of it.
//!
//! The daemon does the folding (`ClientMsg::CombineProjects`) and the dialog goes when it answers;
//! until then the tree is untouched, so a refusal leaves the projects as they are and says why here.
//! What ticking would hold is read off the tree and is only a reading: the daemon decides.

use argus_protocol::{ClientMsg, PaneKind, PaneStatus, ProjectId, ProjectInfo, RepositoryInfo, RequestRefusal};

use super::add_page::ProjectRef;
use super::combine_page::{CombinePage, CombineRow, IntoChoice};
use crate::connection::page::Event;
use crate::connection::sent::Sent;

pub struct Combine {
    into: ProjectId,
    ticked: Vec<ProjectId>,
    /// The ask for the daemon to fold, until it answers.
    waiting: Option<u64>,
    refusal: Option<String>,
}

impl Combine {
    /// Open on `into`, with `tick` already ticked: the project a repository's note named.
    pub fn new(into: ProjectId, tick: Option<ProjectId>) -> Combine {
        let mut dialog = Combine { into, ticked: Vec::new(), waiting: None, refusal: None };
        if let Some(project) = tick {
            dialog.tick(project);
        }
        dialog
    }

    /// Folds into `project` instead. It cannot also be folded in, so it is unticked.
    pub fn set_into(&mut self, project: ProjectId) {
        if self.waiting.is_some() {
            return;
        }
        self.into = project;
        self.ticked.retain(|p| *p != project);
        self.refusal = None;
    }

    pub fn tick(&mut self, project: ProjectId) {
        if self.waiting.is_some() || project == self.into {
            return;
        }
        match self.ticked.iter().position(|p| *p == project) {
            Some(at) => {
                self.ticked.remove(at);
            }
            None => self.ticked.push(project),
        }
        self.refusal = None;
    }

    /// The ask, named by `request()`, or none while one is with the daemon or nothing is ticked.
    /// The dialog stays up until the daemon has answered, so a refusal keeps what was ticked.
    pub fn combine(&mut self, tree: &[ProjectInfo], request: impl FnOnce() -> u64) -> Option<ClientMsg> {
        let from: Vec<ProjectId> = tree.iter().map(|p| p.id).filter(|id| *id != self.into && self.ticked.contains(id)).collect();
        if self.waiting.is_some() || from.is_empty() || !tree.iter().any(|p| p.id == self.into) {
            return None;
        }
        let request_id = request();
        self.waiting = Some(request_id);
        self.refusal = None;
        Some(ClientMsg::CombineProjects { into: self.into, from, request_id })
    }

    /// Takes the daemon's refusal of the ask waiting on it; false for any other request's.
    pub fn refused(&mut self, refusal: &RequestRefusal) -> bool {
        if self.waiting != Some(refusal.request_id) {
            return false;
        }
        self.waiting = None;
        self.refusal = Some(refusal.reason.clone());
        true
    }

    /// A new connection answers nothing the last was asked: the tree will say whether it was done.
    pub fn reconnected(&mut self) {
        self.waiting = None;
    }

    /// The dialog as the tree stands; `None` once the project folded into is gone from it.
    pub fn page(&self, tree: &[ProjectInfo]) -> Option<CombinePage> {
        let into = tree.iter().find(|p| p.id == self.into)?;
        let ticked: Vec<&ProjectInfo> = tree.iter().filter(|p| p.id != self.into && self.ticked.contains(&p.id)).collect();
        let held: Vec<&RepositoryInfo> = into.repositories.iter().chain(ticked.iter().flat_map(|p| &p.repositories)).collect();
        let mut paths: Vec<String> = Vec::new();
        let mut holds: Vec<String> = Vec::new();
        for repository in held {
            let path = path_of(repository);
            if paths.contains(&path) {
                continue;
            }
            paths.push(path);
            if !holds.contains(&repository.name) {
                holds.push(repository.name.clone());
            }
        }
        Some(CombinePage {
            open: true,
            into: Some(ProjectRef { id: into.id.0, name: into.name.clone() }),
            choices: tree.iter().map(|p| IntoChoice { id: p.id.0, name: p.name.clone(), repositories: p.repositories.len() }).collect(),
            rows: tree
                .iter()
                .filter(|p| p.id != self.into)
                .map(|p| CombineRow {
                    id: p.id.0,
                    name: p.name.clone(),
                    repositories: p.repositories.len(),
                    needs_you: statuses(p).filter(|s| s.needs_you()).count(),
                    working: statuses(p).filter(|s| *s == PaneStatus::Working).count(),
                    scans_folder: p.root.is_some(),
                    ticked: self.ticked.contains(&p.id),
                })
                .collect(),
            holds,
            repositories: paths.len(),
            agents: ticked.iter().map(|p| agents(p)).sum(),
            combining: self.waiting.is_some(),
            refusal: self.refusal.clone(),
        })
    }
}

fn statuses(project: &ProjectInfo) -> impl Iterator<Item = PaneStatus> + '_ {
    project.repositories.iter().flat_map(|r| r.checkouts.iter()).flat_map(|c| c.statuses())
}

/// Agents still running in a project, which the daemon carries on in the repositories it moves.
fn agents(project: &ProjectInfo) -> usize {
    project
        .repositories
        .iter()
        .flat_map(|r| &r.checkouts)
        .flat_map(|c| &c.panes)
        .filter(|pane| pane.kind == PaneKind::Agent && !matches!(pane.status, PaneStatus::Exited { .. }))
        .count()
}

/// Where a repository is, which is what the daemon takes for the same one: the primary checkout's
/// path, spelled with one separator.
fn path_of(repository: &RepositoryInfo) -> String {
    let checkout = repository.checkouts.iter().find(|c| c.primary).or(repository.checkouts.first());
    checkout.map_or_else(|| repository.name.clone(), |c| c.path.replace('\\', "/").trim_end_matches('/').to_owned())
}

/// The dialog as the window holds it, while open, and what the page last heard of it.
#[derive(Default)]
pub struct Combining {
    dialog: Option<Combine>,
    sent: Sent<CombinePage>,
}

impl Combining {
    pub fn open(&mut self, dialog: Combine) {
        self.dialog = Some(dialog);
    }

    pub fn close(&mut self) {
        self.dialog = None;
    }

    pub fn get_mut(&mut self) -> Option<&mut Combine> {
        self.dialog.as_mut()
    }

    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }

    /// What the page has not heard of the dialog. One whose project left the tree goes.
    pub fn take(&mut self, tree: &[ProjectInfo]) -> Option<Event> {
        let page = self.dialog.as_ref().and_then(|dialog| dialog.page(tree));
        if page.is_none() {
            self.dialog = None;
        }
        self.sent.take(page).map(|page| Event::Combine(Box::new(page)))
    }

    /// The page reloaded and holds nothing: an open dialog is sent again.
    pub fn reload(&mut self) {
        self.sent.forget();
    }
}

#[cfg(test)]
mod tests;
