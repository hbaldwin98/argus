//! What an edit to a text is, as a value that can be composed, mapped over
//! another and inverted: the change-set algebra the live sync is built on.
//! A set is positioned in the text before it, in bytes of the rope that
//! holds the text; the wire's UTF-16 units are `wire`'s to convert. The
//! shape is CodeMirror's `ChangeSet`, which the page's `sync` uses, so the
//! two fold a disk change in the same way.

use std::collections::VecDeque;

use ropey::Rope;

/// `from..to` of the text before replaced by `insert`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

/// A run of the text before a set: kept, or replaced by a (maybe empty)
/// insertion. Neighbouring runs of one kind are always joined, so equal
/// edits are equal values.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Sec {
    Keep(usize),
    Rep { del: usize, ins: String },
}

/// The pieces the operations walk: a replacement is its insertion, which
/// comes first, then its deletion.
#[derive(Debug)]
enum Atom {
    Keep(usize),
    Del(usize),
    Ins(String),
}

impl Atom {
    /// What the piece covers of the text it is positioned in (insertions
    /// cover none), or of the text it makes, for `ins`.
    fn width(&self) -> usize {
        match self {
            Atom::Keep(n) | Atom::Del(n) => *n,
            Atom::Ins(s) => s.len(),
        }
    }

    /// The first `n` bytes of the piece, and what is left.
    fn cut(self, n: usize) -> (Atom, Option<Atom>) {
        if n >= self.width() {
            return (self, None);
        }
        match self {
            Atom::Keep(w) => (Atom::Keep(n), Some(Atom::Keep(w - n))),
            Atom::Del(w) => (Atom::Del(n), Some(Atom::Del(w - n))),
            Atom::Ins(mut s) => {
                let rest = s.split_off(n);
                (Atom::Ins(s), Some(Atom::Ins(rest)))
            }
        }
    }
}

#[derive(Default)]
struct Builder {
    secs: Vec<Sec>,
}

impl Builder {
    fn keep(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        match self.secs.last_mut() {
            Some(Sec::Keep(k)) => *k += n,
            _ => self.secs.push(Sec::Keep(n)),
        }
    }

    fn rep(&mut self) -> &mut Sec {
        if !matches!(self.secs.last(), Some(Sec::Rep { .. })) {
            self.secs.push(Sec::Rep { del: 0, ins: String::new() });
        }
        self.secs.last_mut().expect("just pushed")
    }

    fn del(&mut self, n: usize) {
        if n > 0 {
            if let Sec::Rep { del, .. } = self.rep() {
                *del += n;
            }
        }
    }

    fn ins(&mut self, text: &str) {
        if !text.is_empty() {
            if let Sec::Rep { ins, .. } = self.rep() {
                ins.push_str(text);
            }
        }
    }

    fn finish(self) -> ChangeSet {
        let (mut old, mut new) = (0, 0);
        for sec in &self.secs {
            match sec {
                Sec::Keep(n) => (old, new) = (old + n, new + n),
                Sec::Rep { del, ins } => (old, new) = (old + del, new + ins.len()),
            }
        }
        ChangeSet { secs: self.secs, old, new }
    }
}

/// Changes that do not fit the text they are for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Misfit {
    /// A change runs backwards or past the end of the text.
    Range { from: usize, to: usize },
    /// A change starts before the one ahead of it ends.
    Order { from: usize },
}

impl std::fmt::Display for Misfit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Misfit::Range { from, to } => write!(f, "the change {from}..{to} does not fit the text"),
            Misfit::Order { from } => write!(f, "the change at {from} overlaps or precedes the one before it"),
        }
    }
}

/// A set of changes made together on one text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    secs: Vec<Sec>,
    old: usize,
    new: usize,
}

impl ChangeSet {
    /// No change to a text of `len` bytes.
    pub fn empty(len: usize) -> ChangeSet {
        let mut b = Builder::default();
        b.keep(len);
        b.finish()
    }

    /// `changes`, sorted and not overlapping, on a text of `len` bytes.
    /// Changes that only touch are one run, as they are once applied.
    pub fn of(changes: &[Change], len: usize) -> Result<ChangeSet, Misfit> {
        let mut b = Builder::default();
        let mut at = 0;
        for c in changes {
            if c.from > c.to || c.to > len {
                return Err(Misfit::Range { from: c.from, to: c.to });
            }
            if c.from < at {
                return Err(Misfit::Order { from: c.from });
            }
            b.keep(c.from - at);
            b.del(c.to - c.from);
            b.ins(&c.insert);
            at = c.to;
        }
        b.keep(len - at);
        Ok(b.finish())
    }

    /// The length of the text the set is made on.
    pub fn old_len(&self) -> usize {
        self.old
    }

    /// The length of the text the set makes.
    pub fn new_len(&self) -> usize {
        self.new
    }

    /// Whether it changes nothing.
    pub fn is_empty(&self) -> bool {
        self.secs.iter().all(|s| matches!(s, Sec::Keep(_)))
    }

    /// Each change, positioned in the text before the set.
    pub fn changes(&self) -> Vec<Change> {
        let mut at = 0;
        let mut out = Vec::new();
        for sec in &self.secs {
            match sec {
                Sec::Keep(n) => at += n,
                Sec::Rep { del, ins } => {
                    out.push(Change { from: at, to: at + del, insert: ins.clone() });
                    at += del;
                }
            }
        }
        out
    }

    /// Where each change lands in the text it makes, as `from..to`.
    pub fn new_ranges(&self) -> Vec<(usize, usize)> {
        let mut at = 0;
        let mut out = Vec::new();
        for sec in &self.secs {
            match sec {
                Sec::Keep(n) => at += n,
                Sec::Rep { ins, .. } => {
                    out.push((at, at + ins.len()));
                    at += ins.len();
                }
            }
        }
        out
    }

    fn atoms(&self) -> VecDeque<Atom> {
        let mut out = VecDeque::new();
        for sec in &self.secs {
            match sec {
                Sec::Keep(n) => out.push_back(Atom::Keep(*n)),
                Sec::Rep { del, ins } => {
                    if !ins.is_empty() {
                        out.push_back(Atom::Ins(ins.clone()));
                    }
                    if *del > 0 {
                        out.push_back(Atom::Del(*del));
                    }
                }
            }
        }
        out
    }

    /// The text the set makes of `doc`. Panics if `doc` is not the text
    /// the set was made on: a mismatch is a bug in whoever held the two.
    pub fn apply(&self, doc: &Rope) -> Rope {
        let mut out = doc.clone();
        self.apply_in_place(&mut out);
        out
    }

    pub fn apply_in_place(&self, doc: &mut Rope) {
        assert_eq!(doc.len(), self.old, "a change set applied to a text of another length");
        let mut at = 0;
        for sec in &self.secs {
            match sec {
                Sec::Keep(n) => at += n,
                Sec::Rep { del, ins } => {
                    if *del > 0 {
                        doc.remove(at..at + del);
                    }
                    if !ins.is_empty() {
                        doc.insert(at, ins);
                    }
                    at += ins.len();
                }
            }
        }
    }

    /// The set that undoes this one, made on the text this one makes, given
    /// the text it was made on.
    pub fn invert(&self, doc: &Rope) -> ChangeSet {
        assert_eq!(doc.len(), self.old, "a change set inverted against a text of another length");
        let mut b = Builder::default();
        let mut at = 0;
        for sec in &self.secs {
            match sec {
                Sec::Keep(n) => {
                    b.keep(*n);
                    at += n;
                }
                Sec::Rep { del, ins } => {
                    b.del(ins.len());
                    b.ins(&doc.slice(at..at + del).to_string());
                    at += del;
                }
            }
        }
        b.finish()
    }

    /// This set followed by `then`, which is made on what this makes: one
    /// set from the first text to the last.
    pub fn compose(&self, then: &ChangeSet) -> ChangeSet {
        assert_eq!(self.new, then.old, "composed sets that do not meet");
        let mut a = self.atoms();
        let mut b = then.atoms();
        let mut out = Builder::default();
        loop {
            if let Some(Atom::Del(_)) = a.front() {
                if let Some(Atom::Del(n)) = a.pop_front() {
                    out.del(n);
                }
                continue;
            }
            if let Some(Atom::Ins(_)) = b.front() {
                if let Some(Atom::Ins(s)) = b.pop_front() {
                    out.ins(&s);
                }
                continue;
            }
            let (Some(x), Some(y)) = (a.pop_front(), b.pop_front()) else {
                break;
            };
            let n = x.width().min(y.width());
            let (x, rest) = x.cut(n);
            if let Some(rest) = rest {
                a.push_front(rest);
            }
            let (y, rest) = y.cut(n);
            if let Some(rest) = rest {
                b.push_front(rest);
            }
            match (x, y) {
                (Atom::Keep(_), Atom::Keep(_)) => out.keep(n),
                (Atom::Keep(_), Atom::Del(_)) => out.del(n),
                (Atom::Ins(s), Atom::Keep(_)) => out.ins(&s),
                (Atom::Ins(_), Atom::Del(_)) => {}
                _ => unreachable!("deletions and insertions are taken above"),
            }
        }
        out.finish()
    }

    /// This set, made on the text `over` was also made on, moved to apply
    /// to the text `over` makes. Where both insert at one place this one's
    /// text goes first if `before`, and after `over`'s if not; text both
    /// delete is deleted once; text inserted inside a deletion stays.
    pub fn map(&self, over: &ChangeSet, before: bool) -> ChangeSet {
        assert_eq!(self.old, over.old, "mapped sets made on different texts");
        let mut a = self.atoms();
        let mut b = over.atoms();
        let mut out = Builder::default();
        loop {
            let a_ins = matches!(a.front(), Some(Atom::Ins(_)));
            let b_ins = matches!(b.front(), Some(Atom::Ins(_)));
            if a_ins && (before || !b_ins) {
                if let Some(Atom::Ins(s)) = a.pop_front() {
                    out.ins(&s);
                }
                continue;
            }
            if b_ins {
                if let Some(Atom::Ins(s)) = b.pop_front() {
                    out.keep(s.len());
                }
                continue;
            }
            let (Some(x), Some(y)) = (a.pop_front(), b.pop_front()) else {
                break;
            };
            let n = x.width().min(y.width());
            let (x, rest) = x.cut(n);
            if let Some(rest) = rest {
                a.push_front(rest);
            }
            let (y, rest) = y.cut(n);
            if let Some(rest) = rest {
                b.push_front(rest);
            }
            match (x, y) {
                (Atom::Keep(_), Atom::Keep(_)) => out.keep(n),
                (Atom::Del(_), Atom::Keep(_)) => out.del(n),
                (_, Atom::Del(_)) => {}
                _ => unreachable!("insertions are taken above"),
            }
        }
        out.finish()
    }

    /// Where `pos` of the text before falls in the text after. A position
    /// inside a replaced run goes to its start; one at an insertion goes
    /// before the text for `assoc` < 0 and after it otherwise.
    pub fn map_pos(&self, pos: usize, assoc: i8) -> usize {
        let (mut old, mut new) = (0, 0);
        for sec in &self.secs {
            match sec {
                Sec::Keep(n) => {
                    if old + n > pos {
                        return new + (pos - old);
                    }
                    old += n;
                    new += n;
                }
                Sec::Rep { del, ins } => {
                    let end = old + del;
                    if end > pos || (end == pos && assoc < 0 && *del == 0) {
                        return if pos == old || assoc < 0 { new } else { new + ins.len() };
                    }
                    old = end;
                    new += ins.len();
                }
            }
        }
        new
    }
}

#[cfg(test)]
mod tests;
