//! One file open for editing, as the window holds it: the buffer's sync
//! with the disk, what the bar says of it, and what it asks of the daemon
//! on each word the daemon sends. This is `editor.ts`'s `receive`, `write`
//! and `host` with the view and the timers taken out; whatever draws the
//! buffer feeds it events and does what it answers. A file that was lost
//! with the connection is opened again when the connection says it is
//! back (`Lives::connected`), not when the stage next draws it.

use std::collections::HashMap;

use ropey::Rope;

use super::change::ChangeSet;
use super::edit::{EditEvent, EditGesture};
use super::sync::{Keep, Synced};

/// What the editor's bar says of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Opening,
    Saved,
    Unsaved,
    Saving,
    /// A clash waits on the person.
    Clash,
    /// The last write was refused, for this reason.
    Failed(String),
    /// The daemon dropped the file, for this reason: what is typed stays on
    /// screen and nothing more is written.
    Closed(String),
    /// The connection went; the buffer is kept and stays editable.
    Offline,
}

/// What the host does with an event.
#[derive(Debug, Default)]
pub struct Heard {
    /// The first text of the file: mount the view with it.
    pub mount: Option<Rope>,
    /// The disk's change to show in the buffer, not typed and not undoable,
    /// and lit.
    pub shown: Option<ChangeSet>,
    /// What to say to the daemon.
    pub asks: Vec<EditGesture>,
    /// Whether the buffer is read-only now: set when the file opens, and
    /// again when the daemon drops it.
    pub read_only: Option<bool>,
    /// Try a write again after the retry delay: the last one was refused.
    pub retry: bool,
    /// Whether the stage reads the file again from disk: something was
    /// written.
    pub reread: bool,
}

pub struct Live {
    checkout: u64,
    path: String,
    status: Status,
    sync: Option<Synced>,
    /// Lost with the connection, to be opened again when it is back.
    reopen: bool,
    /// No longer the file on screen: closed once everything typed is written.
    retiring: bool,
}

impl Live {
    /// A file asked for: the gesture that opens it.
    pub fn open(checkout: u64, path: &str) -> (Live, EditGesture) {
        let live = Live {
            checkout,
            path: path.to_owned(),
            status: Status::Opening,
            sync: None,
            reopen: false,
            retiring: false,
        };
        let ask = live.ask_open();
        (live, ask)
    }

    fn ask_open(&self) -> EditGesture {
        EditGesture::Open { checkout: self.checkout, path: self.path.clone() }
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn sync(&self) -> Option<&Synced> {
        self.sync.as_ref()
    }

    pub fn is(&self, checkout: u64, path: &str) -> bool {
        self.checkout == checkout && self.path == path
    }

    fn closed(&self) -> bool {
        matches!(self.status, Status::Closed(_))
    }

    /// What the bar says once everything heard is in.
    fn settled(&self) -> Status {
        match &self.sync {
            None => Status::Opening,
            Some(s) if s.clashing() => Status::Clash,
            Some(s) if s.pending() => Status::Saving,
            Some(_) => Status::Saved,
        }
    }

    /// The person typed `set`, made on the buffer.
    pub fn typed(&mut self, set: &ChangeSet) {
        if let Some(sync) = &mut self.sync {
            sync.typed(set);
        }
        if self.status == Status::Saved {
            self.status = Status::Unsaved;
        }
    }

    /// Writes what is typed, unless a write is in flight, a clash is held,
    /// the file is closed, or the connection is down or the file not yet
    /// opened again: typing then stays with the buffer, unsent, until the
    /// daemon has answered the open.
    pub fn write(&mut self) -> Option<EditGesture> {
        if matches!(self.status, Status::Closed(_) | Status::Offline | Status::Opening) {
            return None;
        }
        let write = self.sync.as_mut()?.take();
        self.status = self.settled();
        write.map(|w| EditGesture::Change {
            checkout: self.checkout,
            path: self.path.clone(),
            base: w.base,
            changes: w.changes,
        })
    }

    /// Something is written and the file is no longer on screen: it can be
    /// closed. A file lost with typing unwritten waits for the connection.
    pub fn done(&self) -> bool {
        let clean = self.sync.as_ref().is_none_or(|s| !s.pending() && !s.clashing());
        self.retiring && (clean || self.closed())
    }

    pub fn close(&self) -> EditGesture {
        EditGesture::Close { checkout: self.checkout, path: self.path.clone() }
    }

    /// The editor shows this file again.
    pub fn shown_again(&mut self) {
        self.retiring = false;
    }

    /// The editor shows another file: what is typed is written, and then
    /// the file is let go (`done`).
    pub fn retire(&mut self) -> Option<EditGesture> {
        self.retiring = true;
        self.write()
    }

    /// The person's word on a held clash: the change for the buffer, and
    /// the write that follows at once.
    pub fn resolve(&mut self, keep: Keep) -> Heard {
        let mut out = Heard::default();
        if let Some(sync) = &mut self.sync {
            out.shown = sync.resolve(keep);
        }
        out.asks.extend(self.write());
        out
    }

    fn write_into(&mut self, out: &mut Heard) {
        out.asks.extend(self.write());
    }

    /// The daemon's word on this file, or `Lost` for the connection.
    pub fn heard(&mut self, event: &EditEvent) -> Heard {
        let mut out = Heard::default();
        match event {
            EditEvent::Lost => self.lost(),
            EditEvent::Opened { version, text, .. } => {
                match &mut self.sync {
                    None => {
                        let sync = Synced::new(text, *version);
                        out.mount = Some(sync.doc());
                        self.sync = Some(sync);
                    }
                    Some(sync) => out.shown = Some(sync.reopened(text, *version)),
                }
                self.reopen = false;
                if !self.closed() {
                    self.status = self.settled();
                }
                out.read_only = Some(false);
                self.write_into(&mut out);
            }
            EditEvent::Changed { from, version, changes, .. } => {
                let Some(sync) = &mut self.sync else { return out };
                // A change that was not made on the version held, or does not
                // fit the text, means this side and the daemon's have parted:
                // the file is opened again and nothing is shown.
                match sync.changed(*from, *version, changes) {
                    Ok(Some(shown)) => {
                        out.shown = Some(shown);
                        self.write_into(&mut out);
                        out.reread = true;
                    }
                    Ok(None) | Err(_) => out.asks.push(self.ask_open()),
                }
            }
            EditEvent::Saved { base, version, .. } => {
                if let Some(sync) = &mut self.sync {
                    sync.saved(*base, *version);
                }
                self.write_into(&mut out);
                out.reread = true;
            }
            EditEvent::Refused { base, reason, .. } => {
                if let Some(sync) = &mut self.sync {
                    sync.refused(*base, reason.as_deref());
                }
                if let Some(reason) = reason {
                    self.status = Status::Failed(reason.clone());
                    out.retry = true;
                }
            }
            EditEvent::Closed { reason, .. } => {
                self.status = Status::Closed(reason.clone());
                self.reopen = false;
                out.read_only = Some(true);
            }
        }
        out
    }

    /// The connection went, and the write in flight with it. A file the
    /// daemon dropped stays dropped.
    fn lost(&mut self) {
        if let Some(sync) = &mut self.sync {
            sync.lost();
        }
        if !self.closed() {
            self.reopen = true;
            self.status = Status::Offline;
        }
    }

    /// The connection is back: the file lost with it is opened again,
    /// showing `Opening`.
    fn connected(&mut self) -> Option<EditGesture> {
        if !self.reopen {
            return None;
        }
        self.reopen = false;
        self.status = Status::Opening;
        Some(self.ask_open())
    }
}

/// Every file open in the window, by checkout and path.
#[derive(Default)]
pub struct Lives {
    all: HashMap<(u64, String), Live>,
}

impl Lives {
    pub fn get(&self, checkout: u64, path: &str) -> Option<&Live> {
        self.all.get(&(checkout, path.to_owned()))
    }

    pub fn get_mut(&mut self, checkout: u64, path: &str) -> Option<&mut Live> {
        self.all.get_mut(&(checkout, path.to_owned()))
    }

    /// The file asked for: its session, made if the window has none, and
    /// the gesture that opens it only then.
    pub fn open(&mut self, checkout: u64, path: &str) -> Option<EditGesture> {
        if let Some(live) = self.get_mut(checkout, path) {
            live.shown_again();
            return None;
        }
        let (live, ask) = Live::open(checkout, path);
        self.all.insert((checkout, path.to_owned()), live);
        Some(ask)
    }

    pub fn remove(&mut self, checkout: u64, path: &str) -> Option<Live> {
        self.all.remove(&(checkout, path.to_owned()))
    }

    pub fn len(&self) -> usize {
        self.all.len()
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    /// The daemon's word, handed to the file it is about; `Lost` goes to
    /// every file. `None` for a word about a file not open.
    pub fn heard(&mut self, event: &EditEvent) -> Vec<(u64, String, Heard)> {
        let (checkout, path) = match event {
            EditEvent::Lost => {
                return self
                    .all
                    .iter_mut()
                    .map(|((c, p), live)| (*c, p.clone(), live.heard(event)))
                    .collect();
            }
            EditEvent::Opened { checkout, path, .. }
            | EditEvent::Changed { checkout, path, .. }
            | EditEvent::Saved { checkout, path, .. }
            | EditEvent::Refused { checkout, path, .. }
            | EditEvent::Closed { checkout, path, .. } => (*checkout, path.clone()),
        };
        match self.all.get_mut(&(checkout, path.clone())) {
            Some(live) => vec![(checkout, path, live.heard(event))],
            None => Vec::new(),
        }
    }

    /// The connection is up again: every file it took is opened again, at
    /// once, whether or not anything draws it.
    pub fn connected(&mut self) -> Vec<EditGesture> {
        self.all.values_mut().filter_map(Live::connected).collect()
    }
}

#[cfg(test)]
mod tests;
