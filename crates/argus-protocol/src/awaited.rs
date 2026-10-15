//! Requests a client waits on the daemon to make something for — a pane,
//! a checkout, a project, a repository — and which of the things made the
//! tree now holds, so the client acts on each once it can be seen.
//!
//! What was made is matched by the id the daemon answers with
//! ([`Created`]), never by where a new row turns up in the next tree: the
//! tree is broadcast for everything, and the first to arrive need not be
//! the one carrying the row. Guessing by position once put the keys in a
//! pane that was already running. The answer and the tree that carries the
//! row can arrive in either order, so a thing that has been made waits
//! here until the tree has it. Each client says what to do with what it
//! made (`Then`); both clients hold this one module, so they settle alike.

use crate::held_tree::HeldTree;
use crate::message::Created;
use crate::tree::ProjectInfo;

/// The most requests held unanswered. A daemon older than the answer never
/// sends one, and a request nobody will answer should not be carried for
/// the rest of the session.
pub const MAX_ASKED: usize = 32;

#[derive(Debug)]
pub struct Awaited<Then> {
    last: u64,
    /// Asked, and not yet answered.
    asked: Vec<(u64, Then)>,
    /// Answered, and not yet in the tree.
    made: Vec<(Created, Then)>,
}

impl<Then> Default for Awaited<Then> {
    fn default() -> Self {
        Awaited { last: 0, asked: Vec::new(), made: Vec::new() }
    }
}

impl<Then> Awaited<Then> {
    /// Names a request, remembering what to do with what it makes.
    pub fn ask(&mut self, then: Then) -> u64 {
        let id = self.name();
        if self.asked.len() == MAX_ASKED {
            self.asked.remove(0);
        }
        self.asked.push((id, then));
        id
    }

    /// Names a request answered by a message of its own rather than
    /// `Created`, from the same count, so no two of a client's requests
    /// share a name. Never zero: zero is what asks the daemon for no answer.
    pub fn name(&mut self) -> u64 {
        self.last = self.last.wrapping_add(1).max(1);
        self.last
    }

    /// The daemon's answer. A refusal forgets the request — its reason
    /// arrives as an error — and a thing made waits for the tree. An answer
    /// to a request this client did not make, or no longer holds, is
    /// another client's or too late, and changes nothing.
    pub fn answer(&mut self, request_id: u64, created: Option<Created>) {
        let Some(at) = self.asked.iter().position(|(id, _)| *id == request_id) else {
            return;
        };
        let (_, then) = self.asked.remove(at);
        if let Some(created) = created {
            self.made.push((created, then));
        }
    }

    /// A new connection: the daemon on it answers nothing asked of the
    /// last. What was made already waits on, for the next tree to carry.
    pub fn forget_asked(&mut self) {
        self.asked.clear();
    }

    /// Everything made that `tree` holds, to act on now; the rest waits for
    /// a later tree.
    pub fn settle(&mut self, tree: &[ProjectInfo]) -> Vec<(Created, Then)> {
        self.settle_by(|created| holds(tree, created))
    }

    /// Everything made that `held` says the client can now see. For what
    /// travels beside the tree rather than in it, such as a console.
    pub fn settle_by(&mut self, held: impl Fn(Created) -> bool) -> Vec<(Created, Then)> {
        let (seen, waiting) = std::mem::take(&mut self.made).into_iter().partition(|(created, _)| held(*created));
        self.made = waiting;
        seen
    }
}

/// Whether the tree carries what the daemon said it made. Every pane
/// counts, editors included: one made is to be found whether or not its
/// checkout lists it.
pub fn holds(tree: &[ProjectInfo], created: Created) -> bool {
    let repositories = || tree.iter().flat_map(|p| &p.repositories);
    match created {
        Created::Pane(id) => tree.panes().any(|p| p.id == id),
        Created::Checkout(id) => repositories().flat_map(|r| &r.checkouts).any(|c| c.id == id),
        Created::Project(id) => tree.iter().any(|p| p.id == id),
        Created::Repository(id) => repositories().any(|r| r.id == id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{CheckoutId, PaneId, ProjectId};
    use crate::tree::{AgentTelemetry, CheckoutInfo, PaneInfo, PaneKind, PaneStatus};

    /// A project holding one checkout, 100, with the panes `panes`.
    fn tree(panes: &[u64]) -> Vec<ProjectInfo> {
        let panes = panes
            .iter()
            .map(|&id| PaneInfo {
                id: PaneId(id),
                kind: PaneKind::Shell,
                title: format!("pane {id}"),
                status: PaneStatus::Idle,
                note: None,
                template: None,
                children: Vec::new(),
                background: false,
                telemetry: AgentTelemetry::default(),
                since: None,
                live: false,
            })
            .collect();
        let checkout = CheckoutInfo { id: CheckoutId(100), name: "main".into(), path: "/src/main".into(), panes, git: None, primary: true };
        let repository = crate::tree::RepositoryInfo {
            id: crate::ids::RepositoryId(10),
            name: "argus".into(),
            checkouts: vec![checkout],
            branches: Vec::new(),
            default_branch: None,
            remote_branches: Vec::new(),
            unmerged: Default::default(),
            last_used: None,
        };
        vec![ProjectInfo { id: ProjectId(1), name: "src".into(), root: None, repositories: vec![repository] }]
    }

    #[test]
    fn a_thing_made_before_the_tree_holds_it_waits_for_the_tree() {
        let mut awaited = Awaited::default();
        let id = awaited.ask("focus");
        awaited.answer(id, Some(Created::Pane(PaneId(7))));
        assert!(awaited.settle(&tree(&[1])).is_empty(), "no tree holds it yet");
        assert_eq!(awaited.settle(&tree(&[1, 7])), [(Created::Pane(PaneId(7)), "focus")]);
        assert!(awaited.settle(&tree(&[1, 7])).is_empty(), "acted on once");
    }

    #[test]
    fn a_tree_that_holds_it_before_the_answer_settles_on_the_answer() {
        let mut awaited = Awaited::default();
        let id = awaited.ask("focus");
        let grown = tree(&[1, 7]);
        assert!(awaited.settle(&grown).is_empty(), "a new row is not ours until answered");
        awaited.answer(id, Some(Created::Pane(PaneId(7))));
        assert_eq!(awaited.settle(&grown), [(Created::Pane(PaneId(7)), "focus")]);
    }

    #[test]
    fn a_refusal_forgets_the_request() {
        let mut awaited = Awaited::default();
        let id = awaited.ask("focus");
        awaited.answer(id, None);
        awaited.answer(id, Some(Created::Pane(PaneId(1))));
        assert!(awaited.settle(&tree(&[1])).is_empty(), "a later answer to it is not ours");
    }

    #[test]
    fn an_answer_to_a_request_not_made_here_is_ignored() {
        let mut awaited: Awaited<&str> = Awaited::default();
        awaited.answer(5, Some(Created::Pane(PaneId(1))));
        assert!(awaited.settle(&tree(&[1])).is_empty());
    }

    #[test]
    fn past_the_most_held_the_oldest_request_is_dropped() {
        let mut awaited = Awaited::default();
        let first = awaited.ask(0);
        for n in 1..=MAX_ASKED {
            awaited.ask(n);
        }
        awaited.answer(first, Some(Created::Checkout(CheckoutId(100))));
        assert!(awaited.settle(&tree(&[])).is_empty(), "the oldest was let go");
        let last = awaited.name() - 1;
        awaited.answer(last, Some(Created::Checkout(CheckoutId(100))));
        assert_eq!(awaited.settle(&tree(&[])), [(Created::Checkout(CheckoutId(100)), MAX_ASKED)]);
    }

    #[test]
    fn a_new_connection_forgets_what_was_asked_but_not_what_was_made() {
        let mut awaited = Awaited::default();
        let made = awaited.ask("made");
        let asked = awaited.ask("asked");
        awaited.answer(made, Some(Created::Pane(PaneId(7))));
        awaited.forget_asked();
        awaited.answer(asked, Some(Created::Pane(PaneId(1))));
        assert_eq!(awaited.settle(&tree(&[1, 7])), [(Created::Pane(PaneId(7)), "made")]);
    }

    #[test]
    fn requests_are_named_from_one_count_and_never_zero() {
        let mut awaited: Awaited<()> = Awaited::default();
        let asked = awaited.ask(());
        let named = awaited.name();
        assert!(asked != 0 && named != asked);
    }
}
