//! The link and reconcile dialogs as the window holds them: which of the
//! two is open, on which feature, closing when the board loses it, and
//! what the page was last sent of each. What each dialog holds and draws
//! is `link_dialog`'s and `reconcile_dialog`'s.
//!
//! Both work on the feature open on the Feature stage, so opening one puts
//! the other away. A refusal from the daemon is the link dialog's alone: a
//! reconcile's requests say how they went on the board.

use argus_protocol::{ClientMsg, DecisionBoard, Feature, PaneId, ProjectInfo};

use crate::feature_stage::Scope;
use crate::link_dialog::{self, LinkDialog};
use crate::page::{Event, LinkGesture, LinkPage, ReconcileGesture, ReconcilePage};
use crate::reconcile_dialog::{self, ReconcileDialog};
use crate::sent::Sent;

enum Open {
    Link(LinkDialog),
    Reconcile(ReconcileDialog),
}

impl Open {
    fn slug(&self) -> &str {
        match self {
            Open::Link(dialog) => dialog.slug(),
            Open::Reconcile(dialog) => dialog.slug(),
        }
    }
}

/// What a gesture leaves for the session to do beyond the daemon's
/// messages.
#[derive(Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    /// Open a URL in the browser.
    Open(String),
    /// Show the pane; the dialog has closed for it.
    GoTo(PaneId),
    Say(String),
}

#[derive(Default)]
pub struct TrackerDialogs {
    open: Option<Open>,
    link_sent: Sent<LinkPage>,
    reconcile_sent: Sent<ReconcilePage>,
}

impl TrackerDialogs {
    /// Opens the link dialog on the `opened` feature, with `map` on the
    /// tracker map's editor; without a feature, what to say instead.
    pub fn open_link(&mut self, opened: Option<&str>, board: Option<&DecisionBoard>, map: bool) -> Result<(), &'static str> {
        let slug = opened.ok_or("open a feature to link it to a ticket")?;
        let tracker_map = board.map(|b| b.tracker_map.clone()).unwrap_or_default();
        self.open = Some(Open::Link(LinkDialog::new(slug, map.then_some(&tracker_map))));
        Ok(())
    }

    pub fn open_reconcile(&mut self, opened: Option<&str>, board: Option<&DecisionBoard>) -> Result<(), &'static str> {
        let slug = opened.ok_or("open a feature to reconcile it with its tickets")?;
        let mut dialog = ReconcileDialog::new(slug);
        if let Some(feature) = board.and_then(|b| feature(b, slug)) {
            dialog.settle(feature);
        }
        self.open = Some(Open::Reconcile(dialog));
        Ok(())
    }

    /// A gesture of the link dialog, sent to the daemon through `out`. One
    /// that comes after its feature left the board closes it.
    pub fn link_gesture(
        &mut self,
        gesture: LinkGesture,
        board: Option<&DecisionBoard>,
        scope: &Scope,
        out: &mut Vec<ClientMsg>,
    ) -> Then {
        let (Some(Open::Link(dialog)), Some(board)) = (self.open.as_mut(), board) else {
            return Then::Nothing;
        };
        let Some(feature) = feature(board, dialog.slug()) else {
            self.open = None;
            return Then::Nothing;
        };
        match dialog.gesture(gesture, feature, &board.tracker_map, scope, out) {
            link_dialog::Then::Nothing => Then::Nothing,
            link_dialog::Then::Close => {
                self.open = None;
                Then::Nothing
            }
            link_dialog::Then::Open(url) => Then::Open(url),
            link_dialog::Then::Say(text) => Then::Say(text),
        }
    }

    pub fn reconcile_gesture(
        &mut self,
        gesture: ReconcileGesture,
        board: Option<&DecisionBoard>,
        tree: &[ProjectInfo],
        scope: &Scope,
        out: &mut Vec<ClientMsg>,
    ) -> Then {
        let (Some(Open::Reconcile(dialog)), Some(board)) = (self.open.as_mut(), board) else {
            return Then::Nothing;
        };
        let Some(feature) = feature(board, dialog.slug()) else {
            self.open = None;
            return Then::Nothing;
        };
        match dialog.gesture(gesture, feature, tree, scope, out) {
            reconcile_dialog::Then::Nothing => Then::Nothing,
            reconcile_dialog::Then::Close => {
                self.open = None;
                Then::Nothing
            }
            reconcile_dialog::Then::GoTo(pane) => {
                self.open = None;
                Then::GoTo(pane)
            }
            reconcile_dialog::Then::Say(text) => Then::Say(text),
        }
    }

    /// A pushed board: the open dialog follows its feature, and closes
    /// with it.
    pub fn settle(&mut self, board: &DecisionBoard) {
        let Some(open) = &mut self.open else {
            return;
        };
        match (feature(board, open.slug()), open) {
            (None, _) => self.open = None,
            (Some(f), Open::Link(dialog)) => dialog.settle(f),
            (Some(f), Open::Reconcile(dialog)) => dialog.settle(f),
        }
    }

    /// The daemon refused something.
    pub fn refused(&mut self) {
        if let Some(Open::Link(dialog)) = &mut self.open {
            dialog.refused();
        }
    }

    /// What the page has not heard of either dialog. A dialog whose feature
    /// the board does not hold, or a reconcile with no repository chosen,
    /// is drawn closed.
    pub fn take(&mut self, board: Option<&DecisionBoard>, tree: &[ProjectInfo], scope: Option<&Scope>, now: u64) -> Vec<Event> {
        let (mut link, mut reconcile) = (None, None);
        match &self.open {
            Some(Open::Link(dialog)) => {
                link = board.and_then(|b| Some(dialog.page(feature(b, dialog.slug())?, &b.tracker_map)));
            }
            Some(Open::Reconcile(dialog)) => {
                reconcile = board.zip(scope).and_then(|(b, scope)| {
                    Some(dialog.page(feature(b, dialog.slug())?, tree, scope.repository, now))
                });
            }
            None => {}
        }
        let link = self.link_sent.take(link).map(|page| Event::Link(Box::new(page)));
        let reconcile = self.reconcile_sent.take(reconcile).map(|page| Event::Reconcile(Box::new(page)));
        link.into_iter().chain(reconcile).collect()
    }

    /// The page reloaded and holds nothing: an open dialog is sent again.
    pub fn reload(&mut self) {
        self.link_sent.forget();
        self.reconcile_sent.forget();
    }
}

fn feature<'a>(board: &'a DecisionBoard, slug: &str) -> Option<&'a Feature> {
    board.features.iter().find(|f| f.slug == slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, board, feature_tree};
    use crate::keymap::Action;
    use argus_protocol::{CheckoutId, ProjectId, Reconcile, RepositoryId};

    fn scope() -> Scope {
        Scope {
            project: ProjectId(1),
            project_name: "src".into(),
            repository: RepositoryId(2),
            repository_name: "argus".into(),
            checkout: CheckoutId(3),
            key: "/src/main".into(),
        }
    }

    fn shown(dialogs: &mut TrackerDialogs, board: &DecisionBoard) -> Vec<&'static str> {
        dialogs
            .take(Some(board), &feature_tree(), Some(&scope()), 0)
            .iter()
            .map(|e| match e {
                Event::Link(page) if page.open => "link",
                Event::Link(_) => "link closed",
                Event::Reconcile(page) if page.open => "reconcile",
                Event::Reconcile(_) => "reconcile closed",
                _ => "other",
            })
            .collect()
    }

    #[test]
    fn neither_opens_without_a_feature_open() {
        let mut dialogs = TrackerDialogs::default();
        assert!(dialogs.open_link(None, Some(&board()), false).is_err());
        assert!(dialogs.open_reconcile(None, Some(&board())).is_err());
        assert!(shown(&mut dialogs, &board()).is_empty());
    }

    #[test]
    fn one_dialog_is_open_at_a_time() {
        let mut dialogs = TrackerDialogs::default();
        dialogs.open_link(Some("trim"), Some(&board()), false).unwrap();
        assert_eq!(shown(&mut dialogs, &board()), ["link"]);
        dialogs.open_reconcile(Some("trim"), Some(&board())).unwrap();
        assert_eq!(shown(&mut dialogs, &board()), ["link closed", "reconcile"]);
        assert!(shown(&mut dialogs, &board()).is_empty(), "each is heard once");
    }

    #[test]
    fn a_dialog_closes_with_its_feature() {
        let mut dialogs = TrackerDialogs::default();
        dialogs.open_link(Some("trim"), Some(&board()), false).unwrap();
        shown(&mut dialogs, &board());
        let mut gone = board();
        gone.features.retain(|f| f.slug != "trim");
        dialogs.settle(&gone);
        assert_eq!(shown(&mut dialogs, &gone), ["link closed"]);

        dialogs.open_reconcile(Some("trim"), Some(&board())).unwrap();
        let mut out = Vec::new();
        let then = dialogs.reconcile_gesture(ReconcileGesture::Select { line: 0 }, Some(&gone), &feature_tree(), &scope(), &mut out);
        assert_eq!(then, Then::Nothing);
        assert!(dialogs.open.is_none(), "a gesture after the feature went closes it too");
    }

    #[test]
    fn going_to_the_reconciling_agent_closes_the_dialog() {
        let mut held = board();
        let trim = held.features.iter_mut().find(|f| f.slug == "trim").unwrap();
        trim.reconcile = Some(Reconcile {
            asked: String::new(),
            pane: PaneId(1),
            by: "pane #1".into(),
            at: 5,
            proposed: false,
            lines: Vec::new(),
            noticed: Vec::new(),
            carrying: Vec::new(),
        });
        let mut dialogs = TrackerDialogs::default();
        dialogs.open_reconcile(Some("trim"), Some(&held)).unwrap();
        let gesture = ReconcileGesture::Action { action: Action::GoToPane };
        let then = dialogs.reconcile_gesture(gesture, Some(&held), &fixtures::feature_tree(), &scope(), &mut Vec::new());
        assert_eq!(then, Then::GoTo(PaneId(1)));
        assert!(dialogs.open.is_none());
    }

    #[test]
    fn a_refusal_lets_go_of_the_link_waiting_on_it() {
        let mut dialogs = TrackerDialogs::default();
        dialogs.open_link(Some("trim"), Some(&board()), false).unwrap();
        let link = LinkGesture::Link { url: "https://acme.atlassian.net/browse/ARG-142".into(), role: String::new() };
        let mut out = Vec::new();
        assert_eq!(dialogs.link_gesture(link, Some(&board()), &scope(), &mut out), Then::Nothing);
        assert_eq!(out.len(), 1, "the link is asked of the daemon");
        let pending = |dialogs: &mut TrackerDialogs| match &dialogs.take(Some(&board()), &[], None, 0)[..] {
            [Event::Link(page)] => page.pending,
            other => panic!("{other:?}"),
        };
        assert!(pending(&mut dialogs));
        dialogs.refused();
        assert!(!pending(&mut dialogs));
    }

    #[test]
    fn a_reloaded_page_hears_the_open_dialog_again() {
        let mut dialogs = TrackerDialogs::default();
        dialogs.open_reconcile(Some("trim"), Some(&board())).unwrap();
        shown(&mut dialogs, &board());
        dialogs.reload();
        assert_eq!(shown(&mut dialogs, &board()), ["reconcile"]);
    }
}
