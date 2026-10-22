//! The invariant the whole editor rests on, over random interleavings.

use proptest::prelude::*;

use super::*;

/// One thing that can happen next. Positions are picks, cut to the text at
/// hand when the step runs.
#[derive(Debug, Clone)]
enum Step {
    Type(usize, usize, u8),
    Agent(usize, usize, u8),
    Take,
    Answer,
    Hear,
    Resolve(bool),
    /// The connection goes; the write in flight may or may not have landed.
    Lost(bool),
    /// The connection is back and the file is opened again.
    Reopen,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (0usize..64, 0usize..64, 0u8..6).prop_map(|(a, b, p)| Step::Type(a, b, p)),
        2 => (0usize..64, 0usize..64, 0u8..6).prop_map(|(a, b, p)| Step::Agent(a, b, p)),
        2 => Just(Step::Take),
        2 => Just(Step::Answer),
        2 => Just(Step::Hear),
        1 => any::<bool>().prop_map(Step::Resolve),
        1 => any::<bool>().prop_map(Step::Lost),
        1 => Just(Step::Reopen),
    ]
}

/// A change to `text` at char boundaries, whole characters only: a real
/// editor never splits a surrogate pair.
fn edit(text: &str, a: usize, b: usize, piece: u8) -> Option<Change> {
    let mut bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    bounds.push(text.len());
    let (a, b) = (bounds[a % bounds.len()], bounds[b % bounds.len()]);
    let (from, to) = (a.min(b), a.max(b).min(a.min(b) + 6));
    let to = (to..=text.len()).find(|&t| text.is_char_boundary(t)).unwrap_or(text.len());
    let insert = ["x", "yz", "\n", " ", "😀", ""][piece as usize];
    Some(Change { from, to, insert: insert.into() })
}

struct World {
    disk: Disk,
    c: Client,
    in_flight: Option<Write>,
    offline: bool,
}

impl World {
    fn new() -> World {
        let disk = Disk::new("fn a() {}\nfn b() {}\n");
        let c = Client::new(&disk.text);
        World { disk, c, in_flight: None, offline: false }
    }

    fn answer(&mut self) {
        let Some(w) = self.in_flight.take() else { return };
        let saved = self.disk.edit(w.base, &w.changes);
        while let Some(told) = self.disk.told.pop_front() {
            self.c.hear(told);
        }
        match saved {
            Some(v) => self.c.sync.saved(w.base, v),
            None => self.c.sync.refused(w.base, None),
        }
    }

    fn hear_all(&mut self) {
        while let Some(told) = self.disk.told.pop_front() {
            self.c.hear(told);
        }
    }

    fn reopen(&mut self) {
        self.offline = false;
        self.disk.told.clear();
        // A reopened file answers version 0, as the daemon does.
        self.disk.version = 0;
        let shown = self.c.sync.reopened(&self.disk.text.clone(), 0);
        self.c.show(Some(shown));
    }

    fn run(&mut self, step: &Step) {
        match *step {
            Step::Type(a, b, p) => {
                let text = self.c.view.to_string();
                if let Some(ch) = edit(&text, a, b, p) {
                    let set = ChangeSet::of(&[ch], text.len()).unwrap();
                    self.c.type_(&set);
                }
            }
            Step::Agent(a, b, p) => {
                if let Some(ch) = edit(&self.disk.text, a, b, p) {
                    let mut t = self.disk.text.clone();
                    t.replace_range(ch.from..ch.to, &ch.insert);
                    // While the connection is down nothing is told.
                    self.disk.agent(&t);
                    if self.offline {
                        self.disk.told.clear();
                    }
                }
            }
            Step::Take => {
                if !self.offline && self.in_flight.is_none() {
                    self.in_flight = self.c.sync.take();
                }
            }
            Step::Answer => {
                if !self.offline {
                    self.answer();
                }
            }
            Step::Hear => {
                if !self.offline {
                    self.hear_all();
                }
            }
            Step::Resolve(mine) => {
                if self.c.sync.clashing() {
                    let shown = self.c.sync.resolve(if mine { Keep::Mine } else { Keep::Theirs });
                    self.c.show(shown);
                }
            }
            Step::Lost(landed) => {
                if self.offline {
                    return;
                }
                if let (true, Some(w)) = (landed, &self.in_flight) {
                    self.disk.edit(w.base, &w.changes);
                }
                self.in_flight = None;
                self.disk.told.clear();
                self.offline = true;
                self.c.sync.lost();
            }
            Step::Reopen => {
                if self.offline {
                    self.reopen();
                }
            }
        }
    }

    /// Everything lands: the connection is back, what is in flight is
    /// answered, every clash is kept as the person's, and the rest written.
    fn settle(&mut self) {
        if self.offline {
            self.reopen();
        }
        for _ in 0..40 {
            self.answer();
            self.hear_all();
            if self.c.sync.clashing() {
                let shown = self.c.sync.resolve(Keep::Mine);
                self.c.show(shown);
            }
            self.in_flight = self.c.sync.take();
            if self.in_flight.is_none() && !self.c.sync.pending() {
                return;
            }
        }
        panic!("the sync did not settle");
    }
}

proptest! {
    // Source: `page/test/code/sync.test.mjs` "whatever order typing, writes
    // and agent writes land in, the buffer ends as the disk" (EDITOR-SYNC-14),
    // widened with a connection that goes and comes back (EDITOR-RECONN-4).
    #[test]
    fn whatever_order_typing_writes_and_agent_writes_land_in_the_buffer_ends_as_the_disk(
        steps in proptest::collection::vec(step(), 1..60)
    ) {
        let mut w = World::new();
        for s in &steps {
            w.run(s);
            prop_assert_eq!(w.c.view.to_string(), w.c.sync.doc().to_string(), "the view is the buffer after {:?}", s);
        }
        w.settle();
        prop_assert_eq!(w.c.view.to_string(), w.disk.text.clone());
        prop_assert_eq!(w.c.sync.doc().to_string(), w.disk.text.clone());
    }

    // EDITOR-CLASH-2: nothing is written over a clash held.
    #[test]
    fn nothing_is_taken_to_write_while_a_clash_is_held(steps in proptest::collection::vec(step(), 1..60)) {
        let mut w = World::new();
        for s in &steps {
            w.run(s);
            if w.c.sync.clashing() {
                let before = w.c.sync.pending();
                prop_assert_eq!(w.c.sync.take(), None);
                prop_assert_eq!(w.c.sync.pending(), before);
            }
        }
    }
}
