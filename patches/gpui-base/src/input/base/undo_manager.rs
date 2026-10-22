use std::ops::Range;

use super::auto_close::AutoClosedPairs;
use crate::input::change::Change;

use super::cursor::CursorSelection;

const MAX_UNDO_TRANSACTIONS: usize = 1000;
const MAX_CHANGES_PER_TRANSACTION: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditIntent {
    Typing,
    Backspace,
    DeleteForward,
    Atomic,
}

#[derive(Debug)]
struct UndoTransaction {
    intent: EditIntent,
    changes: Vec<Change>,
    /// How many changes the most recently appended batch contributed. A batch
    /// is one logical edit with one change per cursor. Only a following batch
    /// of the same length can coalesce into this transaction.
    last_batch_len: usize,
    /// How many changes were recorded into this transaction. A run of
    /// single-cursor keystrokes is merged into one change, so this, not
    /// `changes.len()`, decides when the transaction is full.
    recorded_changes: usize,
    /// The cursors as they stood before this transaction, restored on undo.
    selections_before: Option<Vec<CursorSelection>>,
    /// The cursors as they stood after it, restored on redo.
    selections_after: Option<Vec<CursorSelection>>,
    auto_closed_pairs_before: Option<AutoClosedPairs>,
    auto_closed_pairs_after: Option<AutoClosedPairs>,
}

/// A batch of changes being collected between `begin_transaction` and the
/// matching `commit_transaction`.
#[derive(Debug)]
struct PendingTransaction {
    intent: EditIntent,
    changes: Vec<Change>,
    selections_before: Option<Vec<CursorSelection>>,
    selections_after: Option<Vec<CursorSelection>>,
    auto_closed_pairs_before: Option<AutoClosedPairs>,
    auto_closed_pairs_after: Option<AutoClosedPairs>,
}

/// One transaction handed back to be replayed, with the cursors to restore
/// once the changes have been applied.
pub(crate) struct Replay {
    pub(super) changes: Vec<Change>,
    pub(super) selections: Option<Vec<CursorSelection>>,
    pub(super) auto_closed_pairs: Option<AutoClosedPairs>,
}

/// Coordinates undo and redo as explicit editing transactions.
///
/// Each edit first creates a transaction. Compatible adjacent transactions
/// may then coalesce until an explicit boundary is encountered. Callers that
/// perform one logical edit through several changes (IME composition, and any
/// multi-cursor edit) bracket those changes with `begin_transaction` and
/// `commit_transaction`. The bracket nests, so an outer caller can group
/// several already-bracketed edits into one undo entry.
#[derive(Debug)]
pub(crate) struct UndoManager {
    undo_transactions: Vec<UndoTransaction>,
    redo_transactions: Vec<UndoTransaction>,
    ignoring: bool,
    transaction_depth: usize,
    pending: Option<PendingTransaction>,
    pending_intent: Option<EditIntent>,
    coalescing_boundary: bool,
}

impl UndoManager {
    pub(super) fn new() -> Self {
        Self {
            undo_transactions: Vec::new(),
            redo_transactions: Vec::new(),
            ignoring: false,
            transaction_depth: 0,
            pending: None,
            pending_intent: None,
            coalescing_boundary: false,
        }
    }

    /// The intent requested for the next recorded change, taken by the edit
    /// that records it.
    pub(super) fn take_pending_intent(&mut self) -> Option<EditIntent> {
        self.pending_intent.take()
    }

    /// Request the intent to record the next change with.
    pub(super) fn set_pending_intent(&mut self, intent: EditIntent) {
        self.pending_intent = Some(intent);
    }

    pub(super) fn record_transaction(&mut self, change: Change, intent: EditIntent) -> bool {
        if self.ignoring {
            return false;
        }
        if change.token_delta.is_none()
            && change.old_range == change.new_range
            && change.old_text == change.new_text
        {
            self.break_transaction_coalescing();
            return false;
        }

        match self.pending.as_mut() {
            Some(pending) => pending.changes.push(change),
            None => self.push_batch(vec![change], intent),
        }
        true
    }

    /// Open a transaction whose changes commit as one atomic undo entry.
    pub(super) fn begin_transaction(&mut self) {
        self.begin_transaction_with(EditIntent::Atomic);
    }

    /// Open a transaction that commits with `intent`, so a following batch of
    /// the same intent and shape can coalesce into it. Multi-cursor typing uses
    /// this to group a burst of keystrokes the way single-cursor typing does.
    ///
    /// Only the outermost bracket decides the intent. A nested
    /// `begin_transaction` merely keeps the batch open.
    pub(super) fn begin_transaction_with(&mut self, intent: EditIntent) {
        self.transaction_depth += 1;
        if self.transaction_depth == 1 {
            self.pending = Some(PendingTransaction {
                intent,
                changes: Vec::new(),
                selections_before: None,
                selections_after: None,
                auto_closed_pairs_before: None,
                auto_closed_pairs_after: None,
            });
        }
    }

    pub(super) fn commit_transaction(&mut self) {
        if self.transaction_depth == 0 {
            return;
        }

        self.transaction_depth -= 1;
        if self.transaction_depth > 0 {
            return;
        }

        let Some(pending) = self.pending.take() else {
            return;
        };
        // A composition that ends where it started (typed then canceled) leaves
        // the document untouched and must not become an undo entry.
        if pending.changes.is_empty() || is_noop_batch(&pending.changes) {
            return;
        }
        self.push_batch(pending.changes, pending.intent);
        if let Some(before) = pending.selections_before {
            self.record_selections_before(before);
        }
        if let Some(after) = pending.selections_after {
            self.record_selections_after(after);
        }
        // These snapshots were recorded with the edits. Commit them even when
        // replay has just enabled ignoring to flush an open IME transaction.
        if let Some(transaction) = self.undo_transactions.last_mut() {
            if let Some(before) = pending.auto_closed_pairs_before {
                transaction.auto_closed_pairs_before.get_or_insert(before);
            }
            if let Some(after) = pending.auto_closed_pairs_after {
                transaction.auto_closed_pairs_after = Some(after);
            }
        }
    }

    /// Push one logical edit, which is one or more changes in application
    /// order, onto the undo stack.
    fn push_batch(&mut self, changes: Vec<Change>, intent: EditIntent) {
        if changes.is_empty() {
            return;
        }

        self.redo_transactions.clear();
        let can_coalesce = !self.coalescing_boundary
            && intent != EditIntent::Atomic
            && self.undo_transactions.last().is_some_and(|previous| {
                previous.intent == intent
                    && previous.last_batch_len == changes.len()
                    && previous.recorded_changes + changes.len() <= MAX_CHANGES_PER_TRANSACTION
                    && is_adjacent_batch(intent, previous.trailing_batch(), &changes)
            });

        if can_coalesce {
            let previous = self
                .undo_transactions
                .last_mut()
                .expect("coalescing requires a previous transaction");
            previous.last_batch_len = changes.len();
            previous.recorded_changes += changes.len();
            // Adjacent single-cursor keystrokes form one contiguous insertion.
            // Keep it as one change so undo and redo replay it as a single
            // edit rather than once per keystroke.
            if intent == EditIntent::Typing
                && let [change] = changes.as_slice()
                && let Some(last) = previous.changes.last_mut()
            {
                last.new_text
                    .truncate(change.old_range.start - last.new_range.start);
                last.new_text.push_str(&change.new_text);
                last.new_range.end = change.new_range.end;
                return;
            }
            previous.changes.extend(changes);
            return;
        }

        if self.undo_transactions.len() >= MAX_UNDO_TRANSACTIONS {
            self.undo_transactions.remove(0);
        }
        self.undo_transactions.push(UndoTransaction {
            intent,
            last_batch_len: changes.len(),
            recorded_changes: changes.len(),
            changes,
            selections_before: None,
            selections_after: None,
            auto_closed_pairs_before: None,
            auto_closed_pairs_after: None,
        });
        self.coalescing_boundary = intent == EditIntent::Atomic;
    }

    /// Record the cursors around the transaction being built, or around the
    /// most recent one when no bracket is open.
    ///
    /// A transaction keeps the cursors it was entered with, so a coalescing
    /// burst of keystrokes still undoes to where the burst began.
    pub(super) fn record_selections(
        &mut self,
        before: Vec<CursorSelection>,
        after: Vec<CursorSelection>,
    ) {
        if self.ignoring {
            return;
        }
        self.record_selections_before(before);
        self.record_selections_after(after);
    }

    pub(super) fn record_auto_closed_pairs(
        &mut self,
        before: AutoClosedPairs,
        after: AutoClosedPairs,
    ) {
        if self.ignoring {
            return;
        }
        if let Some(pending) = self.pending.as_mut() {
            pending.auto_closed_pairs_before.get_or_insert(before);
        } else if let Some(transaction) = self.undo_transactions.last_mut() {
            transaction.auto_closed_pairs_before.get_or_insert(before);
        }
        self.record_auto_closed_pairs_after(after);
    }

    pub(super) fn record_auto_closed_pairs_after(&mut self, after: AutoClosedPairs) {
        if self.ignoring {
            return;
        }
        if let Some(pending) = self.pending.as_mut() {
            pending.auto_closed_pairs_after = Some(after);
        } else if let Some(transaction) = self.undo_transactions.last_mut() {
            transaction.auto_closed_pairs_after = Some(after);
        }
    }

    fn record_selections_before(&mut self, before: Vec<CursorSelection>) {
        if let Some(pending) = self.pending.as_mut() {
            pending.selections_before.get_or_insert(before);
        } else if let Some(transaction) = self.undo_transactions.last_mut() {
            transaction.selections_before.get_or_insert(before);
        }
    }

    fn record_selections_after(&mut self, after: Vec<CursorSelection>) {
        if let Some(pending) = self.pending.as_mut() {
            pending.selections_after = Some(after);
        } else if let Some(transaction) = self.undo_transactions.last_mut() {
            transaction.selections_after = Some(after);
        }
    }

    /// True while a `begin_transaction` bracket is collecting changes.
    pub(super) fn has_open_transaction(&self) -> bool {
        self.transaction_depth > 0
    }

    pub(super) fn break_transaction_coalescing(&mut self) {
        // While a batch is open the boundary applies to that batch, so never
        // close a bracket the caller still owns.
        if self.transaction_depth == 0 {
            self.commit_all_transactions();
        }
        self.coalescing_boundary = true;
    }

    /// Close every open bracket, committing whatever they collected.
    fn commit_all_transactions(&mut self) {
        if self.transaction_depth == 0 {
            return;
        }
        self.transaction_depth = 1;
        self.commit_transaction();
    }

    pub(super) fn is_ignoring(&self) -> bool {
        self.ignoring
    }

    pub(super) fn set_ignoring(&mut self, ignoring: bool) {
        self.ignoring = ignoring;
        if ignoring {
            self.commit_all_transactions();
        }
    }

    pub(super) fn clear(&mut self) {
        self.undo_transactions.clear();
        self.redo_transactions.clear();
        self.transaction_depth = 0;
        self.pending = None;
        self.pending_intent = None;
        self.coalescing_boundary = false;
    }

    pub(super) fn undo(&mut self) -> Option<Replay> {
        self.commit_all_transactions();
        let transaction = self.undo_transactions.pop()?;
        let replay = Replay {
            changes: transaction.changes.iter().rev().cloned().collect(),
            selections: transaction.selections_before.clone(),
            auto_closed_pairs: transaction.auto_closed_pairs_before.clone(),
        };
        self.redo_transactions.push(transaction);
        self.coalescing_boundary = true;
        Some(replay)
    }

    pub(super) fn redo(&mut self) -> Option<Replay> {
        self.commit_all_transactions();
        let transaction = self.redo_transactions.pop()?;
        let replay = Replay {
            changes: transaction.changes.clone(),
            selections: transaction.selections_after.clone(),
            auto_closed_pairs: transaction.auto_closed_pairs_after.clone(),
        };
        self.undo_transactions.push(transaction);
        self.coalescing_boundary = true;
        Some(replay)
    }

    #[cfg(test)]
    pub(super) fn has_undos(&self) -> bool {
        !self.undo_transactions.is_empty()
    }

    #[cfg(test)]
    pub(super) fn undo_count(&self) -> usize {
        self.undo_transactions.len()
    }
}

impl UndoTransaction {
    /// The changes contributed by the most recent batch, which are the ones a
    /// following batch has to be adjacent to.
    fn trailing_batch(&self) -> &[Change] {
        &self.changes[self.changes.len() - self.last_batch_len..]
    }
}

/// True when a chain of changes that each rewrite the region the previous one
/// produced leaves the document exactly as it was found.
///
/// Changes that do not form such a chain (multi-cursor batches, for one) always
/// report `false`, so this only ever collapses the single-region case.
fn is_noop_batch(changes: &[Change]) -> bool {
    if changes.iter().any(|c| c.token_delta.is_some()) {
        return false;
    }
    let Some(first) = changes.first() else {
        return true;
    };

    let mut last = first;
    for change in &changes[1..] {
        if last.new_range.start != change.old_range.start
            || last.new_range.end != change.old_range.end
        {
            return false;
        }
        last = change;
    }

    first.old_range.start == last.new_range.start
        && first.old_range.end == last.new_range.end
        && first.old_text == last.new_text
}

/// True when every change in `current` continues the change at the same
/// position in `previous`, so the two batches are one editing gesture.
///
/// A batch applies from the highest offset down, so each of its changes is
/// still to be shifted by the ones that follow it before the next batch, made
/// against the settled document, can line up with it.
fn is_adjacent_batch(intent: EditIntent, previous: &[Change], current: &[Change]) -> bool {
    if previous.len() != current.len() {
        return false;
    }

    let mut shifts = vec![0isize; previous.len()];
    let mut shift = 0isize;
    for (index, change) in previous.iter().enumerate().rev() {
        shifts[index] = shift;
        shift += change.new_text.len() as isize - change.old_text.len() as isize;
    }

    previous
        .iter()
        .zip(current)
        .zip(shifts)
        .all(|((previous, current), shift)| is_adjacent(intent, &previous.shifted(shift), current))
}

fn is_adjacent(intent: EditIntent, previous: &Change, current: &Change) -> bool {
    if previous.token_delta.is_some() || current.token_delta.is_some() {
        return false;
    }
    match intent {
        EditIntent::Typing => {
            previous.old_range.is_empty()
                && !previous.new_text.contains(['\n', '\r'])
                && !current.new_text.contains(['\n', '\r'])
                && previous.new_range.start <= current.old_range.start
                && previous.new_range.end == current.old_range.end
        }
        EditIntent::Backspace => {
            previous.new_text.is_empty()
                && current.new_text.is_empty()
                && current.old_range.end == previous.old_range.start
        }
        EditIntent::DeleteForward => {
            previous.new_text.is_empty()
                && current.new_text.is_empty()
                && current.old_range.start == previous.old_range.start
        }
        EditIntent::Atomic => false,
    }
}

/// Carrying history over an edit the person did not make (Argus patch; see
/// `patches/gpui-base/README.md`).
impl UndoManager {
    /// An edit that is not in this history replaced `edit` with `new_len` bytes of text, in the
    /// document as it stands. Every entry is rewritten to read as if the edit had always been
    /// there, so undo and redo still reach the person's own changes and never the edit.
    ///
    /// An entry whose text the edit touches cannot be rewritten: that entry and every one that
    /// would have to be undone (or redone) after it are dropped, so history never replays text
    /// that is no longer there.
    pub(super) fn carry_over_external_edit(&mut self, edit: &Range<usize>, new_len: usize) {
        self.commit_all_transactions();
        let delta = new_len as isize - edit.len() as isize;

        // Undo walks back from the document as it is: `at` is where the edit sits at the depth
        // reached, and passing back over a change before it moves it.
        let mut at = edit.clone();
        let mut cut = None;
        'undo: for (index, transaction) in self.undo_transactions.iter_mut().enumerate().rev() {
            shift_snapshot(&mut transaction.selections_after, &at, delta);
            for change in transaction.changes.iter_mut().rev() {
                if at.end <= change.new_range.start {
                    *change = change.shifted(delta);
                } else if at.start >= change.new_range.end {
                    let back = change.old_text.len() as isize - change.new_text.len() as isize;
                    at = shift_by(at.start, back)..shift_by(at.end, back);
                } else {
                    cut = Some(index);
                    break 'undo;
                }
            }
            shift_snapshot(&mut transaction.selections_before, &at, delta);
            transaction.auto_closed_pairs_before = None;
            transaction.auto_closed_pairs_after = None;
        }
        if let Some(index) = cut {
            self.undo_transactions.drain(..=index);
        }

        // Redo walks forward from the same document.
        let mut at = edit.clone();
        let mut cut = None;
        'redo: for (index, transaction) in self.redo_transactions.iter_mut().enumerate().rev() {
            shift_snapshot(&mut transaction.selections_before, &at, delta);
            for change in &mut transaction.changes {
                if at.end <= change.old_range.start {
                    *change = change.shifted(delta);
                } else if at.start >= change.old_range.end {
                    let on = change.new_text.len() as isize - change.old_text.len() as isize;
                    at = shift_by(at.start, on)..shift_by(at.end, on);
                } else {
                    cut = Some(index);
                    break 'redo;
                }
            }
            shift_snapshot(&mut transaction.selections_after, &at, delta);
            transaction.auto_closed_pairs_before = None;
            transaction.auto_closed_pairs_after = None;
        }
        if let Some(index) = cut {
            self.redo_transactions.drain(..=index);
        }
        self.coalescing_boundary = true;
    }
}

fn shift_by(offset: usize, delta: isize) -> usize {
    (offset as isize + delta).max(0) as usize
}

/// The cursors a transaction restores, moved over an edit that sits at `at`: one before it stays,
/// one after it follows it, one inside it goes to its start.
fn shift_snapshot(snapshot: &mut Option<Vec<CursorSelection>>, at: &Range<usize>, delta: isize) {
    let map = |offset: usize| {
        if offset <= at.start {
            offset
        } else if offset >= at.end {
            shift_by(offset, delta)
        } else {
            at.start
        }
    };
    for selection in snapshot.iter_mut().flatten() {
        selection.start = map(selection.start);
        selection.end = map(selection.end);
        selection.column_anchor = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typing_change(offset: usize, text: &str) -> Change {
        let end = offset + text.len();
        Change::new(offset..offset, "", offset..end, text)
    }

    #[test]
    fn adjacent_typing_transactions_coalesce() {
        let mut manager = UndoManager::new();
        manager.record_transaction(typing_change(0, "a"), EditIntent::Typing);
        manager.record_transaction(typing_change(1, "b"), EditIntent::Typing);

        // The run is kept as one insertion, so undo replays a single change.
        assert_eq!(manager.undo().unwrap().changes, [typing_change(0, "ab")]);
        assert!(manager.undo().is_none());
    }

    #[test]
    fn explicit_transaction_collects_multiple_changes() {
        let mut manager = UndoManager::new();
        manager.begin_transaction();
        manager.record_transaction(typing_change(0, "a"), EditIntent::Typing);
        manager.record_transaction(typing_change(1, "b"), EditIntent::Typing);
        manager.commit_transaction();

        // One undo entry that replays its changes in reverse application
        // order.
        let transaction = manager.undo().unwrap().changes;
        assert_eq!(transaction.len(), 2);
        assert_eq!(transaction[0].new_text, "b");
        assert_eq!(transaction[1].new_text, "a");
        assert!(manager.undo().is_none());
    }

    #[test]
    fn nested_transactions_commit_as_one_entry() {
        let mut manager = UndoManager::new();
        manager.begin_transaction();
        manager.record_transaction(typing_change(0, "a"), EditIntent::Typing);
        manager.begin_transaction();
        manager.record_transaction(typing_change(1, "b"), EditIntent::Typing);
        manager.commit_transaction();
        // The inner bracket does not close the transaction.
        assert!(!manager.has_undos());
        manager.record_transaction(typing_change(2, "c"), EditIntent::Typing);
        manager.commit_transaction();

        assert_eq!(manager.undo().unwrap().changes.len(), 3);
        assert!(manager.undo().is_none());
    }

    #[test]
    fn batches_of_the_same_shape_and_intent_coalesce() {
        let mut manager = UndoManager::new();

        // Two cursors typing "a" then "b", as a multi-cursor keystroke burst.
        // A batch applies from the highest offset down, and the second burst
        // sees the offsets the first one left behind.
        manager.begin_transaction_with(EditIntent::Typing);
        manager.record_transaction(typing_change(5, "a"), EditIntent::Typing);
        manager.record_transaction(typing_change(0, "a"), EditIntent::Typing);
        manager.commit_transaction();
        manager.begin_transaction_with(EditIntent::Typing);
        manager.record_transaction(typing_change(7, "b"), EditIntent::Typing);
        manager.record_transaction(typing_change(1, "b"), EditIntent::Typing);
        manager.commit_transaction();

        assert_eq!(manager.undo().unwrap().changes.len(), 4);
        assert!(manager.undo().is_none());
    }

    #[test]
    fn a_batch_does_not_coalesce_into_a_differently_shaped_one() {
        let mut manager = UndoManager::new();

        manager.begin_transaction_with(EditIntent::Typing);
        manager.record_transaction(typing_change(5, "a"), EditIntent::Typing);
        manager.record_transaction(typing_change(0, "a"), EditIntent::Typing);
        manager.commit_transaction();
        // One cursor left: a single change cannot continue a two-cursor batch.
        manager.record_transaction(typing_change(1, "b"), EditIntent::Typing);

        assert_eq!(manager.undo().unwrap().changes.len(), 1);
        assert_eq!(manager.undo().unwrap().changes.len(), 2);
    }

    #[test]
    fn an_atomic_batch_never_coalesces() {
        let mut manager = UndoManager::new();

        manager.begin_transaction();
        manager.record_transaction(typing_change(0, "a"), EditIntent::Typing);
        manager.commit_transaction();
        manager.record_transaction(typing_change(1, "b"), EditIntent::Typing);

        assert_eq!(manager.undo().unwrap().changes.len(), 1);
        assert_eq!(manager.undo().unwrap().changes.len(), 1);
    }

    #[test]
    fn limits_the_number_of_retained_transactions() {
        let mut manager = UndoManager::new();

        for offset in 0..1_100 {
            manager.record_transaction(typing_change(offset, "a"), EditIntent::Atomic);
        }

        for _ in 0..MAX_UNDO_TRANSACTIONS {
            assert!(manager.undo().is_some());
        }
        assert!(manager.undo().is_none());
    }

    #[test]
    fn splits_a_coalesced_transaction_before_its_change_list_grows_too_large() {
        let mut manager = UndoManager::new();

        for offset in 0..1_100 {
            manager.record_transaction(typing_change(offset, "a"), EditIntent::Typing);
        }

        // Each run is merged into one insertion, but the split still happens
        // after the same number of keystrokes.
        assert_eq!(
            manager.undo().unwrap().changes,
            [typing_change(MAX_CHANGES_PER_TRANSACTION, &"a".repeat(100))]
        );
        assert_eq!(
            manager.undo().unwrap().changes,
            [typing_change(0, &"a".repeat(MAX_CHANGES_PER_TRANSACTION))]
        );
        assert!(manager.undo().is_none());
    }
}

#[cfg(test)]
mod external_edit_tests {
    use super::*;

    fn typed(offset: usize, text: &str) -> Change {
        Change::new(offset..offset, "", offset..offset + text.len(), text)
    }

    fn atomic(change: Change) -> UndoManager {
        let mut manager = UndoManager::new();
        manager.record_transaction(change, EditIntent::Atomic);
        manager
    }

    #[test]
    fn an_edit_before_a_change_moves_it_and_undo_still_finds_its_text() {
        let mut manager = atomic(typed(10, "abc"));
        manager.carry_over_external_edit(&(0..0), 4);
        let undo = manager.undo().unwrap().changes;
        assert_eq!(undo, [typed(14, "abc")]);
    }

    #[test]
    fn an_edit_after_a_change_leaves_it_alone() {
        let mut manager = atomic(typed(10, "abc"));
        manager.carry_over_external_edit(&(40..50), 0);
        assert_eq!(manager.undo().unwrap().changes, [typed(10, "abc")]);
    }

    #[test]
    fn an_edit_that_touches_a_change_drops_it_and_what_was_done_before_it() {
        let mut manager = UndoManager::new();
        manager.record_transaction(typed(0, "x"), EditIntent::Atomic);
        manager.record_transaction(typed(10, "abc"), EditIntent::Atomic);
        manager.record_transaction(typed(20, "z"), EditIntent::Atomic);
        // The edit is inside the second change's text, so that and the first go.
        manager.carry_over_external_edit(&(11..12), 1);
        assert_eq!(manager.undo().unwrap().changes, [typed(20, "z")]);
        assert!(manager.undo().is_none());
    }

    #[test]
    fn an_edit_between_two_changes_moves_only_the_older_ones_positions_correctly() {
        let mut manager = UndoManager::new();
        manager.record_transaction(typed(30, "late"), EditIntent::Atomic);
        // `at` is where the edit sits in the document as it is; the older change lies after the
        // edit once the newer one is undone, so it moves by the edit's growth.
        manager.record_transaction(typed(2, "ab"), EditIntent::Atomic);
        manager.carry_over_external_edit(&(10..10), 3);
        assert_eq!(manager.undo().unwrap().changes, [typed(2, "ab")]);
        assert_eq!(manager.undo().unwrap().changes, [typed(33, "late")]);
    }

    #[test]
    fn redo_entries_are_carried_the_same_way() {
        let mut manager = atomic(typed(10, "abc"));
        manager.undo();
        manager.carry_over_external_edit(&(0..0), 2);
        assert_eq!(manager.redo().unwrap().changes, [typed(12, "abc")]);
        let replaced = Change::new(10..14, "wxyz", 10..13, "abc");
        let mut manager = atomic(replaced);
        manager.undo();
        manager.carry_over_external_edit(&(12..13), 2);
        assert!(manager.redo().is_none(), "an edit inside the text a redo would replace drops it");
    }
}
