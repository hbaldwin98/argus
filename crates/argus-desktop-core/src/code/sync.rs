//! The editor's buffer kept in step with the file on disk, which the
//! daemon referees: what of the buffer is the person's and not yet
//! written, what is in flight, and how a write by anything else — an
//! agent — is folded in. The daemon writes a change only onto the version
//! it was made on, so a change made while an agent wrote is made again on
//! top of the agent's. Where both touched the same text nothing is
//! guessed: the disk's change is held aside until the person says which
//! one stands. This is `sync.ts` over a rope; no window and no timer are
//! in it, so whatever draws the buffer can hold one.

use argus_protocol::TextChange;
use ropey::Rope;

use super::change::{Change, ChangeSet};
use super::clash::{split, Clash};
use super::wire::{from_wire, to_wire, Refused};

/// Whose word stands on a held clash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    Mine,
    Theirs,
}

/// A write to send: `changes` made on version `base`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    pub base: u64,
    pub changes: Vec<TextChange>,
}

/// The text with every line break one `\n`, which is how the editor and the
/// protocol count a break as one position.
pub fn lf(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The single change that turns `a` into `b`: what they share at each end
/// kept. Used only where no diff came from the daemon — a file opened again
/// after the connection went. Whole characters only, so it never falls
/// between the halves of a surrogate pair in the UTF-16 the wire counts.
pub fn between(a: &str, b: &str) -> Option<Change> {
    if a == b {
        return None;
    }
    let start = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).map(|(x, _)| x.len_utf8()).sum::<usize>();
    let (ra, rb) = (&a[start..], &b[start..]);
    let end = ra
        .chars()
        .rev()
        .zip(rb.chars().rev())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.len_utf8())
        .sum::<usize>();
    Some(Change { from: start, to: a.len() - end, insert: b[start..b.len() - end].to_owned() })
}

pub struct Synced {
    /// The disk's text at `version`, or while a clash is held, the text the
    /// held changes are made on.
    base: Rope,
    /// The daemon's latest version of the file.
    version: u64,
    /// Written, and waiting on the daemon: `base` to what was sent.
    sent: Option<ChangeSet>,
    /// Typed since: what was sent, or `base`, to the buffer.
    local: ChangeSet,
    /// The disk's changes that clash with the person's, made on `base`.
    held: Option<ChangeSet>,
}

impl Synced {
    pub fn new(text: &str, version: u64) -> Synced {
        let base = Rope::from_str(&lf(text));
        let local = ChangeSet::empty(base.len());
        Synced { base, version, sent: None, local, held: None }
    }

    /// The daemon's latest version of the file.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Everything not yet on disk: `base` to the buffer.
    fn mine(&self) -> ChangeSet {
        match &self.sent {
            Some(sent) => sent.compose(&self.local),
            None => self.local.clone(),
        }
    }

    /// The buffer as it should stand.
    pub fn doc(&self) -> Rope {
        self.mine().apply(&self.base)
    }

    /// The person typed `changes`, made on the buffer.
    pub fn typed(&mut self, changes: &ChangeSet) {
        self.local = self.local.compose(changes);
    }

    /// Whether anything typed is still not written.
    pub fn pending(&self) -> bool {
        self.sent.is_some() || !self.local.is_empty()
    }

    /// Whether a clash waits on the person.
    pub fn clashing(&self) -> bool {
        self.held.is_some()
    }

    /// What to write now, made on `version`: nothing while a write is in
    /// flight, a clash is held, or nothing was typed.
    pub fn take(&mut self) -> Option<Write> {
        if self.sent.is_some() || self.held.is_some() || self.local.is_empty() {
            return None;
        }
        let fresh = ChangeSet::empty(self.local.new_len());
        let sent = std::mem::replace(&mut self.local, fresh);
        let write = Write { base: self.version, changes: to_wire(&sent, &self.base) };
        self.sent = Some(sent);
        Some(write)
    }

    /// The write made on `base` is on disk as `version`. An answer for a
    /// write already folded back is ignored.
    pub fn saved(&mut self, base: u64, version: u64) {
        let Some(sent) = &self.sent else { return };
        if base != self.version {
            return;
        }
        sent.apply_in_place(&mut self.base);
        self.version = version;
        self.sent = None;
    }

    /// The write made on `base` failed for `reason`: it is typed again, to
    /// go with the next. A refusal with no reason followed a change the
    /// buffer has already taken, which took the write back with it.
    pub fn refused(&mut self, base: u64, reason: Option<&str>) {
        if reason.is_none() || base != self.version {
            return;
        }
        if let Some(sent) = self.sent.take() {
            self.local = sent.compose(&self.local);
        }
    }

    /// The connection went, and the write in flight with it.
    pub fn lost(&mut self) {
        self.local = self.mine();
        self.sent = None;
    }

    /// The disk's text, held clash and all.
    fn disk(&self) -> Rope {
        match &self.held {
            Some(held) => held.apply(&self.base),
            None => self.base.clone(),
        }
    }

    /// The disk changed by `changes` from `from` to `version`: the change
    /// for the buffer, or `None` when it was not made on the version held
    /// and the file has to be opened again. Changes that do not fit the
    /// text are refused and leave everything as it was.
    pub fn changed(&mut self, from: u64, version: u64, changes: &[TextChange]) -> Result<Option<ChangeSet>, Refused> {
        if from != self.version {
            return Ok(None);
        }
        let step = from_wire(changes, &self.disk())?;
        self.version = version;
        Ok(Some(self.fold(self.stacked(step))))
    }

    /// The file opened again, as `text` at `version`: whatever it became
    /// while the connection was down is folded in as one change.
    pub fn reopened(&mut self, text: &str, version: u64) -> ChangeSet {
        self.lost();
        let disk = self.disk();
        let step = match between(&disk.to_string(), &lf(text)) {
            Some(change) => ChangeSet::of(&[change], disk.len()).expect("a change between two texts fits the first"),
            None => ChangeSet::empty(disk.len()),
        };
        self.version = version;
        self.fold(self.stacked(step))
    }

    /// `step`, made on the disk's text, as a set on `base`.
    fn stacked(&self, step: ChangeSet) -> ChangeSet {
        match &self.held {
            Some(held) => held.compose(&step),
            None => step,
        }
    }

    /// Takes `theirs`, the disk's changes on `base`, into the buffer: the
    /// ones clear of the person's at once, the rest held.
    fn fold(&mut self, theirs: ChangeSet) -> ChangeSet {
        let mine = self.mine();
        self.sent = None;
        let (clear, clashing) = split(&theirs, &mine);
        let shown = clear.map(&mine, true);
        let next = clear.apply(&self.base);
        let old = std::mem::replace(&mut self.base, next);
        self.local = mine.map(&clear, false);
        self.held = if clashing.is_empty() { None } else { Some(clear.invert(&old).compose(&theirs)) };
        shown
    }

    /// Where the held changes fall in the buffer, and what each would put
    /// there.
    pub fn clashes(&self) -> Vec<Clash> {
        let Some(held) = &self.held else { return Vec::new() };
        held.changes()
            .into_iter()
            .map(|c| Clash {
                from: self.local.map_pos(c.from, -1),
                to: self.local.map_pos(c.to, 1),
                theirs: c.insert,
            })
            .collect()
    }

    /// The person's word on a held clash. Keeping theirs gives up the
    /// person's changes that clash and takes the disk's whole; keeping mine
    /// writes the buffer as it stands over the disk's. The change for the
    /// buffer, or `None` when it stays as it is.
    pub fn resolve(&mut self, keep: Keep) -> Option<ChangeSet> {
        let held = self.held.take()?;
        let mine = self.local.clone();
        let disk = held.apply(&self.base);
        match keep {
            Keep::Mine => {
                self.local = held.invert(&self.base).compose(&mine);
                self.base = disk;
                None
            }
            Keep::Theirs => {
                let (kept, _) = split(&mine, &held);
                let rebased = kept.map(&held, false);
                let shown = mine.invert(&self.base).compose(&held).compose(&rebased);
                self.local = rebased;
                self.base = disk;
                Some(shown)
            }
        }
    }
}

#[cfg(test)]
mod tests;
