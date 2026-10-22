//! `sync.test.mjs`'s examples, and the daemon they are played against.

use std::collections::VecDeque;

use ropey::Rope;

use super::*;

mod properties;

/// What the daemon tells of a write by anything else: the change from one
/// version to the next.
pub struct Told {
    pub from: u64,
    pub version: u64,
    pub changes: Vec<TextChange>,
}

/// The daemon as `ClientMsg::Edit` and `EditChanged` describe it
/// (argus-protocol `code/edit`): a change applies only to the version it
/// was made on; any other write is told as the change from one version to
/// the next. `between` stands in for its line diff, which argusd tests.
pub struct Disk {
    pub text: String,
    pub version: u64,
    pub told: VecDeque<Told>,
}

impl Disk {
    pub fn new(text: &str) -> Disk {
        Disk { text: text.into(), version: 0, told: VecDeque::new() }
    }

    /// The version it is saved as, or `None` when `base` is not the one held.
    pub fn edit(&mut self, base: u64, changes: &[TextChange]) -> Option<u64> {
        if base != self.version {
            return None;
        }
        let doc = Rope::from_str(&self.text);
        self.text = from_wire(changes, &doc).expect("the client sends what fits").apply(&doc).to_string();
        self.version += 1;
        Some(self.version)
    }

    pub fn agent(&mut self, text: &str) {
        let doc = Rope::from_str(&self.text);
        let changes = match between(&self.text, text) {
            Some(c) => to_wire(&ChangeSet::of(&[c], doc.len()).unwrap(), &doc),
            None => Vec::new(),
        };
        self.told.push_back(Told { from: self.version, version: self.version + 1, changes });
        self.text = text.into();
        self.version += 1;
    }
}

/// A client: the sync and the buffer a view would show.
pub struct Client {
    pub sync: Synced,
    pub view: Rope,
}

impl Client {
    pub fn new(text: &str) -> Client {
        Client { sync: Synced::new(text, 0), view: Rope::from_str(&lf(text)) }
    }

    pub fn type_(&mut self, set: &ChangeSet) {
        set.apply_in_place(&mut self.view);
        self.sync.typed(set);
    }

    pub fn hear(&mut self, told: Told) {
        let shown = self
            .sync
            .changed(told.from, told.version, &told.changes)
            .expect("the changes fit")
            .expect("made on the version held");
        shown.apply_in_place(&mut self.view);
    }

    pub fn show(&mut self, shown: Option<ChangeSet>) {
        if let Some(shown) = shown {
            shown.apply_in_place(&mut self.view);
        }
    }
}

fn at(len: usize, from: usize, to: usize, insert: &str) -> ChangeSet {
    ChangeSet::of(&[Change { from, to, insert: insert.into() }], len).unwrap()
}

#[test]
fn an_agent_write_clear_of_the_typing_lands_in_the_buffer_and_the_typing_goes_on_top() {
    let mut disk = Disk::new("one\ntwo\nthree");
    let mut c = Client::new(&disk.text);
    c.type_(&at(c.view.len(), 0, 3, "ONE"));
    disk.agent("one\ntwo\nthree\nfour");
    c.hear(disk.told.pop_front().unwrap());
    assert_eq!(c.view.to_string(), "ONE\ntwo\nthree\nfour");
    assert!(!c.sync.clashing());

    let write = c.sync.take().unwrap();
    let saved = disk.edit(write.base, &write.changes).unwrap();
    c.sync.saved(write.base, saved);
    assert_eq!(disk.text, "ONE\ntwo\nthree\nfour");
}

#[test]
fn a_write_made_while_an_agent_wrote_is_refused_and_made_again_on_its_text() {
    let mut disk = Disk::new("a\nb");
    let mut c = Client::new(&disk.text);
    c.type_(&at(c.view.len(), 3, 3, "!"));
    let write = c.sync.take().unwrap();
    disk.agent("A\nb");
    c.hear(disk.told.pop_front().unwrap());
    assert_eq!(disk.edit(write.base, &write.changes), None);
    c.sync.refused(write.base, None);

    let again = c.sync.take().unwrap();
    let saved = disk.edit(again.base, &again.changes).unwrap();
    c.sync.saved(again.base, saved);
    assert_eq!(disk.text, "A\nb!");
    assert!(!c.sync.pending());
}

#[test]
fn a_clash_is_held_until_the_person_keeps_theirs_or_the_agents() {
    for keep in [Keep::Mine, Keep::Theirs] {
        let mut disk = Disk::new("let x = 1;");
        let mut c = Client::new(&disk.text);
        c.type_(&at(c.view.len(), 8, 9, "2"));
        disk.agent("let x = 3;");
        c.hear(disk.told.pop_front().unwrap());
        assert!(c.sync.clashing());
        assert_eq!(c.view.to_string(), "let x = 2;", "the buffer is left as typed");
        assert_eq!(c.sync.clashes(), vec![Clash { from: 8, to: 9, theirs: "3".into() }]);
        assert_eq!(c.sync.take(), None, "nothing is written over a clash");

        let shown = c.sync.resolve(keep);
        c.show(shown);
        if let Some(write) = c.sync.take() {
            let saved = disk.edit(write.base, &write.changes).unwrap();
            c.sync.saved(write.base, saved);
        }
        let want = if keep == Keep::Mine { "let x = 2;" } else { "let x = 3;" };
        assert_eq!(c.view.to_string(), want);
        assert_eq!(disk.text, want);
    }
}

#[test]
fn a_change_made_on_another_version_asks_for_the_file_again() {
    let mut c = Client::new("abc");
    assert_eq!(c.sync.changed(7, 8, &[]).unwrap(), None);
    assert_eq!(c.sync.version(), 0, "the version held is not moved by it");
}

#[test]
fn a_change_that_does_not_fit_is_refused_and_leaves_the_sync_as_it_was() {
    let mut c = Client::new("abc");
    let bad = [TextChange { from: 2, to: 9, insert: String::new() }];
    assert!(c.sync.changed(0, 1, &bad).is_err());
    assert_eq!(c.sync.version(), 0);
    assert_eq!(c.sync.doc().to_string(), "abc");
}

#[test]
fn a_write_in_flight_is_the_only_one() {
    let mut c = Client::new("abc");
    c.type_(&at(3, 0, 0, "x"));
    assert!(c.sync.take().is_some());
    c.type_(&at(4, 0, 0, "y"));
    assert_eq!(c.sync.take(), None);
    assert!(c.sync.pending());
}

#[test]
fn a_refusal_with_a_reason_types_the_write_again() {
    let mut c = Client::new("abc");
    c.type_(&at(3, 3, 3, "!"));
    let write = c.sync.take().unwrap();
    c.sync.refused(write.base, Some("could not write a.rs"));
    let again = c.sync.take().unwrap();
    assert_eq!(again.changes, write.changes);
}

#[test]
fn an_answer_for_a_write_already_folded_back_is_ignored() {
    let mut disk = Disk::new("abc");
    let mut c = Client::new("abc");
    c.type_(&at(3, 3, 3, "!"));
    let write = c.sync.take().unwrap();
    disk.agent("abcd");
    c.hear(disk.told.pop_front().unwrap());
    c.sync.saved(write.base, 9);
    assert_eq!(c.sync.version(), 1, "the version is the agent's, not the stale answer's");
}

#[test]
fn a_file_opened_again_takes_in_what_it_became_while_the_connection_was_down() {
    let mut c = Client::new("one\ntwo\nthree");
    c.type_(&at(13, 0, 3, "ONE"));
    let write = c.sync.take().unwrap();
    assert_eq!(write.base, 0);
    // The connection went with the write in flight; the file gained a line.
    let shown = c.sync.reopened("one\ntwo\nthree\nfour", 0);
    c.show(Some(shown));
    assert_eq!(c.view.to_string(), "ONE\ntwo\nthree\nfour");
    assert!(c.sync.pending(), "the typing is still to be written, on top");
    let write = c.sync.take().unwrap();
    assert_eq!(write.changes, vec![TextChange { from: 0, to: 3, insert: "ONE".into() }]);
}

#[test]
fn between_two_texts_keeps_what_they_share_at_each_end_and_never_splits_a_pair() {
    assert_eq!(between("abc", "abc"), None);
    assert_eq!(between("hello world", "hello brave world"), Some(Change { from: 6, to: 6, insert: "brave ".into() }));
    // "😀" and "😁" share their first UTF-16 unit; the change takes both.
    assert_eq!(between("a😀b", "a😁b"), Some(Change { from: 1, to: 5, insert: "😁".into() }));
}

#[test]
fn line_breaks_count_as_one() {
    assert_eq!(lf("a\r\nb\rc\nd"), "a\nb\nc\nd");
    assert_eq!(Synced::new("a\r\nb", 0).doc().len(), 3);
}
