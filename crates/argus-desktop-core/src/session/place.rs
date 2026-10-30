//! How the session lands where the person was when it switches project,
//! workspace or host, and notes where they are as they go.
//!
//! The memory itself is `chrome::place`; what it holds is read off the
//! session here and put back on it, and only what the current tree holds is
//! put back: a pane that has closed is replaced by another of its checkout
//! or by nothing, and no request goes to the daemon for something it lacks.

use super::*;
use crate::chrome::place::{Spot, Stage as Remembered};

impl Session {
    /// The key of the daemon the window reaches: this machine's is empty.
    fn host_key(&self) -> String {
        self.status.host().unwrap_or("").to_string()
    }

    /// Notes where the person is in the open project. Nothing is noted until
    /// the workspace has been settled, since the tree can name a workspace's
    /// projects before the list of workspaces says which is open, and what
    /// is on screen then belongs to no place yet.
    pub(super) fn remember_place(&mut self) {
        let (Some(workspace), Some(project)) = (self.placed.clone(), self.current_project()) else {
            return;
        };
        let name = project.name.clone();
        let (host, spot) = (self.host_key(), self.spot_here());
        if self.places.record(&host, &workspace, &name, spot) {
            self.places_unsaved = true;
        }
    }

    fn spot_here(&self) -> Spot {
        let Some(project) = self.current_project() else {
            return Spot::default();
        };
        let repository = self
            .repository
            .and_then(|id| project.repositories.iter().find(|r| r.id == id))
            .map(|r| r.name.clone());
        let stage = match self.stages.now() {
            Stage::Feature => Remembered::Feature,
            Stage::Checkouts => Remembered::Checkouts,
            _ => Remembered::Workspace,
        };
        // A pane shown from another project is that project's to remember.
        let ours = |pane: &PaneId| self.project_of(*pane) == Some(project.id);
        let grid: Vec<PaneId> = self.shown.panes().into_iter().filter(|&p| self.shown.grid().holds(p)).filter(ours).collect();
        let pane = self.shown.on(Side::Workspace).filter(ours);
        let checkout = pane.and_then(|p| self.tree.lineage_of_pane(p)).map(|(lineage, _)| lineage.checkout.path.clone());
        Spot {
            repository,
            stage,
            pane: pane.map(|p| p.0),
            checkout,
            beside: grid.iter().filter(|&&p| Some(p) != pane).map(|p| p.0).collect(),
            keys: self.shown.pane().filter(|p| grid.contains(p)).map(|p| p.0),
        }
    }

    /// The project's pane the spot names, else another of the checkout it
    /// was in. The id alone is not trusted when the checkout is known: a
    /// restarted daemon hands ids out again, to other panes.
    fn surviving_pane(&self, project: &ProjectInfo, pane: Option<u64>, checkout: Option<&str>) -> Option<PaneId> {
        let checkouts = || project.repositories.iter().flat_map(|r| r.checkouts.iter());
        let named = pane.and_then(|id| {
            checkouts()
                .find(|c| c.listed_panes().any(|p| p.id.0 == id))
                .filter(|c| checkout.is_none_or(|path| c.path == path))
                .map(|_| PaneId(id))
        });
        named.or_else(|| {
            let path = checkout?;
            checkouts().find(|c| c.path == path)?.listed_panes().next().map(|p| p.id)
        })
    }

    /// Puts back what `project` showed when the person left it. A project
    /// never visited shows what today's switch shows: an empty Workspace.
    pub(super) fn return_to(&mut self, project: ProjectId) {
        let Some(workspace) = self.placed.clone() else {
            return;
        };
        let Some(info) = self.tree.iter().find(|p| p.id == project).cloned() else {
            return;
        };
        let spot = self.places.spot(&self.host_key(), &workspace, &info.name).cloned();
        if let Some(spot) = spot {
            let mut panes: Vec<PaneId> = Vec::new();
            panes.extend(self.surviving_pane(&info, spot.pane, spot.checkout.as_deref()));
            for beside in &spot.beside {
                let held = self.surviving_pane(&info, Some(*beside), None);
                panes.extend(held.filter(|p| !panes.contains(p)));
            }
            let mut panes = panes.into_iter();
            if let Some(first) = panes.next() {
                self.show(first);
            }
            for pane in panes {
                // Refused for want of room: that pane stays listed, as it was.
                let _ = self.join_grid(pane);
            }
            if let Some(keys) = spot.keys.map(PaneId).filter(|&p| self.shown.grid().holds(p)) {
                self.shown.focus(keys);
            }
            self.repository = spot.repository.and_then(|name| info.repositories.iter().find(|r| r.name == name)).map(|r| r.id);
            match spot.stage {
                Remembered::Workspace => self.leave_stages(),
                Remembered::Feature => self.go(Stage::Feature),
                Remembered::Checkouts => self.go(Stage::Checkouts),
            }
        }
        self.remember_place();
    }

    /// Lands the window once the daemon has said which workspace is open and
    /// sent its tree: back in the workspace the host was last in, and in the
    /// project that workspace was last in. Run on every tree and every list
    /// of workspaces, since either can arrive last. `trusting` is a list
    /// that arrived after the tree changed, which therefore describes it;
    /// any other is believed only if the tree holds the project it says
    /// the workspace was last in, since a list that lags the tree names the
    /// workspace the tree just left.
    pub(super) fn settle_place(&mut self, trusting: bool) {
        let Some(open) = self.switcher.open().filter(|_| self.loaded).cloned() else {
            return;
        };
        // The tree and the list arrive separately; a tree of another size is
        // not this workspace's yet.
        if open.projects != self.tree.len() {
            return;
        }
        let host = self.host_key();
        let remembered = self.places.project(&host, &open.name);
        if !trusting && remembered.is_some_and(|name| !self.tree.iter().any(|p| p.name == name)) {
            return;
        }
        if !std::mem::replace(&mut self.host_visited, true) {
            let wanted = self.places.workspace(&host).filter(|name| *name != open.name);
            if let Some(id) = wanted.and_then(|name| self.switcher.id_of(name)) {
                self.out.extend(self.switcher.open_workspace(id));
                return;
            }
        }
        if self.placed.as_deref() == Some(open.name.as_str()) {
            return;
        }
        self.placed = Some(open.name.clone());
        self.places_unsaved |= self.places.record_workspace(&host, &open.name);
        let last = self.places.project(&host, &open.name).and_then(|name| self.tree.iter().find(|p| p.name == name));
        match last.map(|p| p.id) {
            Some(project) => {
                if self.project != Some(project) {
                    self.project = Some(project);
                    self.hide();
                }
                self.return_to(project);
            }
            None => self.remember_place(),
        }
    }
}

#[cfg(test)]
mod tests;
