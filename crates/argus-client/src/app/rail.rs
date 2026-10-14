//! What the rail lists, in what order, which repositories fold away as
//! quiet, which one is open, and what choosing a row does.
//!
//! The renderer draws these rows, a click lands on one of them, and `j`/`k`
//! walk them — so the order on screen is the order the cursor moves in,
//! and everything a click reaches the keys reach too. The cursor is not
//! stored: it is the selection (`sel_*`) read at the depth `focus` names,
//! so a new tree cannot leave the two disagreeing.

use super::*;

/// One row of the rail, in the order it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailRow {
    /// The project heading. Not in the scrolling list, but a stop for the
    /// cursor: `n` adds a project and `D` removes this one from there.
    Project,
    Repository(usize),
    /// A checkout with panes running in it, or the repository's primary
    /// one even when empty, so there is always somewhere to start a shell
    /// or an agent. Another with none has nothing for the rail to say; the
    /// Checkouts stage lists every one.
    Checkout(usize, usize),
    Pane(PaneLocation),
}

/// The line drawn for the quiet repositories. Not a [`RailRow`]: it is a
/// lid over rows rather than somewhere to be, so the cursor never stops on
/// it; `z` and a click work it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuietFold {
    pub open: bool,
    /// Closed, how many repositories it hides; open, how many it revealed.
    pub count: usize,
    /// The repository row it is drawn above, open; closed it trails the list.
    pub above: Option<usize>,
}

impl App {
    /// The repositories the rail lists, the ones with anything running
    /// first: what is active is what the rail is for. The quiet ones fold
    /// away behind [`QuietFold`]'s line while there is an active one to
    /// fold behind, all but the selected one — the cursor and the Checkouts
    /// stage both read `sel_repository`, so hiding it would leave them on a
    /// row nobody can see.
    fn rail_repositories(&self) -> Vec<usize> {
        let (mut listed, quiet) = self.repositories_by_activity();
        let folds = !listed.is_empty() && !self.quiet_open;
        listed.extend(
            quiet
                .into_iter()
                .filter(|index| !folds || *index == self.sel_repository),
        );
        listed
    }

    /// The open project's repositories as (active, quiet), each in tree
    /// order. A repository is quiet when no checkout of it has a listed pane.
    fn repositories_by_activity(&self) -> (Vec<usize>, Vec<usize>) {
        let Some(project) = self.current_project() else {
            return (Vec::new(), Vec::new());
        };
        (0..project.repositories.len()).partition(|index| {
            project.repositories[*index]
                .checkouts
                .iter()
                .any(|checkout| checkout.listed_panes().next().is_some())
        })
    }

    /// The line that stands for the quiet repositories, when there is one
    /// to draw: none while nothing is active, since the list is then the
    /// whole project, and none while it would count zero.
    pub fn quiet_fold(&self) -> Option<QuietFold> {
        let (active, quiet) = self.repositories_by_activity();
        if active.is_empty() {
            return None;
        }
        let fold = if self.quiet_open {
            QuietFold {
                open: true,
                count: quiet.len(),
                above: quiet.first().copied(),
            }
        } else {
            QuietFold {
                open: false,
                count: quiet.iter().filter(|index| **index != self.sel_repository).count(),
                above: None,
            }
        };
        (fold.count > 0).then_some(fold)
    }

    /// `z`, or a click on the fold's line. Without a line there is nothing
    /// to show for an opening, and a fold opened unseen would surprise
    /// whoever next watched a repository go quiet. Closing needs no line:
    /// the one it was opened under may be gone since, every repository
    /// having gone active or quiet together.
    pub(crate) fn toggle_quiet_fold(&mut self) {
        if self.quiet_open {
            self.quiet_open = false;
            return;
        }
        self.quiet_open = self.quiet_fold().is_some();
    }

    /// The one repository the rail shows open: the one the operator opened,
    /// or the one holding the pane the keys are in when that is somewhere
    /// else — a terminal being typed into is never hidden under a closed row.
    pub fn open_repository(&self) -> Option<RepositoryId> {
        let in_pane = self.view == View::Workspace
            && matches!(self.focus, Focus::Panes | Focus::PaneContent);
        match self.current_repository() {
            Some(current) if in_pane => Some(current.id),
            _ => self.opened_repository,
        }
    }

    /// Every row the rail draws, in the order the cursor walks them.
    pub fn rail_rows(&self) -> Vec<RailRow> {
        let Some(project) = self.current_project() else {
            return Vec::new();
        };
        let open = self.open_repository();
        let mut rows = vec![RailRow::Project];
        for repository in self.rail_repositories() {
            rows.push(RailRow::Repository(repository));
            let repo = &project.repositories[repository];
            if Some(repo.id) != open {
                continue;
            }
            for (checkout, item) in repo.checkouts.iter().enumerate() {
                let panes = item.listed_panes().count();
                if panes == 0 && !item.primary {
                    continue;
                }
                rows.push(RailRow::Checkout(repository, checkout));
                rows.extend((0..panes).map(|pane| {
                    RailRow::Pane(PaneLocation {
                        project: self.sel_project,
                        repository,
                        checkout,
                        pane,
                    })
                }));
            }
        }
        rows
    }

    /// The row the rail's cursor is on, while the rail has the keys.
    pub fn rail_cursor(&self) -> Option<RailRow> {
        match self.focus {
            Focus::Projects => Some(RailRow::Project),
            Focus::Repositories => Some(RailRow::Repository(self.sel_repository)),
            Focus::Checkouts => self
                .selected_checkout_index()
                .map(|checkout| RailRow::Checkout(self.sel_repository, checkout)),
            Focus::Panes => self.pane_location().map(RailRow::Pane),
            _ => None,
        }
    }

    /// `j`/`k` and the wheel: one row along what is drawn. A cursor on a
    /// row the rail does not show — a checkout with nothing running, picked
    /// in the Checkouts stage — starts from its repository.
    pub(super) fn step_rail(&mut self, delta: i32) {
        let rows = self.rail_rows();
        if rows.is_empty() {
            return;
        }
        let here = self
            .rail_cursor()
            .and_then(|cursor| rows.iter().position(|row| *row == cursor))
            .or_else(|| {
                let repository = RailRow::Repository(self.sel_repository);
                rows.iter().position(|row| *row == repository)
            })
            .unwrap_or(0) as i32;
        let next = (here + delta).clamp(0, rows.len() as i32 - 1) as usize;
        self.put_rail_cursor(rows[next]);
    }

    /// What choosing a row does, by click: the project heading switches
    /// projects, a repository opens, and a pane takes the stage. The keys
    /// stay on the rail, which is what lets a bare `x` close the pane just
    /// clicked; the terminal itself takes typing.
    pub(crate) fn choose_rail_row(&mut self, row: RailRow) {
        match row {
            RailRow::Project => self.open_project_picker(),
            RailRow::Repository(_) => {
                self.put_rail_cursor(row);
                self.open_selected_repository();
            }
            RailRow::Checkout(..) => self.put_rail_cursor(row),
            RailRow::Pane(location) => {
                self.select_pane_location(location);
                self.open_view(View::Workspace);
                self.focus = Focus::Panes;
                self.clamp();
            }
        }
    }

    /// `l`/Enter: into the row the cursor is on. A repository opens and the
    /// cursor goes to what it holds; a checkout goes to its first pane; a
    /// pane takes the keys.
    pub(super) fn enter_rail_row(&mut self) {
        let Some(cursor) = self.rail_cursor() else {
            // A branch with no directory, picked in the Checkouts stage,
            // has no row here; what it offers instead is somewhere to be.
            if self.focus == Focus::Checkouts && self.current_branch_row().is_some() {
                self.switch_primary_to_selected_branch();
            }
            return;
        };
        match cursor {
            RailRow::Project => {
                if let Some(first) = self.rail_repositories().first().copied() {
                    self.put_rail_cursor(RailRow::Repository(first));
                }
            }
            RailRow::Repository(repository) => {
                self.open_selected_repository();
                let child = self.rail_rows().into_iter().find(
                    |row| matches!(row, RailRow::Checkout(r, _) if *r == repository),
                );
                if let Some(child) = child {
                    self.put_rail_cursor(child);
                }
            }
            RailRow::Checkout(repository, checkout) => {
                let pane = PaneLocation {
                    project: self.sel_project,
                    repository,
                    checkout,
                    pane: 0,
                };
                if self.pane_at(pane).is_some() {
                    self.put_rail_cursor(RailRow::Pane(pane));
                }
            }
            RailRow::Pane(_) => {
                self.open_view(View::Workspace);
                self.focus = Focus::PaneContent;
            }
        }
    }

    /// `h`/Esc: up to the row this one hangs under.
    pub(super) fn leave_rail_row(&mut self) {
        self.focus = match self.focus {
            Focus::Panes => Focus::Checkouts,
            Focus::Checkouts => Focus::Repositories,
            Focus::Repositories | Focus::Projects => Focus::Projects,
            other => other,
        };
    }

    /// Puts the cursor on `row` without opening anything: stepping past a
    /// repository does not expand it, or walking the list would unfold
    /// every row it crossed.
    fn put_rail_cursor(&mut self, row: RailRow) {
        match row {
            RailRow::Project => self.focus = Focus::Projects,
            RailRow::Repository(repository) => {
                if repository != self.sel_repository {
                    self.sel_repository = repository;
                    self.sel_checkout = self.home_checkout_row();
                    self.sel_pane = 0;
                }
                self.focus = Focus::Repositories;
            }
            RailRow::Checkout(repository, checkout) => {
                self.sel_repository = repository;
                self.sel_checkout = self.checkout_row_of(checkout).unwrap_or(0);
                self.sel_pane = 0;
                self.focus = Focus::Checkouts;
            }
            RailRow::Pane(location) => {
                self.select_pane_location(location);
                self.focus = Focus::Panes;
            }
        }
        self.clamp();
    }

    fn open_selected_repository(&mut self) {
        self.opened_repository = self.current_repository().map(|r| r.id);
    }

    /// After switching projects: the selection starts on the first
    /// repository the rail lists, open, since that is the row on top.
    pub(super) fn start_on_first_rail_repository(&mut self) {
        let Some(first) = self.rail_repositories().first().copied() else {
            self.opened_repository = None;
            return;
        };
        self.sel_repository = first;
        self.sel_checkout = self.home_checkout_row();
        self.sel_pane = 0;
        self.open_selected_repository();
    }
}
