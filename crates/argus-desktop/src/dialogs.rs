//! The review and history dialogs as the window holds them: which is
//! open, the daemon's answers routed to the one that asked, the side and
//! layout the next review opens on, and what the page was last sent of
//! each. What each dialog holds and draws is `review`'s and `history`'s;
//! the history gives way to the review of a commit chosen in it.

use argus_protocol::{CheckoutId, ClientMsg, HeldTree, PaneId, ProjectInfo, ServerMsg};

use crate::history::{HistoryDialog, Then};
use crate::keymap::Action;
use crate::page::{Event, HistoryPage, ReviewClick, ReviewJumpGesture, ReviewPage};
use crate::review::ReviewDialog;
use crate::sent::Sent;
use crate::settings::ReviewLayout;

/// A file at a line, to open in the person's editor.
pub type EditorAt = (CheckoutId, String, Option<u32>);

/// A dialog put away while the Code stage is up.
enum Parked {
    Review(Box<ReviewDialog>),
    History(Box<HistoryDialog>),
}

pub struct Dialogs {
    review: Option<ReviewDialog>,
    /// A dialog put away while the Code stage reads what it was on, to be
    /// come back to as it was.
    parked: Option<Parked>,
    review_sent: Sent<ReviewPage>,
    history: Option<HistoryDialog>,
    history_sent: Sent<HistoryPage>,
    /// The side and layout a review opens on, as last chosen.
    layout: ReviewLayout,
    layout_changed: bool,
}

impl Dialogs {
    pub fn new(layout: ReviewLayout) -> Dialogs {
        Dialogs {
            review: None,
            parked: None,
            review_sent: Sent::default(),
            history: None,
            history_sent: Sent::default(),
            layout,
            layout_changed: false,
        }
    }

    /// Hands a daemon message to the dialog that asked for it, giving back
    /// any message neither takes. A commit's diff the history asked for is
    /// the history's, though it comes as a review.
    pub fn receive(&mut self, msg: ServerMsg, say: &mut Vec<Event>) -> Option<ServerMsg> {
        match msg {
            ServerMsg::Review(review) => {
                let taken = self.history.as_mut().is_some_and(|h| h.receive_diff(review.clone()));
                if let Some(dialog) = self.review.as_mut().filter(|_| !taken) {
                    dialog.receive(review);
                }
            }
            ServerMsg::ReviewFailed { request_id, checkout, message } => {
                if let Some(dialog) = &mut self.review {
                    dialog.failed(checkout, request_id, message);
                }
            }
            ServerMsg::ReviewContext(context) => {
                if let Some(dialog) = &mut self.review {
                    dialog.receive_context(context);
                }
            }
            ServerMsg::ReviewContextFailed { request_id, checkout, message } => {
                if self.review.as_mut().is_some_and(|d| d.context_failed(checkout, request_id)) {
                    say.push(Event::Notice { text: message });
                }
            }
            ServerMsg::ReviewCommentSaved { id, delivered } => {
                if let Some(dialog) = &mut self.review {
                    dialog.commented();
                }
                let text = if delivered { format!("comment #{id} sent") } else { format!("comment #{id} saved; the agent was not there to take it") };
                say.push(Event::Notice { text });
            }
            ServerMsg::Commits { request_id, commits, .. } => {
                if let Some(dialog) = &mut self.history {
                    dialog.receive_commits(request_id, commits);
                }
            }
            ServerMsg::CommitsFailed { request_id, message, .. } => {
                if let Some(dialog) = &mut self.history {
                    dialog.commits_failed(request_id, message);
                }
            }
            ServerMsg::CommitFiles { checkout, commit, files } => {
                if let Some(dialog) = &mut self.history {
                    dialog.receive_files(checkout, &commit, files);
                }
            }
            ServerMsg::CommitFilesFailed { checkout, commit, message } => {
                if let Some(dialog) = &mut self.history {
                    dialog.files_failed(checkout, &commit);
                }
                say.push(Event::Notice { text: message });
            }
            other => return Some(other),
        }
        None
    }

    /// Opens the review of a checkout's working changes.
    pub fn open_review(&mut self, checkout: CheckoutId, tree: &[ProjectInfo], id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) {
        let Some(info) = tree.checkout(checkout) else {
            return;
        };
        let (dialog, msgs) = ReviewDialog::working(info, self.layout, id);
        self.review = Some(dialog);
        out.extend(msgs);
    }

    pub fn open_history(&mut self, checkout: CheckoutId, tree: &[ProjectInfo], mut id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) {
        let Some(info) = tree.checkout(checkout) else {
            return;
        };
        let (dialog, msg) = HistoryDialog::open(info, id());
        self.history = Some(dialog);
        out.push(msg);
    }

    /// Opens the history with the cursor on `sha`, for a reference in an
    /// agent's view.
    pub fn open_history_at(&mut self, checkout: CheckoutId, sha: &str, tree: &[ProjectInfo], mut id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) {
        let Some(info) = tree.checkout(checkout) else {
            return;
        };
        let (dialog, msg) = HistoryDialog::open_at(info, id(), sha);
        self.history = Some(dialog);
        out.push(msg);
    }

    /// A key or button of the review dialog. `E` gives the review way to
    /// the file at its cursor, as the terminal client's does: what to open
    /// comes back, for the session to ask the daemon for.
    pub fn review_action(&mut self, action: Action, tree: &[ProjectInfo], say: &mut Vec<Event>) -> Option<EditorAt> {
        let dialog = self.review.as_mut()?;
        let info = tree.checkout(dialog.checkout());
        let refused = match action {
            Action::Side => dialog.switch_side().err(),
            Action::Comment => dialog.compose(info).err().map(str::to_owned),
            _ => None,
        };
        if let Some(text) = refused {
            say.push(Event::Notice { text });
        }
        let mut editor = None;
        match action {
            Action::Next => dialog.move_by(1),
            Action::Previous => dialog.move_by(-1),
            Action::First => dialog.ends(false),
            Action::Last => dialog.ends(true),
            Action::NextFile => dialog.jump_file(true),
            Action::PreviousFile => dialog.jump_file(false),
            Action::Pick => dialog.toggle_pick(),
            Action::Split => dialog.toggle_split(),
            Action::Filter => dialog.open_jump(),
            Action::OpenEditor => match dialog.editor_target() {
                Some(target) => editor = Some(target),
                None => say.push(Event::Notice { text: "no file to open".into() }),
            },
            _ => {}
        }
        self.remember_layout();
        if editor.is_some() || action == Action::CloseLayer {
            self.review = None;
        }
        editor
    }

    /// A click in the review: a file in the list, a line in the diff (with
    /// shift, extending the range), a side, or a recipient.
    pub fn review_click(&mut self, target: ReviewClick, say: &mut Vec<Event>) {
        let Some(dialog) = &mut self.review else {
            return;
        };
        match target {
            ReviewClick::File { file } => dialog.open_file(file),
            ReviewClick::Row { row, extend } => dialog.select_row(row, extend),
            ReviewClick::Side { staged } => {
                if let Err(text) = dialog.show_side(staged) {
                    say.push(Event::Notice { text });
                }
            }
            ReviewClick::Recipient { pane } => dialog.choose_recipient(PaneId(pane)),
            // A request to the daemon, which `review_unfold` sends.
            ReviewClick::Fold { .. } => {}
        }
        self.remember_layout();
    }

    /// Enter on a fold, or a click on one of its buttons (`row`): a step of
    /// its lines asked for, or with `all` the whole of it.
    pub fn review_unfold(&mut self, row: Option<usize>, all: bool, id: impl FnOnce() -> u64, out: &mut Vec<ClientMsg>) {
        if let Some(msg) = self.review.as_mut().and_then(|d| d.unfold(row, all, id())) {
            out.push(msg);
        }
    }

    /// A letter, a step, Enter or Escape in the review's jump field.
    pub fn review_jump(&mut self, gesture: ReviewJumpGesture) {
        let Some(dialog) = &mut self.review else {
            return;
        };
        match gesture {
            ReviewJumpGesture::Query { text } => dialog.type_jump(text),
            ReviewJumpGesture::Step { by } => dialog.step_jump(by),
            ReviewJumpGesture::Choose => dialog.choose_jump(),
            ReviewJumpGesture::Close => dialog.close_jump(),
        }
    }

    pub fn review_refresh(&mut self, id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) {
        if let Some(dialog) = &mut self.review {
            out.extend(dialog.refresh(id));
        }
    }

    /// Sends the comment, or with no `body` puts the box away.
    pub fn review_comment(&mut self, body: Option<String>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>, say: &mut Vec<Event>) {
        let Some(dialog) = &mut self.review else {
            return;
        };
        match body {
            None => dialog.cancel_comment(),
            Some(body) => match dialog.comment(tree.checkout(dialog.checkout()), &body) {
                Ok(msg) => out.push(msg),
                Err(text) => say.push(Event::Notice { text: text.into() }),
            },
        }
    }

    /// A key or click of the history dialog; `row` is a clicked row.
    pub fn history_action(
        &mut self,
        action: Action,
        row: Option<usize>,
        tree: &[ProjectInfo],
        mut id: impl FnMut() -> u64,
        out: &mut Vec<ClientMsg>,
    ) {
        let Some(dialog) = &mut self.history else {
            return;
        };
        let then = match (action, row) {
            (_, Some(row)) => dialog.select_row(row, &mut id),
            (Action::Next, _) => dialog.move_by(1, &mut id),
            (Action::Previous, _) => dialog.move_by(-1, &mut id),
            (Action::First, _) => dialog.ends(false, &mut id),
            (Action::Last, _) => dialog.ends(true, &mut id),
            (Action::Unfold, _) => dialog.unfold(),
            (Action::Fold, _) => {
                dialog.fold();
                Then::Nothing
            }
            (Action::Choose, _) => dialog.choose(),
            (Action::AsReview, _) => dialog.as_review(),
            (Action::CloseLayer, _) => {
                self.history = None;
                return;
            }
            _ => Then::Nothing,
        };
        match then {
            Then::Nothing => {}
            Then::Ask(msg) => out.push(*msg),
            // The history gives way to the review of the commit.
            Then::Review { commit, file } => {
                let checkout = dialog.checkout();
                self.history = None;
                if let Some(info) = tree.checkout(checkout) {
                    let (review, msg) = ReviewDialog::of_commit(info, commit, file, self.layout, id());
                    self.review = Some(review);
                    out.push(msg);
                }
            }
        }
    }

    /// What the history's filter field says now.
    pub fn filter_history(&mut self, filter: String, id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) {
        if let Some(dialog) = &mut self.history {
            if let Then::Ask(msg) = dialog.set_filter(filter, id) {
                out.push(*msg);
            }
        }
    }

    /// What the page has not heard of either dialog. `now` is seconds since
    /// the Unix epoch.
    pub fn take(&mut self, tree: &[ProjectInfo], now: u64) -> Vec<Event> {
        let review = self.review.as_ref().map(|d| d.page(tree.checkout(d.checkout())));
        let history = self.history.as_ref().map(|d| d.page(now));
        let review = self.review_sent.take(review).map(|page| Event::Review(Box::new(page)));
        let history = self.history_sent.take(history).map(|page| Event::History(Box::new(page)));
        review.into_iter().chain(history).collect()
    }

    /// The page reloaded and holds nothing: an open dialog is sent again.
    pub fn reload(&mut self) {
        self.review_sent.forget();
        self.history_sent.forget();
    }

    pub fn layout(&self) -> ReviewLayout {
        self.layout
    }

    /// Whether the layout changed since last asked, so it is saved.
    pub fn take_layout_changed(&mut self) -> bool {
        std::mem::take(&mut self.layout_changed)
    }

    /// What the review on screen has the person choosing, kept as the side
    /// and layout the next one opens on.
    fn remember_layout(&mut self) {
        let Some(chosen) = self.review.as_ref().map(ReviewDialog::chosen) else {
            return;
        };
        if chosen != self.layout {
            self.layout = chosen;
            self.layout_changed = true;
        }
    }

    /// `o` in a review: it is put away to be come back to, at the file and
    /// line its cursor is on, which come back to be read in the Code stage.
    pub fn park_review(&mut self) -> Option<(CheckoutId, String, u32, argus_protocol::CodeVersion)> {
        let target = self.review.as_ref()?.code_target()?;
        self.parked = self.review.take().map(|dialog| Parked::Review(Box::new(dialog)));
        Some(target)
    }

    /// `c` in the history: it is put away, and the commit under its cursor
    /// comes back, with the file when the cursor is on one.
    pub fn park_history(&mut self) -> Option<(CheckoutId, String, Option<String>)> {
        let target = self.history.as_ref()?.code_target()?;
        self.parked = self.history.take().map(|dialog| Parked::History(Box::new(dialog)));
        Some(target)
    }

    /// Escape in the Code stage: the dialog it was opened from, as it was.
    pub fn unpark(&mut self) -> bool {
        match self.parked.take() {
            Some(Parked::Review(dialog)) => self.review = Some(*dialog),
            Some(Parked::History(dialog)) => self.history = Some(*dialog),
            None => return false,
        }
        true
    }

    #[cfg(test)]
    pub fn history_open(&self) -> bool {
        self.history.is_some()
    }

    #[cfg(test)]
    pub fn review(&self) -> Option<&ReviewDialog> {
        self.review.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, project, repository};

    fn tree() -> Vec<ProjectInfo> {
        vec![project(1, "src", vec![repository(10, "argus", None, vec![checkout(100, "main", true, Vec::new())])])]
    }

    fn ids() -> impl FnMut() -> u64 {
        let mut last = 0;
        move || {
            last += 1;
            last
        }
    }

    fn reviewing(layout: ReviewLayout) -> Dialogs {
        let mut dialogs = Dialogs::new(layout);
        dialogs.open_review(CheckoutId(100), &tree(), ids(), &mut Vec::new());
        dialogs
    }

    #[test]
    fn the_side_and_layout_chosen_are_saved_and_the_next_review_opens_on_them() {
        let mut dialogs = reviewing(ReviewLayout::default());
        let mut say = Vec::new();
        dialogs.review_action(Action::Side, &tree(), &mut say);
        assert!(dialogs.take_layout_changed());
        assert_eq!(dialogs.layout(), ReviewLayout { staged: true, split: false });
        dialogs.review_click(ReviewClick::Side { staged: true }, &mut say);
        assert!(!dialogs.take_layout_changed(), "nothing changed");
        dialogs.review_action(Action::Split, &tree(), &mut say);
        dialogs.review_action(Action::CloseLayer, &tree(), &mut say);
        assert!(dialogs.review().is_none());
        assert_eq!(dialogs.layout(), ReviewLayout { staged: true, split: true });
        dialogs.open_review(CheckoutId(100), &tree(), ids(), &mut Vec::new());
        let page = dialogs.review().unwrap().page(None);
        assert_eq!((page.base, page.split), ("staged", true));
    }

    #[test]
    fn a_window_opens_reviews_on_the_layout_it_was_saved_with() {
        let dialogs = reviewing(ReviewLayout { staged: true, split: true });
        assert_eq!(dialogs.review().unwrap().page(None).base, "staged");
    }

    #[test]
    fn e_with_nothing_read_opens_nothing_and_says_so() {
        let mut dialogs = reviewing(ReviewLayout::default());
        let mut say = Vec::new();
        assert_eq!(dialogs.review_action(Action::OpenEditor, &tree(), &mut say), None);
        assert!(dialogs.review().is_some(), "the review stays");
        assert!(matches!(say.as_slice(), [Event::Notice { text }] if text == "no file to open"));
    }

    #[test]
    fn the_page_hears_a_dialog_while_it_is_open_and_its_closing_once() {
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        assert!(dialogs.take(&tree(), 0).is_empty(), "nothing open");
        dialogs.open_history(CheckoutId(100), &tree(), ids(), &mut Vec::new());
        assert!(matches!(dialogs.take(&tree(), 0).as_slice(), [Event::History(page)] if page.open));
        assert!(dialogs.take(&tree(), 0).is_empty(), "unchanged");
        dialogs.history_action(Action::CloseLayer, None, &tree(), ids(), &mut Vec::new());
        assert!(matches!(dialogs.take(&tree(), 0).as_slice(), [Event::History(page)] if !page.open));
        assert!(dialogs.take(&tree(), 0).is_empty());
    }

    #[test]
    fn a_checkout_not_in_the_tree_opens_nothing() {
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        let mut out = Vec::new();
        dialogs.open_review(CheckoutId(9), &tree(), ids(), &mut out);
        dialogs.open_history(CheckoutId(9), &tree(), ids(), &mut out);
        assert!(out.is_empty() && dialogs.take(&tree(), 0).is_empty());
    }

    #[test]
    fn a_message_neither_dialog_asked_for_is_given_back() {
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        let msg = ServerMsg::Error { message: "no".into() };
        assert!(matches!(dialogs.receive(msg, &mut Vec::new()), Some(ServerMsg::Error { .. })));
    }
}
