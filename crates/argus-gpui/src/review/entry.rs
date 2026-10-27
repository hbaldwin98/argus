//! How the window enters and leaves the Git stage, and which `Call`s each way
//! sends.
//!
//! Core has no Git stage: it holds a review and a history, each open or not,
//! as it held the two dialogs. The window shows the Git stage while either is
//! open, so the stage is the session's to say and a press only asks: Git sends
//! `OpenReview` for the checkout the window is on, a tab sends the open of its
//! own and the close of the other (core never holds both for long), and any
//! other way out sends the close of whatever is open, in the same breath as
//! the call that goes somewhere else. Nothing is drawn of a review before its
//! event.
//!
//! The one thing held here is what only the window knows: that Git was pressed
//! with no checkout to open, so the stage is up saying to pick one, and which
//! tab was pressed last, for the moment neither is open.

use argus_desktop_core::connection::page::{Call, Rail};
use argus_desktop_core::keys::keymap::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// The review of a checkout's working changes, or of one commit.
    #[default]
    Changes,
    /// The checkout's commits.
    History,
}

/// What the session says is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Open {
    pub review: bool,
    pub history: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Entry {
    tab: Tab,
    /// Git was pressed and there was no checkout to open it on.
    asked: bool,
    /// The tab a switch has sent the open of and the session has not yet
    /// opened. Core publishes the other tab's close and this tab's open each
    /// as its own batch, so between them nothing is open and the stage would
    /// drop, handing a key in the gap to the pane under it.
    pending: Option<Tab>,
    /// The checkout the stage is on, for the tab that has to open its own
    /// view of it: the press that opened the stage, or the review or history
    /// that last opened, by whatever route.
    checkout: Option<u64>,
}

impl Entry {
    /// Whether the Git stage is up: the session has a review or a history
    /// open, Git was pressed with nothing to open, or a tab switch is between
    /// its close and its open.
    pub fn up(&self, open: Open) -> bool {
        open.review || open.history || self.asked || self.pending.is_some()
    }

    /// Which tab the stage shows: the one that is open, else the last pressed.
    pub fn tab(&self, open: Open) -> Tab {
        match (open.review, open.history) {
            (true, false) => Tab::Changes,
            (false, true) => Tab::History,
            _ => self.tab,
        }
    }

    /// Git pressed, with `checkout` the one the window is on. Nothing is sent
    /// while the stage already shows something open.
    pub fn enter(&mut self, open: Open, checkout: Option<u64>) -> Vec<Call> {
        if open.review || open.history {
            return Vec::new();
        }
        self.tab = Tab::Changes;
        self.checkout = checkout.or(self.checkout);
        self.begin(Tab::Changes, checkout)
    }

    /// A review was opened, by whatever asked for it: its checkout is the
    /// one the stage is on, and whatever had only been asked for is answered.
    pub fn opened(&mut self, checkout: u64) {
        self.checkout = Some(checkout);
        self.asked = false;
        self.answered(Tab::Changes);
    }

    /// A history was opened, by whatever asked for it: its checkout is the
    /// one the stage is on, as a review's is.
    pub fn history_opened(&mut self, checkout: u64) {
        self.checkout = Some(checkout);
        self.asked = false;
        self.answered(Tab::History);
    }

    /// The session opened `tab`: a switch to it is no longer pending.
    fn answered(&mut self, tab: Tab) {
        if self.pending == Some(tab) {
            self.pending = None;
        }
    }

    /// A tab pressed: the other one's close, then this one's open.
    pub fn show(&mut self, tab: Tab, open: Open, checkout: Option<u64>) -> Vec<Call> {
        self.tab = tab;
        let mut calls = Vec::new();
        match tab {
            Tab::Changes if open.history => calls.push(close_history()),
            Tab::History if open.review => calls.push(close_review()),
            _ => {}
        }
        let already = match tab {
            Tab::Changes => open.review,
            Tab::History => open.history,
        };
        if !already {
            let opening = self.begin(tab, self.checkout.or(checkout));
            if !opening.is_empty() {
                self.pending = Some(tab);
            }
            calls.extend(opening);
        }
        calls
    }

    /// The close of whatever is open, for a press that goes elsewhere or
    /// shuts the stage; the stage is down at once when only Git had been
    /// pressed.
    pub fn leave(&mut self, open: Open) -> Vec<Call> {
        self.asked = false;
        self.pending = None;
        let mut calls = Vec::new();
        if open.review {
            calls.push(close_review());
        }
        if open.history {
            calls.push(close_history());
        }
        calls
    }

    fn begin(&mut self, tab: Tab, checkout: Option<u64>) -> Vec<Call> {
        let Some(checkout) = checkout else {
            self.asked = true;
            return Vec::new();
        };
        self.asked = false;
        vec![match tab {
            Tab::Changes => Call::OpenReview { checkout },
            Tab::History => Call::OpenHistory { checkout },
        }]
    }
}

fn close_review() -> Call {
    Call::ReviewAction { action: Action::CloseLayer }
}

fn close_history() -> Call {
    Call::HistoryAction { action: Action::CloseLayer, row: None }
}

/// The checkout the window is on: the one holding the pane on screen, else
/// the one the person picked in the rail. A folded checkout lists no panes,
/// so the pane is looked for by its names too.
pub fn checkout_here(rail: &Rail, picked: Option<u64>) -> Option<u64> {
    let repositories = rail.project.iter().flat_map(|project| &project.repositories);
    let found = rail.shown.as_ref().and_then(|shown| {
        repositories
            .clone()
            .flat_map(|repository| &repository.checkouts)
            .find(|checkout| checkout.panes.iter().any(|pane| pane.id == shown.pane))
            .or_else(|| {
                repositories
                    .clone()
                    .filter(|repository| repository.name == shown.repository)
                    .flat_map(|repository| &repository.checkouts)
                    .find(|checkout| checkout.name == shown.checkout)
            })
            .map(|checkout| checkout.id)
    });
    found.or(picked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::{Checkout, Pane, Project, Repository, Shown};

    fn said(calls: Vec<Call>) -> Vec<String> {
        calls.iter().map(|call| format!("{call:?}")).collect()
    }

    const NOTHING: Open = Open { review: false, history: false };
    const REVIEW: Open = Open { review: true, history: false };
    const HISTORY: Open = Open { review: false, history: true };

    #[test]
    fn git_opens_the_review_of_the_checkout_the_window_is_on() {
        let mut entry = Entry::default();
        assert!(!entry.up(NOTHING));
        assert_eq!(said(entry.enter(NOTHING, Some(7))), ["OpenReview { checkout: 7 }"]);
        assert!(!entry.up(NOTHING), "nothing is up until the session says the review is open");
        assert!(entry.up(REVIEW));
    }

    #[test]
    fn git_with_no_checkout_is_up_saying_to_pick_one_and_sends_nothing() {
        let mut entry = Entry::default();
        assert!(entry.enter(NOTHING, None).is_empty());
        assert!(entry.up(NOTHING), "the stage is the window's here, not the session's");
        // The person picks a checkout and presses Git again: now it opens.
        assert_eq!(said(entry.enter(NOTHING, Some(9))), ["OpenReview { checkout: 9 }"]);
        assert!(!entry.up(NOTHING), "asked is answered once there was something to open");
    }

    #[test]
    fn git_pressed_while_a_review_or_history_is_open_sends_nothing() {
        let mut entry = Entry::default();
        assert!(entry.enter(REVIEW, Some(7)).is_empty());
        assert!(entry.enter(HISTORY, Some(7)).is_empty());
    }

    #[test]
    fn the_stage_shows_the_tab_that_is_open_and_otherwise_the_one_pressed() {
        let mut entry = Entry::default();
        assert_eq!(entry.tab(NOTHING), Tab::Changes);
        assert_eq!(entry.tab(HISTORY), Tab::History);
        entry.show(Tab::History, NOTHING, None);
        assert_eq!(entry.tab(NOTHING), Tab::History);
        assert_eq!(entry.tab(REVIEW), Tab::Changes, "what the session has open wins");
        assert_eq!(entry.tab(Open { review: true, history: true }), Tab::History, "both: the last pressed");
    }

    #[test]
    fn a_tab_closes_the_other_and_opens_its_own() {
        let mut entry = Entry::default();
        assert_eq!(
            said(entry.show(Tab::History, REVIEW, Some(7))),
            ["ReviewAction { action: CloseLayer }", "OpenHistory { checkout: 7 }"]
        );
        assert_eq!(
            said(entry.show(Tab::Changes, HISTORY, Some(7))),
            ["HistoryAction { action: CloseLayer, row: None }", "OpenReview { checkout: 7 }"]
        );
        assert!(entry.show(Tab::Changes, REVIEW, Some(7)).is_empty(), "the tab already open sends nothing");
    }

    #[test]
    fn a_tab_opens_on_the_checkout_the_stage_is_on_not_the_one_the_window_is_on_now() {
        let mut entry = Entry::default();
        entry.enter(NOTHING, Some(7));
        assert_eq!(
            said(entry.show(Tab::History, REVIEW, Some(9))),
            ["ReviewAction { action: CloseLayer }", "OpenHistory { checkout: 7 }"]
        );
        let mut other = Entry::default();
        other.opened(5);
        assert_eq!(said(other.show(Tab::History, HISTORY, None)), [] as [&str; 0], "already open");
        assert_eq!(said(other.show(Tab::Changes, HISTORY, None)).last().map(String::as_str), Some("OpenReview { checkout: 5 }"));
    }

    #[test]
    fn leaving_closes_whatever_the_session_has_open_and_nothing_else() {
        let mut entry = Entry::default();
        assert!(entry.leave(NOTHING).is_empty());
        assert_eq!(said(entry.leave(REVIEW)), ["ReviewAction { action: CloseLayer }"]);
        assert_eq!(said(entry.leave(HISTORY)), ["HistoryAction { action: CloseLayer, row: None }"]);
        assert_eq!(
            said(entry.leave(Open { review: true, history: true })),
            ["ReviewAction { action: CloseLayer }", "HistoryAction { action: CloseLayer, row: None }"]
        );
    }

    #[test]
    fn a_review_opened_by_another_route_answers_a_stage_that_was_only_asked_for() {
        let mut entry = Entry::default();
        entry.enter(NOTHING, None);
        assert!(entry.up(NOTHING));
        entry.opened(5);
        assert!(entry.up(REVIEW));
        assert!(!entry.up(NOTHING), "the review closed, and nothing is asked for any more");
    }

    #[test]
    fn a_history_opened_by_another_route_answers_a_stage_that_was_only_asked_for() {
        let mut entry = Entry::default();
        entry.enter(NOTHING, None);
        entry.history_opened(5);
        assert!(!entry.up(NOTHING));
    }

    #[test]
    fn changes_opens_on_the_checkout_of_a_history_opened_from_the_rail() {
        let mut entry = Entry::default();
        entry.opened(5);
        entry.leave(REVIEW);
        // `h` on another checkout in the rail opens its history, not through Git.
        entry.history_opened(9);
        assert_eq!(
            said(entry.show(Tab::Changes, HISTORY, Some(9))),
            ["HistoryAction { action: CloseLayer, row: None }", "OpenReview { checkout: 9 }"],
            "not the checkout an earlier review was on"
        );
    }

    #[test]
    fn a_tab_switch_keeps_the_stage_up_between_the_close_and_the_open() {
        let mut entry = Entry::default();
        entry.enter(NOTHING, Some(7));
        entry.show(Tab::History, REVIEW, Some(7));
        assert!(entry.up(NOTHING), "the close was heard, the open not yet");
        entry.history_opened(7);
        assert!(entry.up(HISTORY));
        assert!(!entry.up(NOTHING), "the open answered the switch");
    }

    #[test]
    fn only_the_tab_pressed_answers_its_switch_and_leaving_ends_it() {
        let mut entry = Entry::default();
        entry.show(Tab::History, REVIEW, Some(7));
        entry.opened(7);
        assert!(entry.up(NOTHING), "a review opening is not the history that was asked for");
        entry.leave(NOTHING);
        assert!(!entry.up(NOTHING));
    }

    #[test]
    fn a_tab_that_sends_no_open_is_not_pending() {
        let mut entry = Entry::default();
        entry.show(Tab::Changes, REVIEW, Some(7));
        assert!(!entry.up(NOTHING), "the tab was already open");
    }

    #[test]
    fn leaving_puts_down_a_stage_that_was_only_asked_for() {
        let mut entry = Entry::default();
        entry.enter(NOTHING, None);
        assert!(entry.up(NOTHING));
        assert!(entry.leave(NOTHING).is_empty());
        assert!(!entry.up(NOTHING));
    }

    fn pane(id: u64) -> Pane {
        Pane { id, agent: false, title: "zsh".into(), status: "idle", note: None, on: false, views: 0 }
    }

    fn checkout(id: u64, name: &str, panes: Vec<Pane>) -> Checkout {
        Checkout {
            id,
            name: name.into(),
            branch: name.into(),
            kind: "main",
            git: None,
            clean: true,
            shared: None,
            open: !panes.is_empty(),
            count: panes.len(),
            status: None,
            panes,
        }
    }

    fn rail(shown: Option<(u64, &str, &str)>) -> Rail {
        Rail {
            project: Some(Project {
                id: 1,
                name: "orion".into(),
                needs_you: 0,
                repositories: vec![Repository {
                    id: 10,
                    name: "argus".into(),
                    quiet: false,
                    divider: false,
                    open: true,
                    panes: 1,
                    status: None,
                    used: None,
                    checkouts: vec![checkout(100, "main", vec![pane(1)]), checkout(110, "side", Vec::new())],
                }],
                running: Vec::new(),
            }),
            shown: shown.map(|(pane, repository, checkout)| Shown {
                pane,
                repository: repository.into(),
                checkout: checkout.into(),
                branch: "main".into(),
                agent: false,
                browser: false,
                title: "zsh".into(),
                status: "idle",
                resumed: None,
            }),
            ..Rail::default()
        }
    }

    #[test]
    fn the_window_is_on_the_checkout_of_the_pane_shown_else_the_one_picked() {
        assert_eq!(checkout_here(&rail(Some((1, "argus", "main"))), Some(110)), Some(100), "the pane wins over the pick");
        assert_eq!(checkout_here(&rail(None), Some(110)), Some(110));
        assert_eq!(checkout_here(&rail(None), None), None);
    }

    #[test]
    fn a_pane_of_a_folded_checkout_is_found_by_its_names() {
        assert_eq!(checkout_here(&rail(Some((5, "argus", "side"))), None), Some(110));
        assert_eq!(checkout_here(&rail(Some((5, "other", "side"))), None), None, "another repository's name is not this one's");
    }
}
