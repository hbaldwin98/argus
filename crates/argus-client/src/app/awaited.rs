//! What the terminal client does with what the daemon made for it — a
//! pane, a checkout, a project, a repository — once the tree holds it.
//! Which requests are waited on, and when what one made is in the tree,
//! is `argus_protocol::awaited`'s, which the desktop window holds too.

use argus_protocol::Created;

use super::*;

/// What to do with a thing once it is in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Then {
    /// Take the keys to the new pane, or float it as a window.
    FocusPane { floating: bool },
    SelectCheckout,
    /// Start this agent in the new checkout, then take the keys to it.
    SpawnAgent { template: String },
    /// Start a shell in the new checkout, then take the keys to it.
    SpawnShell,
    SelectProject,
    SelectRepository,
}

pub(super) type Awaited = argus_protocol::awaited::Awaited<Then>;

impl App {
    /// Acts on everything made that the tree, or the list of consoles beside
    /// it, now holds; the rest waits for a later one.
    pub(super) fn settle_awaited(&mut self) {
        let (tree, consoles) = (&self.tree, &self.consoles);
        let seen = |created| {
            argus_protocol::awaited::holds(tree, created)
                || matches!(created, Created::Pane(id) if consoles.iter().any(|c| c.id == id))
        };
        for (created, then) in self.awaited.settle_by(seen) {
            self.act_on_made(created, &then);
        }
    }

    /// The tree holds what was made, so every lookup here finds it.
    fn act_on_made(&mut self, created: Created, then: &Then) {
        match (created, then) {
            (Created::Pane(id), Then::FocusPane { floating }) => self.focus_made_pane(id, *floating),
            (Created::Checkout(id), Then::SelectCheckout) => self.select_checkout_id(id),
            (Created::Checkout(id), Then::SpawnAgent { template }) => {
                self.select_checkout_id(id);
                let request_id = self.awaited.ask(Then::FocusPane { floating: false });
                let _ = self.out.send(ClientMsg::SpawnAgent {
                    checkout: id,
                    template: template.clone(),
                    prompt: None,
                    request_id,
                });
            }
            (Created::Checkout(id), Then::SpawnShell) => {
                self.select_checkout_id(id);
                let request_id = self.awaited.ask(Then::FocusPane { floating: false });
                let _ = self.out.send(ClientMsg::SpawnShell {
                    checkout: id,
                    request_id,
                });
            }
            (Created::Project(id), Then::SelectProject) => {
                let Some(index) = self.tree.iter().position(|p| p.id == id) else {
                    return;
                };
                self.leave_pane();
                self.sel_project = index;
                self.clamp();
            }
            (Created::Repository(id), Then::SelectRepository) => {
                let found = self.tree.iter().enumerate().find_map(|(project, p)| {
                    p.repositories
                        .iter()
                        .position(|r| r.id == id)
                        .map(|repository| (project, repository))
                });
                let Some((project, repository)) = found else {
                    return;
                };
                self.leave_pane();
                self.sel_project = project;
                self.sel_repository = repository;
                self.sel_checkout = self.home_checkout_row();
                self.clamp();
            }
            // A pairing no request makes.
            _ => {}
        }
    }

    /// Takes the keys to a new pane on the stage that shows it — or floats
    /// it, when asked to or when it is not a pane the workspace can hold
    /// (an editor is not a listed pane).
    fn focus_made_pane(&mut self, id: PaneId, floating: bool) {
        if let Some(console) = self.consoles.iter().position(|c| c.id == id) {
            self.open_view(View::Workspace);
            self.pick_console(console);
            self.focus = Focus::PaneContent;
            return;
        }
        let Some(title) = self.tree.panes()
            .find(|p| p.id == id)
            .map(|p| p.title.clone())
        else {
            return;
        };
        let listed = self
            .flat_pane_locations()
            .into_iter()
            .find(|location| self.pane_at(*location).is_some_and(|p| p.id == id));
        match listed {
            Some(location) if !floating => {
                self.select_pane_location(location);
                self.sync_subscription();
                self.open_view(View::Workspace);
                self.focus = Focus::PaneContent;
            }
            // Deliberately leaves the selection alone: the stage keeps
            // showing whatever you were watching, and closing the window
            // puts you back there rather than on the editor.
            _ => self.open_overlay_pane(id, title, true),
        }
    }

    /// Moving the selection to something new leaves the pane being typed
    /// into: the keys would otherwise go to a pane the stage no longer
    /// shows.
    fn leave_pane(&mut self) {
        self.forget_console();
        if self.focus == Focus::PaneContent {
            self.focus = Focus::Panes;
        }
        self.pane_fullscreen = false;
        self.leader_pending = false;
    }

    /// Selects a checkout by its id, wherever it is.
    fn select_checkout_id(&mut self, id: CheckoutId) {
        let found = self.tree.iter().enumerate().find_map(|(project, p)| {
            p.repositories
                .iter()
                .enumerate()
                .find_map(|(repository, r)| {
                    r.checkouts
                        .iter()
                        .position(|c| c.id == id)
                        .map(|checkout| (project, repository, checkout))
                })
        });
        let Some((project, repository, checkout)) = found else {
            return;
        };
        self.leave_pane();
        self.sel_project = project;
        self.sel_repository = repository;
        self.sel_checkout = self.checkout_row_of(checkout).unwrap_or(0);
        self.sel_pane = 0;
        self.clamp();
    }
}
