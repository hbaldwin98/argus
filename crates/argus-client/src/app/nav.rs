//! Where you are in the tree, and what moving does.
//!
//! Selection is an index per column, not a pointer: the daemon reissues the
//! tree on every structural change, and an index survives that where a
//! borrowed row would not. Everything here is therefore either reading the
//! row an index currently names, or moving an index and clamping the ones
//! below it back into range.

use super::*;

impl App {
    pub fn current_project(&self) -> Option<&ProjectInfo> {
        self.tree.get(self.sel_project)
    }

    pub fn current_repository(&self) -> Option<&RepositoryInfo> {
        self.current_project()
            .and_then(|p| p.repositories.get(self.sel_repository))
    }

    pub fn current_checkout(&self) -> Option<&CheckoutInfo> {
        match self.checkout_rows().get(self.sel_checkout).copied()? {
            CheckoutRow::Checkout(i) => self.current_repository()?.checkouts.get(i),
            CheckoutRow::Branch(_) | CheckoutRow::Remote(_) => None,
        }
    }

    /// The checkouts column, in the order it is drawn.
    ///
    /// The main branch leads it either way — as the checkout sitting on it,
    /// or as the offer of one — so that the branch everything is measured
    /// against is always the row at the top. The remaining branches come
    /// last and only while the column is expanded: a repository with forty
    /// of them would otherwise bury the handful of checkouts that are the
    /// point of the column.
    pub fn checkout_rows(&self) -> Vec<CheckoutRow> {
        let Some(r) = self.current_repository() else {
            return Vec::new();
        };
        let default = r.default_branch.as_deref();
        // `branches` only holds the ones no checkout has, so a hit here
        // means the main branch has no directory and needs a row of its own.
        let pinned = default.and_then(|d| r.branches.iter().position(|b| b == d));
        let leads = |i: &usize| default.is_some_and(|d| on_branch(&r.checkouts[*i], d));

        let mut rows: Vec<CheckoutRow> = pinned.map(CheckoutRow::Branch).into_iter().collect();
        rows.extend(
            (0..r.checkouts.len())
                .filter(leads)
                .map(CheckoutRow::Checkout),
        );
        rows.extend(
            (0..r.checkouts.len())
                .filter(|i| !leads(i))
                .map(CheckoutRow::Checkout),
        );
        if self.show_branches {
            rows.extend(
                (0..r.branches.len())
                    .filter(|i| Some(*i) != pinned)
                    .map(CheckoutRow::Branch),
            );
            // What a fetch turned up comes last: it is the furthest from
            // being somewhere you can work.
            rows.extend((0..r.remote_branches.len()).map(CheckoutRow::Remote));
        }
        rows
    }

    /// The selected row when it is a branch nothing is sitting on.
    /// Everything reached through `current_checkout` — panes, review,
    /// spawning — is `None` there, which is what a branch with no directory
    /// has to offer.
    /// The selected row when it is a branch nothing is sitting on, as the
    /// branch it would be locally. A remote-only row answers here too: what
    /// you switch to, or make a worktree of, is `feature`, and git is left
    /// to notice that it comes from `origin/feature`.
    pub fn current_branch_row(&self) -> Option<&str> {
        let r = self.current_repository()?;
        match self.checkout_rows().get(self.sel_checkout).copied()? {
            CheckoutRow::Branch(i) => r.branches.get(i).map(String::as_str),
            CheckoutRow::Remote(i) => r.remote_branches.get(i).and_then(|b| local_name(b)),
            CheckoutRow::Checkout(_) => None,
        }
    }

    /// The same row as `origin/feature`, for the places that have to care
    /// which side of the remote it is on.
    pub fn current_remote_row(&self) -> Option<&str> {
        match self.checkout_rows().get(self.sel_checkout).copied()? {
            CheckoutRow::Remote(i) => self
                .current_repository()?
                .remote_branches
                .get(i)
                .map(String::as_str),
            _ => None,
        }
    }

    pub fn checkout_row_count(&self) -> usize {
        self.checkout_rows().len()
    }

    /// The row the checkout at `index` is drawn on.
    ///
    /// `sel_checkout` is a position in [`App::checkout_rows`], and a
    /// checkout's position in `checkouts` is not one: the main branch leads
    /// the column either way, so it takes a row of its own as soon as no
    /// checkout is sitting on it, and every checkout slides down past it.
    /// Anything that finds a checkout by walking `checkouts` has to come
    /// back through here before it can point the cursor at it.
    pub(super) fn checkout_row_of(&self, index: usize) -> Option<usize> {
        self.checkout_rows()
            .iter()
            .position(|row| matches!(row, CheckoutRow::Checkout(i) if *i == index))
    }

    /// The inverse: where the selected row sits in `checkouts`, for the
    /// places that compare a cursor against one.
    fn selected_checkout_index(&self) -> Option<usize> {
        match self.checkout_rows().get(self.sel_checkout).copied()? {
            CheckoutRow::Checkout(i) => Some(i),
            CheckoutRow::Branch(_) | CheckoutRow::Remote(_) => None,
        }
    }

    /// The selected checkout row as an identity, paired with the repository
    /// it belongs to. Taken before a new tree replaces the old one.
    pub(super) fn checkout_anchor(&self) -> Option<(RepositoryId, CheckoutAnchor)> {
        let r = self.current_repository()?;
        let row = self.checkout_rows().get(self.sel_checkout).copied()?;
        let anchor = match row {
            CheckoutRow::Checkout(i) => CheckoutAnchor::Checkout(r.checkouts.get(i)?.id),
            CheckoutRow::Branch(i) => CheckoutAnchor::Branch(r.branches.get(i)?.clone()),
            CheckoutRow::Remote(i) => CheckoutAnchor::Remote(r.remote_branches.get(i)?.clone()),
        };
        Some((r.id, anchor))
    }

    /// Puts the cursor back on the row the anchor names. The column's order
    /// is not stable across trees: a checkout whose git status has not
    /// landed yet leaves its own branch looking unoccupied, which pins a
    /// branch row above every checkout and slides a bare index up by one.
    /// Repeat that and the selection walks to the top row, which is exactly
    /// where the main branch is pinned.
    pub(super) fn restore_checkout_anchor(
        &mut self,
        repository: RepositoryId,
        anchor: &CheckoutAnchor,
    ) {
        let Some((project_index, repository_index)) =
            self.tree
                .iter()
                .enumerate()
                .find_map(|(project_index, project)| {
                    project
                        .repositories
                        .iter()
                        .position(|r| r.id == repository)
                        .map(|repository_index| (project_index, repository_index))
                })
        else {
            return;
        };
        self.sel_project = project_index;
        self.sel_repository = repository_index;
        let Some(r) = self.current_repository() else {
            return;
        };
        let rows = self.checkout_rows();
        let found = rows.iter().position(|row| match (row, anchor) {
            (CheckoutRow::Checkout(i), CheckoutAnchor::Checkout(id)) => {
                r.checkouts.get(*i).is_some_and(|c| c.id == *id)
            }
            (CheckoutRow::Branch(i), CheckoutAnchor::Branch(name)) => {
                r.branches.get(*i) == Some(name)
            }
            (CheckoutRow::Remote(i), CheckoutAnchor::Remote(name)) => {
                r.remote_branches.get(*i) == Some(name)
            }
            // A branch that has just been given a directory is still the
            // row the user was on, so follow it into its new checkout.
            (CheckoutRow::Checkout(i), CheckoutAnchor::Branch(name)) => {
                r.checkouts.get(*i).is_some_and(|c| on_branch(c, name))
            }
            _ => false,
        });
        if let Some(index) = found {
            self.sel_checkout = index;
        }
    }

    /// The checkout a repository-wide action runs in: its primary one,
    /// which is where a branch without a directory would be switched to.
    pub(super) fn primary_checkout(&self) -> Option<&CheckoutInfo> {
        self.current_repository()?
            .checkouts
            .iter()
            .find(|c| c.primary)
    }

    pub fn current_pane(&self) -> Option<&PaneInfo> {
        self.current_checkout()
            .and_then(|c| c.listed_panes().nth(self.sel_pane))
    }

    pub fn pane_location(&self) -> Option<PaneLocation> {
        self.current_pane()?;
        Some(PaneLocation {
            project: self.sel_project,
            repository: self.sel_repository,
            checkout: self.selected_checkout_index()?,
            pane: self.sel_pane,
        })
    }

    /// Every listed pane in the open workspace, in tree order.
    pub fn flat_pane_locations(&self) -> Vec<PaneLocation> {
        self.tree
            .iter()
            .enumerate()
            .flat_map(|(project, p)| {
                p.repositories
                    .iter()
                    .enumerate()
                    .flat_map(move |(repository, r)| {
                        r.checkouts
                            .iter()
                            .enumerate()
                            .flat_map(move |(checkout, c)| {
                                c.listed_panes()
                                    .enumerate()
                                    .map(move |(pane, _)| PaneLocation {
                                        project,
                                        repository,
                                        checkout,
                                        pane,
                                    })
                            })
                    })
            })
            .collect()
    }

    pub fn pane_column_locations(&self) -> Vec<PaneLocation> {
        if self.settings.pane_view == crate::settings::PaneView::Flat {
            return self.flat_pane_locations();
        }
        let (Some(checkout), Some(checkout_index)) =
            (self.current_checkout(), self.selected_checkout_index())
        else {
            return Vec::new();
        };
        (0..checkout.listed_panes().count())
            .map(|pane| PaneLocation {
                project: self.sel_project,
                repository: self.sel_repository,
                checkout: checkout_index,
                pane,
            })
            .collect()
    }

    pub fn pane_at(&self, location: PaneLocation) -> Option<&PaneInfo> {
        self.tree
            .get(location.project)?
            .repositories
            .get(location.repository)?
            .checkouts
            .get(location.checkout)?
            .listed_panes()
            .nth(location.pane)
    }

    pub fn pane_path(&self, location: PaneLocation) -> Option<(&str, &str, &str)> {
        let project = self.tree.get(location.project)?;
        let repository = project.repositories.get(location.repository)?;
        let checkout = repository.checkouts.get(location.checkout)?;
        Some((&project.name, &repository.name, &checkout.name))
    }

    pub fn select_pane_location(&mut self, location: PaneLocation) -> bool {
        let valid = self.pane_at(location).is_some();
        if !valid {
            return false;
        }
        self.sel_project = location.project;
        self.sel_repository = location.repository;
        let Some(row) = self.checkout_row_of(location.checkout) else {
            return false;
        };
        self.sel_checkout = row;
        self.sel_pane = location.pane;
        true
    }

    pub(super) fn clamp(&mut self) {
        let nproj = self.tree.len();
        if nproj == 0 {
            self.sel_project = 0;
        } else if self.sel_project >= nproj {
            self.sel_project = nproj - 1;
        }
        let nrepo = self
            .current_project()
            .map(|p| p.repositories.len())
            .unwrap_or(0);
        if nrepo == 0 {
            self.sel_repository = 0;
        } else if self.sel_repository >= nrepo {
            self.sel_repository = nrepo - 1;
        }
        let ncheck = self.checkout_row_count();
        if ncheck == 0 {
            self.sel_checkout = 0;
        } else if self.sel_checkout >= ncheck {
            self.sel_checkout = ncheck - 1;
        }
        let npane = self.visible_pane_count();
        if npane == 0 {
            self.sel_pane = 0;
        } else if self.sel_pane >= npane {
            self.sel_pane = npane - 1;
        }
        self.sync_subscription();
    }

    /// Keeps the live view subscribed to whatever pane current navigation
    /// state implies, independent of `focus` — the rightmost column always
    /// shows this pane's content alongside the project/checkout columns,
    /// it never takes over the whole screen.
    pub(super) fn visible_pane_count(&self) -> usize {
        self.current_checkout()
            .map(|c| c.listed_panes().count())
            .unwrap_or(0)
    }

    /// The pane the rightmost column draws: whatever the tree selection
    /// implies, untouched by anything floating above it.
    pub fn column_pane(&self) -> Option<PaneId> {
        self.current_pane().map(|p| p.id)
    }

    pub fn overlay_pane(&self) -> Option<PaneId> {
        self.overlay.as_ref().and_then(Overlay::pane)
    }

    pub(super) fn jump_to_next_attention(&mut self) {
        // Compared against candidates walked out of `checkouts`, so the
        // cursor has to be read back as one of those positions rather than
        // as the row it is drawn on.
        let current = (
            self.sel_project,
            self.sel_repository,
            self.selected_checkout_index().unwrap_or(0),
            self.sel_pane,
        );
        let candidates: Vec<_> = self
            .tree
            .iter()
            .enumerate()
            .flat_map(|(project_index, project)| {
                project.repositories.iter().enumerate().flat_map(
                    move |(repository_index, repository)| {
                        repository.checkouts.iter().enumerate().flat_map(
                            move |(checkout_index, checkout)| {
                                checkout.listed_panes().enumerate().filter_map(
                                    move |(pane_index, pane)| {
                                        let (label, note) = attention_of(pane)?;
                                        Some((
                                            project_index,
                                            repository_index,
                                            checkout_index,
                                            pane_index,
                                            label,
                                            note,
                                        ))
                                    },
                                )
                            },
                        )
                    },
                )
            })
            .collect();

        let Some(next) = candidates
            .iter()
            .find(|candidate| (candidate.0, candidate.1, candidate.2, candidate.3) > current)
            .or_else(|| candidates.first())
        else {
            self.report("no panes need attention");
            return;
        };

        self.sel_project = next.0;
        self.sel_repository = next.1;
        self.sel_checkout = self.checkout_row_of(next.2).unwrap_or(0);
        self.sel_pane = next.3;
        self.focus = Focus::PaneContent;
        let status = next
            .5
            .as_deref()
            .map(|note| format!("{}: {note}", next.4))
            .unwrap_or_else(|| format!("attention: {}", next.4));
        self.report(status);
        self.sync_subscription();
    }

    pub(super) fn selection_in(&self, target: Focus) -> usize {
        match target {
            Focus::Projects => self.sel_project,
            Focus::Repositories => self.sel_repository,
            Focus::Checkouts => self.sel_checkout,
            _ => self.sel_pane,
        }
    }

    pub(super) fn selection_mut(&mut self, target: Focus) -> &mut usize {
        match target {
            Focus::Projects => &mut self.sel_project,
            Focus::Repositories => &mut self.sel_repository,
            Focus::Checkouts => &mut self.sel_checkout,
            _ => &mut self.sel_pane,
        }
    }

    pub(super) fn move_selection(&mut self, delta: i32) {
        self.adjust_selection(self.focus, delta);
    }

    pub(super) fn adjust_selection(&mut self, target: Focus, delta: i32) {
        if target == Focus::Panes && self.settings.pane_view == crate::settings::PaneView::Flat {
            let locations = self.flat_pane_locations();
            if locations.is_empty() {
                return;
            }
            let here = self
                .pane_location()
                .and_then(|current| locations.iter().position(|location| *location == current))
                .unwrap_or(0) as i32;
            let next = (here + delta).clamp(0, locations.len() as i32 - 1) as usize;
            self.select_pane_location(locations[next]);
            self.sync_subscription();
            return;
        }
        let sel = match target {
            Focus::Projects => &mut self.sel_project,
            Focus::Repositories => &mut self.sel_repository,
            Focus::Checkouts => &mut self.sel_checkout,
            Focus::Panes => &mut self.sel_pane,
            Focus::PaneContent | Focus::Review | Focus::Overlay => return,
        };
        let new = *sel as i32 + delta;
        if new >= 0 {
            *sel = new as usize;
        }
        self.clamp();
    }

    pub(super) fn descend(&mut self) {
        match self.focus {
            Focus::Projects => {
                if self.current_project().is_some() {
                    self.sel_repository = 0;
                    self.focus = Focus::Repositories;
                }
            }
            Focus::Repositories => {
                if self.current_repository().is_some() {
                    self.sel_checkout = 0;
                    self.focus = Focus::Checkouts;
                }
            }
            Focus::Checkouts => {
                if self.current_checkout().is_some() {
                    self.sel_pane = 0;
                    if self.settings.pane_view == crate::settings::PaneView::Flat
                        && self.current_pane().is_none()
                    {
                        if let Some(first) = self.flat_pane_locations().first().copied() {
                            self.select_pane_location(first);
                        }
                    }
                    self.focus = Focus::Panes;
                } else if self.current_branch_row().is_some() {
                    // A branch with no directory has no panes to descend
                    // into; what it offers instead is somewhere to be.
                    self.switch_primary_to_selected_branch();
                }
            }
            Focus::Panes => {
                if self.current_pane().is_some() {
                    self.focus = Focus::PaneContent;
                }
            }
            Focus::PaneContent | Focus::Review | Focus::Overlay => {}
        }
    }

    pub(super) fn ascend(&mut self) {
        match self.focus {
            Focus::PaneContent => {
                // Deliberately does not unsubscribe: the live view keeps
                // showing this pane in the rightmost column while browsing.
                self.leader_pending = false;
                self.pane_fullscreen = false;
                self.focus = Focus::Panes;
            }
            Focus::Panes => self.focus = Focus::Checkouts,
            Focus::Checkouts => self.focus = Focus::Repositories,
            Focus::Repositories => {
                // Ascending into a folded-away projects column would park the
                // cursor on a tab with no rows, so it stays put instead.
                if !self.projects_collapsed {
                    self.focus = Focus::Projects;
                }
            }
            Focus::Projects => {}
            Focus::Review | Focus::Overlay => self.focus = Focus::Checkouts,
        }
    }

    /// Back to the top of the tree. Anything that swaps the whole project
    /// column out from under the columns needs it: the old indices refer
    /// to rows that are no longer there. If the projects column is
    /// collapsed there is nothing for it to park the cursor on, so land on
    /// repositories — the same place a collapsed startup does.
    pub(super) fn reset_navigation(&mut self) {
        self.sel_project = 0;
        self.sel_repository = 0;
        self.sel_checkout = 0;
        self.sel_pane = 0;
        self.focus = if self.projects_collapsed {
            Focus::Repositories
        } else {
            Focus::Projects
        };
    }
}
