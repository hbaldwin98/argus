//! Requests waiting on something the daemon is making — a pane, a
//! checkout, a project, a repository — and what to do with it once it
//! exists.
//!
//! What was made is matched by the id the daemon answers with
//! ([`ServerMsg::Created`]), never by where a new row turns up in the next
//! tree: the tree is broadcast for everything, and the first to arrive need
//! not be the one carrying the row. Guessing by position once put the keys
//! in a pane that was already running. The answer and the tree that carries
//! the row can arrive in either order, so a thing that has been made waits
//! here until the tree has it.

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

/// The most requests held unanswered. A daemon older than the answer never
/// sends one, and a request nobody will answer should not be carried for
/// the rest of the session.
const MAX_ASKED: usize = 32;

#[derive(Debug, Default)]
pub(super) struct Awaited {
    last: u64,
    /// Asked, and not yet answered.
    asked: Vec<(u64, Then)>,
    /// Answered, and not yet in the tree.
    made: Vec<(Created, Then)>,
}

impl Awaited {
    /// Names a request, remembering what to do with what it makes. Never
    /// zero: zero is what asks the daemon for no answer.
    pub(super) fn ask(&mut self, then: Then) -> u64 {
        self.last = self.last.wrapping_add(1).max(1);
        if self.asked.len() == MAX_ASKED {
            self.asked.remove(0);
        }
        self.asked.push((self.last, then));
        self.last
    }

    /// The daemon's answer. A refusal forgets the request — its reason
    /// arrives as an error — and a thing made waits for the tree.
    pub(super) fn answer(&mut self, request_id: u64, created: Option<Created>) {
        let Some(at) = self.asked.iter().position(|(id, _)| *id == request_id) else {
            return;
        };
        let (_, then) = self.asked.remove(at);
        if let Some(created) = created {
            self.made.push((created, then));
        }
    }
}

impl App {
    /// Acts on everything made that the tree now holds; the rest waits for
    /// a later tree.
    pub(super) fn settle_awaited(&mut self) {
        for (created, then) in std::mem::take(&mut self.awaited.made) {
            if !self.act_on_made(created, &then) {
                self.awaited.made.push((created, then));
            }
        }
    }

    /// Whether the thing was in the tree to be acted on.
    fn act_on_made(&mut self, created: Created, then: &Then) -> bool {
        match (created, then) {
            (Created::Pane(id), Then::FocusPane { floating }) => self.focus_made_pane(id, *floating),
            (Created::Checkout(id), Then::SelectCheckout) => self.select_checkout_id(id),
            (Created::Checkout(id), Then::SpawnAgent { template }) => {
                if !self.select_checkout_id(id) {
                    return false;
                }
                let request_id = self.awaited.ask(Then::FocusPane { floating: false });
                let _ = self.out.send(ClientMsg::SpawnAgent {
                    checkout: id,
                    template: template.clone(),
                    request_id,
                });
                true
            }
            (Created::Checkout(id), Then::SpawnShell) => {
                if !self.select_checkout_id(id) {
                    return false;
                }
                let request_id = self.awaited.ask(Then::FocusPane { floating: false });
                let _ = self.out.send(ClientMsg::SpawnShell {
                    checkout: id,
                    request_id,
                });
                true
            }
            (Created::Project(id), Then::SelectProject) => {
                let Some(index) = self.tree.iter().position(|p| p.id == id) else {
                    return false;
                };
                self.leave_pane();
                self.sel_project = index;
                self.clamp();
                true
            }
            (Created::Repository(id), Then::SelectRepository) => {
                let found = self.tree.iter().enumerate().find_map(|(project, p)| {
                    p.repositories
                        .iter()
                        .position(|r| r.id == id)
                        .map(|repository| (project, repository))
                });
                let Some((project, repository)) = found else {
                    return false;
                };
                self.leave_pane();
                self.sel_project = project;
                self.sel_repository = repository;
                self.sel_checkout = self.home_checkout_row();
                self.clamp();
                true
            }
            // A pairing no request makes; nothing to wait for.
            _ => true,
        }
    }

    /// Takes the keys to a new pane on the stage that shows it — or floats
    /// it, when asked to or when it is not a pane the workspace can hold
    /// (an editor is not a listed pane).
    fn focus_made_pane(&mut self, id: PaneId, floating: bool) -> bool {
        let Some(title) = panes_in(&self.tree)
            .find(|p| p.id == id)
            .map(|p| p.title.clone())
        else {
            return false;
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
        true
    }

    /// Moving the selection to something new leaves the pane being typed
    /// into: the keys would otherwise go to a pane the stage no longer
    /// shows.
    fn leave_pane(&mut self) {
        if self.focus == Focus::PaneContent {
            self.focus = Focus::Panes;
        }
        self.pane_fullscreen = false;
        self.leader_pending = false;
    }

    /// Selects a checkout by its id, wherever it is. False when the tree
    /// does not hold it yet.
    fn select_checkout_id(&mut self, id: CheckoutId) -> bool {
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
            return false;
        };
        self.leave_pane();
        self.sel_project = project;
        self.sel_repository = repository;
        self.sel_checkout = self.checkout_row_of(checkout).unwrap_or(0);
        self.sel_pane = 0;
        self.clamp();
        true
    }
}
