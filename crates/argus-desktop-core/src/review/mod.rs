//! The review and history dialogs as the window holds them: which is
//! open or parked, the daemon's answers and refusals routed to the one
//! that asked whether open or parked, the side and layout the next
//! review opens on, and what the page was last sent of each. What each
//! dialog holds and draws is `dialog`'s and `history`'s; the history
//! gives way to the review of a commit chosen in it.

pub mod comment_box;
pub mod dialog;
pub mod drafts;
pub mod file_jump;
pub mod history;
pub mod rows;
pub mod staging;

use argus_protocol::review::session::Answered;
use argus_protocol::{CheckoutId, ClientMsg, HeldTree, PaneId, ProjectInfo, RequestRefusal, ServerMsg};

use crate::review::history::{HistoryDialog, Then};
use crate::keys::keymap::Action;
use crate::connection::page::{Event, HistoryPage, ReviewClick, ReviewJumpGesture, ReviewPage};
use crate::review::dialog::{Abilities, ReviewDialog};
use crate::review::drafts::DraftGesture;
use crate::review::staging::{Landed, StageGesture};
use crate::connection::sent::Sent;
use crate::settings::remembered::ReviewLayout;

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
    /// What the daemon on the line lets a review do, for the next to open.
    abilities: Abilities,
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
            abilities: Abilities::default(),
        }
    }

    /// The daemon just greeted: what a review opened from now on may ask it.
    pub fn set_abilities(&mut self, abilities: Abilities) {
        self.abilities = abilities;
    }

    /// Hands a daemon message to the dialog that asked for it, giving back
    /// any message neither takes. A commit's diff the history asked for is
    /// the history's, though it comes as a review.
    pub fn receive(&mut self, msg: ServerMsg, say: &mut Vec<Event>) -> Option<ServerMsg> {
        match msg {
            ServerMsg::Review(review) => {
                let mut unasked = Some(review);
                for history in self.histories() {
                    unasked = unasked.and_then(|review| history.receive_diff(review));
                }
                if let (Some(review), Some(dialog)) = (unasked, self.review_mut()) {
                    dialog.receive(review);
                }
            }
            ServerMsg::ReviewContext(context) => {
                if let Some(dialog) = self.review_mut() {
                    dialog.receive_context(context);
                }
            }
            // Every review hears the save and takes it only when it names
            // the comment its box sent, open or parked.
            ServerMsg::ReviewCommentSaved { id, delivered, request_id } => {
                for dialog in self.reviews() {
                    dialog.commented(request_id);
                }
                let text = if delivered { format!("comment #{id} sent") } else { format!("comment #{id} saved; the agent was not there to take it") };
                say.push(Event::Notice { text });
            }
            // Each history keeps only what it asked for: a list by its id, a
            // commit's files while it waits on them.
            ServerMsg::Commits { request_id, commits, .. } => {
                for history in self.histories() {
                    history.receive_commits(request_id, commits.clone());
                }
            }
            ServerMsg::CommitFiles { checkout, commit, files } => {
                for history in self.histories() {
                    history.receive_files(checkout, &commit, files.clone());
                }
            }
            // A write the Changes pane made: what it moved shows when the
            // diff is read again, so only a commit is said.
            ServerMsg::Written { request_id } => {
                let landed = self.reviews().fold(None, |landed, dialog| landed.or(dialog.written(request_id)));
                if landed == Some(Landed::Committed) {
                    say.push(Event::Notice { text: "committed".into() });
                }
            }
            ServerMsg::Drafts { request_id, checkout, drafts } => {
                for dialog in self.reviews() {
                    dialog.drafted(request_id, checkout, drafts.clone());
                }
            }
            ServerMsg::ReviewSubmitted { comments, delivered, .. } => {
                let said = if comments == 1 { "1 comment".to_owned() } else { format!("{comments} comments") };
                let text = if delivered { format!("review sent: {said}") } else { format!("review saved: {said}; the agent was not there to take it") };
                say.push(Event::Notice { text });
            }
            ServerMsg::CommitFilesFailed { checkout, commit, message } => {
                for history in self.histories() {
                    history.files_failed(checkout, &commit);
                }
                say.push(Event::Notice { text: message });
            }
            other => return Some(other),
        }
        None
    }

    /// A refusal of a request one of the dialogs made, taken by the one
    /// that made it, open or parked: a history's list, a commit's diff it
    /// asked for and a review side say it in place, a fold as a notice.
    /// False when none asked.
    pub fn refused(&mut self, refusal: &RequestRefusal, say: &mut Vec<Event>) -> bool {
        let (id, reason) = (refusal.request_id, &refusal.reason);
        for history in self.histories() {
            if history.commits_failed(id, reason.clone()) || history.diff_failed(id, reason.clone()) {
                return true;
            }
        }
        let Some(review) = self.review_mut() else {
            return false;
        };
        if review.context_failed(id) || review.change_refused(id) {
            say.push(Event::Notice { text: reason.clone() });
            return true;
        }
        review.failed(id, reason.clone()) != Answered::Stale
    }

    /// A refusal of a comment a review sent, open or parked: its box lets
    /// go and keeps its words. Every review hears it, since an older
    /// daemon's names none. Said by the caller, as a refused change is.
    pub fn comment_refused(&mut self, refusal: &RequestRefusal) -> bool {
        self.reviews().fold(false, |taken, dialog| dialog.comment_refused(refusal) | taken)
    }

    /// A new connection: no review's comment will be answered on it.
    pub fn forget_sent_comments(&mut self) {
        for dialog in self.reviews() {
            dialog.forget_sent_comment();
        }
    }

    /// Every history answers can be for: the open one, then the one parked
    /// while the Code stage reads, which still waits on what it asked.
    fn histories(&mut self) -> impl Iterator<Item = &mut HistoryDialog> + '_ {
        let parked = match &mut self.parked {
            Some(Parked::History(dialog)) => Some(&mut **dialog),
            _ => None,
        };
        self.history.as_mut().into_iter().chain(parked)
    }

    /// Every review there is: the open one, then one parked while the Code
    /// stage reads, which may have a comment of its own out.
    fn reviews(&mut self) -> impl Iterator<Item = &mut ReviewDialog> + '_ {
        let parked = match &mut self.parked {
            Some(Parked::Review(dialog)) => Some(&mut **dialog),
            _ => None,
        };
        self.review.as_mut().into_iter().chain(parked)
    }

    /// The review answers go to: the open one, else the parked one. A
    /// parked review always holds a drawn view, so one open beside it
    /// costs it only a refresh.
    fn review_mut(&mut self) -> Option<&mut ReviewDialog> {
        match (&mut self.review, &mut self.parked) {
            (Some(dialog), _) => Some(dialog),
            (None, Some(Parked::Review(dialog))) => Some(&mut **dialog),
            _ => None,
        }
    }

    /// Opens the review of a checkout's working changes.
    pub fn open_review(&mut self, checkout: CheckoutId, tree: &[ProjectInfo], id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) {
        let Some(info) = tree.checkout(checkout) else {
            return;
        };
        let (dialog, msgs) = ReviewDialog::working(info, self.layout, self.abilities, id);
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
        if let Some(dialog) = &mut self.review {
            dialog.jump(gesture);
        }
    }

    pub fn review_refresh(&mut self, id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) {
        if let Some(dialog) = &mut self.review {
            out.extend(dialog.refresh(id));
        }
    }

    /// A new tree: every review, open or parked, hears whether its
    /// checkout's working changes moved, and the one answers go to reads
    /// them again if they did.
    pub fn follow(&mut self, tree: &[ProjectInfo], id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) {
        for dialog in self.reviews() {
            dialog.follow(tree.checkout(dialog.checkout()));
        }
        self.catch_up(id, out);
    }

    /// The review answers go to reads its sides again if its checkout's
    /// working changes moved while it could not: a read was out, or its
    /// comment box was open.
    pub fn catch_up(&mut self, mut id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) {
        if let Some(dialog) = self.review_mut() {
            out.extend(dialog.catch_up(&mut id));
        }
        // A commit opened asks for its first file's diff once its files land.
        if let Some(Then::Ask(msg)) = self.history.as_mut().map(|h| h.catch_up(&mut id)) {
            out.push(*msg);
        }
    }

    /// Sends the comment, named by `id`, or with no `body` puts the box away.
    pub fn review_comment(
        &mut self,
        body: Option<String>,
        tree: &[ProjectInfo],
        mut id: impl FnMut() -> u64,
        out: &mut Vec<ClientMsg>,
        say: &mut Vec<Event>,
    ) {
        let Some(dialog) = &mut self.review else {
            return;
        };
        match body {
            None => {
                dialog.cancel_comment();
                self.catch_up(id, out);
            }
            Some(body) => match dialog.comment(tree.checkout(dialog.checkout()), &body, id()) {
                Ok(msg) => out.push(msg),
                Err(text) => say.push(Event::Notice { text: text.into() }),
            },
        }
    }

    /// A file of a Changes list opened on its side.
    pub fn open_change(&mut self, staged: bool, path: &str, say: &mut Vec<Event>) {
        if let Some(Err(text)) = self.review.as_mut().map(|d| d.open_change(staged, path)) {
            say.push(Event::Notice { text });
        }
        self.remember_layout();
    }

    /// A box ticked, files moved or a hunk taken, sent to the daemon.
    pub fn stage(&mut self, gesture: StageGesture, id: impl FnOnce() -> u64, out: &mut Vec<ClientMsg>, say: &mut Vec<Event>) {
        if let Some(sent) = self.review.as_mut().map(|d| d.stage(gesture, id())) {
            match sent {
                Ok(msg) => out.push(msg),
                Err(text) => say.push(Event::Notice { text }),
            }
        }
    }

    pub fn commit(&mut self, message: &str, id: impl FnOnce() -> u64, out: &mut Vec<ClientMsg>, say: &mut Vec<Event>) {
        if let Some(sent) = self.review.as_mut().map(|d| d.commit(message, id())) {
            match sent {
                Ok(msg) => out.push(msg),
                Err(text) => say.push(Event::Notice { text }),
            }
        }
    }

    /// The comment box's words, as a draft of the review.
    pub fn draft_comment(&mut self, body: &str, id: impl FnOnce() -> u64, out: &mut Vec<ClientMsg>, say: &mut Vec<Event>) {
        if let Some(sent) = self.review.as_mut().map(|d| d.draft_comment(body, id())) {
            match sent {
                Ok(msg) => out.push(msg),
                Err(text) => say.push(Event::Notice { text: text.into() }),
            }
        }
    }

    pub fn drafts(&mut self, gesture: DraftGesture, tree: &[ProjectInfo], id: impl FnOnce() -> u64, out: &mut Vec<ClientMsg>, say: &mut Vec<Event>) {
        let Some(dialog) = self.review.as_mut() else {
            return;
        };
        match dialog.draft_gesture(gesture, tree.checkout(dialog.checkout()), id()) {
            Ok(msg) => out.push(msg),
            Err(text) => say.push(Event::Notice { text }),
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
            // A click on a commit toggles it: the chevron says which.
            (Action::Fold | Action::Unfold, Some(row)) => {
                dialog.select_row(row, &mut id);
                if action == Action::Fold {
                    dialog.fold();
                    Then::Nothing
                } else {
                    dialog.unfold(&mut id)
                }
            }
            (_, Some(row)) => dialog.select_row(row, &mut id),
            (Action::Next, _) => dialog.move_by(1, &mut id),
            (Action::Previous, _) => dialog.move_by(-1, &mut id),
            (Action::First, _) => dialog.ends(false, &mut id),
            (Action::Last, _) => dialog.ends(true, &mut id),
            (Action::Unfold, _) => dialog.unfold(&mut id),
            (Action::Fold, _) => {
                dialog.fold();
                Then::Nothing
            }
            (Action::FoldAll, _) => {
                dialog.fold_all();
                Then::Nothing
            }
            (Action::Choose, _) => dialog.choose(&mut id),
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
    use crate::fixtures::{checkout, commit, commit_file, diff_of, pane, project, repository};
    use argus_protocol::{PaneKind, PaneStatus, ReviewBase};

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

    /// The tree with one live agent in checkout 100.
    fn agent_tree() -> Vec<ProjectInfo> {
        let agent = pane(7, PaneKind::Agent, "claude", PaneStatus::Working);
        vec![project(1, "src", vec![repository(10, "argus", None, vec![checkout(100, "main", true, vec![agent])])])]
    }

    /// A review with an agent to comment to, its unstaged side read, a line
    /// picked and the comment box open on it.
    fn composing(tree: &[ProjectInfo]) -> Dialogs {
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        compose(&mut dialogs, tree);
        dialogs
    }

    fn compose(dialogs: &mut Dialogs, tree: &[ProjectInfo]) {
        dialogs.open_review(CheckoutId(100), tree, ids(), &mut Vec::new());
        let mut read = diff_of(1, "src/a.rs");
        read.base = ReviewBase::Unstaged;
        assert!(dialogs.receive(ServerMsg::Review(read), &mut Vec::new()).is_none());
        let mut say = Vec::new();
        dialogs.review_action(Action::Pick, tree, &mut say);
        dialogs.review_action(Action::Comment, tree, &mut say);
        assert!(say.is_empty(), "{say:?}");
    }

    #[test]
    fn a_review_parked_with_its_comment_out_takes_its_save() {
        let tree = agent_tree();
        let mut dialogs = composing(&tree);
        let (mut out, mut say) = (Vec::new(), Vec::new());
        dialogs.review_comment(Some("tidy".into()), &tree, || 50, &mut out, &mut say);
        assert!(matches!(out.as_slice(), [ClientMsg::ReviewComment { request_id: 50, .. }]), "{say:?}");
        assert!(dialogs.park_review().is_some());
        dialogs.receive(ServerMsg::ReviewCommentSaved { id: 1, delivered: true, request_id: 50 }, &mut say);
        assert!(dialogs.unpark());
        let page = dialogs.review().unwrap().page(tree.checkout(CheckoutId(100)));
        assert!(page.comment.is_none() && page.pick.is_none());
    }

    /// The case two boxes out at once got wrong while a save named none:
    /// the first save shut both and dropped both picks.
    #[test]
    fn a_save_reaches_only_the_review_whose_comment_it_names() {
        let tree = agent_tree();
        let mut dialogs = composing(&tree);
        let (mut out, mut say) = (Vec::new(), Vec::new());
        dialogs.review_comment(Some("parked".into()), &tree, || 50, &mut out, &mut say);
        assert!(dialogs.park_review().is_some());
        compose(&mut dialogs, &tree);
        dialogs.review_comment(Some("open".into()), &tree, || 51, &mut out, &mut say);
        assert_eq!(out.len(), 2, "{say:?}");

        dialogs.receive(ServerMsg::ReviewCommentSaved { id: 2, delivered: true, request_id: 51 }, &mut say);
        let page = dialogs.review().unwrap().page(tree.checkout(CheckoutId(100)));
        assert!(page.comment.is_none() && page.pick.is_none(), "the open review took its save");
        assert!(dialogs.unpark());
        let page = dialogs.review().unwrap().page(tree.checkout(CheckoutId(100)));
        assert!(page.comment.is_some() && page.pick.is_some(), "the parked review's comment is still out");
        dialogs.receive(ServerMsg::ReviewCommentSaved { id: 3, delivered: true, request_id: 50 }, &mut say);
        let page = dialogs.review().unwrap().page(tree.checkout(CheckoutId(100)));
        assert!(page.comment.is_none() && page.pick.is_none());
    }

    /// A review opened from the rail while another is parked with its
    /// comment out: the save still shuts the parked one's box, and the open
    /// one, which sent nothing, keeps its own.
    #[test]
    fn a_review_parked_with_its_comment_out_takes_its_save_while_another_is_open() {
        let tree = agent_tree();
        let mut dialogs = composing(&tree);
        let (mut out, mut say) = (Vec::new(), Vec::new());
        dialogs.review_comment(Some("tidy".into()), &tree, || 50, &mut out, &mut say);
        assert!(matches!(out.as_slice(), [ClientMsg::ReviewComment { .. }]), "{say:?}");
        assert!(dialogs.park_review().is_some());
        compose(&mut dialogs, &tree);

        dialogs.receive(ServerMsg::ReviewCommentSaved { id: 1, delivered: true, request_id: 50 }, &mut say);
        let page = dialogs.review().unwrap().page(tree.checkout(CheckoutId(100)));
        assert!(page.comment.is_some() && page.pick.is_some(), "the open review keeps its box");
        assert!(dialogs.unpark());
        let page = dialogs.review().unwrap().page(tree.checkout(CheckoutId(100)));
        assert!(page.comment.is_none() && page.pick.is_none(), "the parked review took its save");
    }

    #[test]
    fn a_refusal_reaches_the_review_whose_comment_it_names_open_or_parked() {
        let tree = agent_tree();
        let mut dialogs = composing(&tree);
        let (mut out, mut say) = (Vec::new(), Vec::new());
        dialogs.review_comment(Some("parked".into()), &tree, || 50, &mut out, &mut say);
        assert!(dialogs.park_review().is_some());
        compose(&mut dialogs, &tree);
        dialogs.review_comment(Some("open".into()), &tree, || 51, &mut out, &mut say);
        assert_eq!(out.len(), 2, "{say:?}");

        let refused = |id| RequestRefusal { request_id: id, reason: "no".into(), read: false };
        assert!(dialogs.comment_refused(&refused(51)));
        dialogs.review_comment(Some("open".into()), &tree, || 52, &mut out, &mut say);
        assert_eq!(out.len(), 3, "the open review sends again: {say:?}");
        assert!(dialogs.unpark());
        dialogs.review_comment(Some("parked".into()), &tree, || 53, &mut out, &mut say);
        assert_eq!(out.len(), 3);
        assert!(matches!(say.as_slice(), [Event::Notice { text }] if text == "the last comment is still being saved"), "{say:?}");
        assert!(dialogs.comment_refused(&refused(50)));
        dialogs.review_comment(Some("parked".into()), &tree, || 54, &mut out, &mut say);
        assert_eq!(out.len(), 4, "the parked review, back, sends again");
    }

    #[test]
    fn a_code_comment_saved_while_a_review_is_parked_leaves_its_picks() {
        let tree = agent_tree();
        let mut dialogs = composing(&tree);
        let (mut out, mut say) = (Vec::new(), Vec::new());
        dialogs.review_comment(Some("tidy".into()), &tree, || 50, &mut out, &mut say);
        assert!(matches!(out.as_slice(), [ClientMsg::ReviewComment { .. }]), "{say:?}");
        assert!(dialogs.park_review().is_some());
        // The Code stage's comment, named by its own count.
        dialogs.receive(ServerMsg::ReviewCommentSaved { id: 1, delivered: true, request_id: 9 }, &mut Vec::new());
        assert!(dialogs.unpark());
        let page = dialogs.review().unwrap().page(tree.checkout(CheckoutId(100)));
        assert!(page.comment.is_some() && page.pick.is_some());
    }

    fn refusal(request_id: u64, reason: &str) -> RequestRefusal {
        RequestRefusal { request_id, reason: reason.into(), read: true }
    }

    /// The daemon refuses a review of a checkout that is gone under its
    /// ids; the review says why where the person is looking.
    #[test]
    fn a_review_of_a_checkout_that_is_gone_says_why_in_the_review() {
        let mut dialogs = reviewing(ReviewLayout::default());
        for request_id in [1, 2] {
            assert!(dialogs.refused(&refusal(request_id, "no such checkout"), &mut Vec::new()));
        }
        assert_eq!(dialogs.review().unwrap().page(None).empty.as_deref(), Some("no such checkout"));
        assert!(!dialogs.refused(&refusal(99, "another's"), &mut Vec::new()), "not a request either made");
    }

    /// A history over one commit of one file, with the cursor on that file:
    /// the id its diff was asked under. `id` is shared with any review open
    /// beside it, so the two never ask under one id.
    fn history_on_a_file(dialogs: &mut Dialogs, mut id: impl FnMut() -> u64) -> u64 {
        let mut out = Vec::new();
        dialogs.open_history(CheckoutId(100), &tree(), &mut id, &mut out);
        let Some(ClientMsg::ListCommits { request_id, .. }) = out.pop() else {
            panic!("the commits are asked for");
        };
        let say = &mut Vec::new();
        dialogs.receive(ServerMsg::Commits { request_id, checkout: CheckoutId(100), commits: vec![commit("aaaa1", "s")] }, say);
        dialogs.history_action(Action::Unfold, None, &tree(), &mut id, &mut out);
        let files = vec![commit_file("DESKTOP.md")];
        dialogs.receive(ServerMsg::CommitFiles { checkout: CheckoutId(100), commit: "aaaa1".into(), files }, say);
        dialogs.history_action(Action::Next, None, &tree(), &mut id, &mut out);
        let Some(ClientMsg::Review { request_id, .. }) = out.pop() else {
            panic!("the file's diff is asked for");
        };
        request_id
    }

    fn history_page(dialogs: &mut Dialogs) -> HistoryPage {
        dialogs.history_sent.forget();
        dialogs
            .take(&tree(), 0)
            .into_iter()
            .find_map(|e| match e {
                Event::History(page) => Some(*page),
                _ => None,
            })
            .expect("a history is open")
    }

    /// The chevron says what a click does: an open commit folds, a folded
    /// one opens, and `H` folds them all.
    #[test]
    fn a_click_on_a_commit_row_folds_it_when_open_and_opens_it_when_folded() {
        let mut id = ids();
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        history_on_a_file(&mut dialogs, &mut id);
        let mut out = Vec::new();
        let kinds = |d: &mut Dialogs| history_page(d).rows.iter().map(|r| r.kind).collect::<Vec<_>>();
        assert_eq!(kinds(&mut dialogs), ["commit", "file"]);
        dialogs.history_action(Action::Fold, Some(0), &tree(), &mut id, &mut out);
        assert_eq!(kinds(&mut dialogs), ["commit"], "an open commit folds on a click");
        dialogs.history_action(Action::Unfold, Some(0), &tree(), &mut id, &mut out);
        let page = history_page(&mut dialogs);
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.path.as_deref(), Some("DESKTOP.md"), "and opens on its first file");
        dialogs.history_action(Action::FoldAll, None, &tree(), &mut id, &mut out);
        assert_eq!(kinds(&mut dialogs), ["commit"]);
    }

    #[test]
    fn a_failed_commit_diff_reaches_the_history_that_asked() {
        let mut id = ids();
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        dialogs.open_review(CheckoutId(100), &tree(), &mut id, &mut Vec::new());
        let asked = history_on_a_file(&mut dialogs, &mut id);

        let mut say = Vec::new();
        assert!(dialogs.refused(&refusal(asked, "bad object"), &mut say));
        assert!(say.is_empty(), "said in place, not as a notice");
        let page = history_page(&mut dialogs);
        assert_eq!((page.diff_empty.as_deref(), page.diff_loading), (Some("bad object"), false));
        assert!(dialogs.review().unwrap().page(None).empty.is_none(), "and the review beside it was handed nothing");
    }

    /// The Code stage reads while a history's diff is still out: the
    /// answer must reach it, or it waits on it once it comes back.
    #[test]
    fn a_parked_history_still_hears_the_diff_it_asked_for() {
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        let asked = history_on_a_file(&mut dialogs, ids());
        assert!(dialogs.park_history().is_some());

        assert!(dialogs.receive(ServerMsg::Review(diff_of(asked, "DESKTOP.md")), &mut Vec::new()).is_none());
        assert!(dialogs.unpark());
        let page = history_page(&mut dialogs);
        assert_eq!((page.diff.len(), page.diff_loading), (2, false));
    }

    /// A history opened from the rail while another is parked: the parked
    /// one still takes its own answers, and the open one is not handed them.
    #[test]
    fn a_history_parked_under_an_open_one_still_hears_its_answers() {
        let mut id = ids();
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        let (mut out, say) = (Vec::new(), &mut Vec::new());
        dialogs.open_history(CheckoutId(100), &tree(), &mut id, &mut out);
        let Some(ClientMsg::ListCommits { request_id, .. }) = out.pop() else {
            panic!("the commits are asked for");
        };
        dialogs.receive(ServerMsg::Commits { request_id, checkout: CheckoutId(100), commits: vec![commit("aaaa1", "s"), commit("bbbb2", "s")] }, say);
        dialogs.history_action(Action::Unfold, None, &tree(), &mut id, &mut out);
        dialogs.receive(ServerMsg::CommitFiles { checkout: CheckoutId(100), commit: "aaaa1".into(), files: vec![commit_file("a.md")] }, say);
        dialogs.history_action(Action::Next, Some(2), &tree(), &mut id, &mut out);
        dialogs.history_action(Action::Unfold, None, &tree(), &mut id, &mut out);
        dialogs.receive(ServerMsg::CommitFiles { checkout: CheckoutId(100), commit: "bbbb2".into(), files: vec![commit_file("b.md")] }, say);
        out.clear();
        dialogs.history_action(Action::Next, Some(1), &tree(), &mut id, &mut out);
        dialogs.history_action(Action::Next, Some(3), &tree(), &mut id, &mut out);
        let [ClientMsg::Review { request_id: a, .. }, ClientMsg::Review { request_id: b, .. }] = out.as_slice() else {
            panic!("both files' diffs are asked for: {out:?}");
        };
        let (a, b) = (*a, *b);
        assert!(dialogs.park_history().is_some());
        dialogs.open_history(CheckoutId(100), &tree(), &mut id, &mut Vec::new());
        let open = history_page(&mut dialogs);

        assert!(dialogs.receive(ServerMsg::Review(diff_of(a, "a.md")), say).is_none());
        assert!(dialogs.refused(&refusal(b, "bad object"), say));
        assert_eq!(history_page(&mut dialogs), open, "the open history was handed nothing");

        assert!(dialogs.unpark());
        assert_eq!(history_page(&mut dialogs).diff_empty.as_deref(), Some("bad object"), "the cursor is on the refused one");
        let mut asked = Vec::new();
        dialogs.history_action(Action::Next, Some(1), &tree(), &mut id, &mut asked);
        assert!(asked.is_empty(), "the diff came while parked");
        let page = history_page(&mut dialogs);
        assert_eq!((page.path.as_deref(), page.diff.len(), page.diff_loading), (Some("a.md"), 2, false));
    }

    /// A history opened from the rail asks for a commit's files that the
    /// parked one already holds unfolded: its refusal folds only its own.
    #[test]
    fn a_refusal_of_files_another_history_asked_for_leaves_the_parked_one_unfolded() {
        let mut id = ids();
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        history_on_a_file(&mut dialogs, &mut id);
        assert!(dialogs.park_history().is_some());
        let mut out = Vec::new();
        dialogs.open_history(CheckoutId(100), &tree(), &mut id, &mut out);
        let Some(ClientMsg::ListCommits { request_id, .. }) = out.pop() else {
            panic!("the commits are asked for");
        };
        let say = &mut Vec::new();
        dialogs.receive(ServerMsg::Commits { request_id, checkout: CheckoutId(100), commits: vec![commit("aaaa1", "s")] }, say);
        dialogs.history_action(Action::Unfold, None, &tree(), &mut id, &mut out);
        assert!(matches!(out.pop(), Some(ClientMsg::ListCommitFiles { .. })));

        let failed = ServerMsg::CommitFilesFailed { checkout: CheckoutId(100), commit: "aaaa1".into(), message: "bad object".into() };
        assert!(dialogs.receive(failed, say).is_none());
        assert!(dialogs.unpark());
        assert_eq!(history_page(&mut dialogs).path.as_deref(), Some("DESKTOP.md"), "still unfolded, the cursor on its file");
    }

    /// The tree with one live agent in checkout 100, whose working changes
    /// the daemon fingerprints as `changes`.
    fn changes_at(changes: u64) -> Vec<ProjectInfo> {
        let mut tree = agent_tree();
        tree[0].repositories[0].checkouts[0].git.as_mut().unwrap().changes = changes;
        tree
    }

    /// A review of checkout 100 opened at `changes_at(0)`, both its sides
    /// read, with a line of the unstaged side picked.
    fn read_review(id: &mut impl FnMut() -> u64) -> Dialogs {
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        let mut out = Vec::new();
        dialogs.open_review(CheckoutId(100), &changes_at(0), &mut *id, &mut out);
        for (msg, base) in out.iter().zip([ReviewBase::Unstaged, ReviewBase::Staged]) {
            let ClientMsg::Review { request_id, .. } = msg else { panic!("{msg:?}") };
            let mut read = diff_of(*request_id, "src/a.rs");
            read.base = base;
            assert!(dialogs.receive(ServerMsg::Review(read), &mut Vec::new()).is_none());
        }
        dialogs.review_action(Action::Pick, &changes_at(0), &mut Vec::new());
        dialogs
    }

    #[test]
    fn a_review_reads_again_when_the_tree_says_its_checkout_changed() {
        let mut id = ids();
        let mut dialogs = read_review(&mut id);
        let mut out = Vec::new();
        dialogs.follow(&changes_at(0), &mut id, &mut out);
        assert!(out.is_empty(), "a tree with nothing moved asks nothing");
        dialogs.follow(&changes_at(9), &mut id, &mut out);
        assert!(matches!(out.as_slice(), [ClientMsg::Review { .. }, ClientMsg::Review { .. }]), "{out:?}");
        assert!(dialogs.review().unwrap().page(None).pick.is_some(), "the pick stays while the new read is out");
    }

    #[test]
    fn an_open_comment_box_holds_the_review_until_it_closes() {
        let mut id = ids();
        let mut dialogs = read_review(&mut id);
        let (mut out, mut say) = (Vec::new(), Vec::new());
        dialogs.review_action(Action::Comment, &changes_at(0), &mut say);
        assert!(say.is_empty(), "{say:?}");
        dialogs.follow(&changes_at(9), &mut id, &mut out);
        assert!(out.is_empty(), "the lines the comment quotes stay put: {out:?}");
        dialogs.review_comment(None, &changes_at(9), &mut id, &mut out, &mut say);
        assert_eq!(out.len(), 2, "put away, the box lets the review catch up: {out:?}");
    }

    #[test]
    fn a_message_neither_dialog_asked_for_is_given_back() {
        let mut dialogs = Dialogs::new(ReviewLayout::default());
        let msg = ServerMsg::Error { message: "no".into() };
        assert!(matches!(dialogs.receive(msg, &mut Vec::new()), Some(ServerMsg::Error { .. })));
    }
}
