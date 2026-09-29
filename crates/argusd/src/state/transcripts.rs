//! Which file a pane's conversation is read from, and the clients following
//! it.
//!
//! The harness keeps the transcript; the daemon only reads it. A pane's own
//! agent names its file on the reports its hooks already make, and while
//! any client watches the pane one task follows the file's end and hands
//! each new line to them. Nothing is stored: a client that falls behind, or
//! arrives late, is sent the file's tail again.
//!
//! The end of a file is followed by polling its length rather than by a
//! filesystem watcher. A harness appends many small writes per turn, each of
//! which a watcher would report, only for the length to be read anyway; a
//! poll of one file's length a few times a second, for only the panes
//! someone is watching, costs less and behaves the same on every platform.
//!
//! A tail and the stream that continues it may overlap, never leave a gap:
//! following starts no later than any tail a watcher is sent ends, and an
//! entry read twice replaces itself.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use argus_protocol::{Body, Earlier, Entry, Update};

use super::*;
use crate::harness::transcript::Dialect;

/// How much of a file's end a fresh tail reads, and how much each request
/// for earlier history reads. Big enough for a turn with a few large tool
/// results, small enough to answer at once. Tests page through a few lines
/// rather than megabytes.
const WINDOW: u64 = if cfg!(test) { 1024 } else { 1024 * 1024 };

/// How often a watched file's length is checked.
const POLL: Duration = Duration::from_millis(250);

/// The most one poll reads, so a file that grew by gigabytes between two
/// looks is caught up with over several polls instead of in one read.
const MAX_READ: u64 = 16 * 1024 * 1024;

/// How far back from a file's end following looks for the start of a line,
/// in steps, before giving up and starting at the end.
const LINE_SEARCH: u64 = 64 * 1024;

/// A pane's followers: the channel its updates go out on, how many
/// connections hold it, and the task reading the file while any do.
pub(super) struct Feed {
    tx: broadcast::Sender<ServerMsg>,
    watchers: usize,
    tail: tokio::task::JoinHandle<()>,
}

/// Every file a pane's conversation has lived in, oldest first, and how to
/// read them.
struct History {
    files: Vec<PathBuf>,
    dialect: Dialect,
}

/// Where following a pane has got to: which of its files, and the byte the
/// next unread line starts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Position {
    file: u32,
    offset: u64,
}

/// A stretch of a pane's conversation read into updates, where it ends, and
/// where to ask for what came before it.
struct Page {
    earlier: Option<Earlier>,
    updates: Vec<Update>,
    end: u64,
}

/// Some complete lines of a file, each with the byte it starts at, and the
/// byte after the last of them.
struct Lines {
    lines: Vec<(u64, String)>,
    end: u64,
}

impl Daemon {
    /// Records the file a pane's conversation is written to, as a report
    /// from its agent named it. Only the pane's own agent may: a CLI started
    /// inside it inherits the hook environment, and its conversation is not
    /// the pane's. A file the pane is not already on starts a new part of
    /// the conversation, below everything before it.
    pub(crate) fn report_transcript(&self, pane: PaneId, reporter: Option<&str>, raw: &str) {
        if self.child_of(pane, reporter).is_some() {
            return;
        }
        let path = PathBuf::from(raw.trim());
        if !path.is_absolute() {
            return;
        }
        let first = {
            let mut inner = self.inner.lock().unwrap();
            let Some(p) = find_pane(&mut inner.projects, pane) else {
                return;
            };
            // An exited pane still takes it: a hook can race its process's
            // exit, and what a finished agent did is still worth reading.
            if p.harness.as_deref().and_then(|h| self.dialect_of(h)).is_none() {
                return;
            }
            if p.transcripts.last() == Some(&path) {
                return;
            }
            p.transcripts.push(path);
            p.transcripts.len() == 1
        };
        // The one change a client can see without watching: the pane now
        // has a conversation to offer.
        if first {
            self.broadcast_tree();
        }
    }

    fn dialect_of(&self, harness: &str) -> Option<Dialect> {
        self.harnesses
            .iter()
            .find(|h| h.name == harness)
            .and_then(|h| h.transcript)
    }

    fn history(&self, pane: PaneId) -> Option<History> {
        let inner = self.inner.lock().unwrap();
        let p = find_pane_ref(&inner.projects, pane)?;
        let dialect = self.dialect_of(p.harness.as_deref()?)?;
        (!p.transcripts.is_empty()).then(|| History {
            files: p.transcripts.clone(),
            dialect,
        })
    }

    /// Joins a pane's followers, starting the task that reads its file if
    /// nobody was following it. What the file already holds is
    /// [`Daemon::transcript_tail`]'s to send; this is everything after.
    ///
    /// Where a new reader starts is settled here, before any tail can be
    /// read, so that no tail can end before it.
    pub fn watch_transcript(self: &Arc<Self>, pane: PaneId) -> broadcast::Receiver<ServerMsg> {
        let mut feeds = self.transcripts.lock().unwrap();
        if let Some(feed) = feeds.get_mut(&pane) {
            feed.watchers += 1;
            return feed.tx.subscribe();
        }
        let start = self.history(pane).map(|history| {
            let file = history.files.len() - 1;
            Position {
                file: file as u32,
                offset: last_line_end(&history.files[file]),
            }
        });
        let (tx, rx) = broadcast::channel(64);
        let tail = tokio::spawn(follow(self.clone(), pane, tx.clone(), start));
        feeds.insert(
            pane,
            Feed {
                tx,
                watchers: 1,
                tail,
            },
        );
        rx
    }

    /// Leaves a pane's followers, stopping its reader when nobody is left.
    pub fn unwatch_transcript(&self, pane: PaneId) {
        let mut feeds = self.transcripts.lock().unwrap();
        let Some(feed) = feeds.get_mut(&pane) else {
            return;
        };
        feed.watchers = feed.watchers.saturating_sub(1);
        if feed.watchers == 0 {
            if let Some(feed) = feeds.remove(&pane) {
                feed.tail.abort();
            }
        }
    }

    /// Whether anyone is following a pane's conversation.
    #[cfg(test)]
    pub(crate) fn transcript_watched(&self, pane: PaneId) -> bool {
        self.transcripts.lock().unwrap().contains_key(&pane)
    }

    /// Drops a closed pane's followers, which ends every watch of it.
    pub(super) fn forget_transcript(&self, pane: PaneId) {
        if let Some(feed) = self.transcripts.lock().unwrap().remove(&pane) {
            feed.tail.abort();
        }
    }

    /// The end of a pane's conversation as a fresh copy: what a client
    /// starts from, and what one that fell behind starts again from.
    /// Blocking file I/O; call it off the runtime's threads.
    pub fn transcript_tail(&self, pane: PaneId) -> ServerMsg {
        let page = self.history(pane).and_then(|history| history.tail());
        let (earlier, updates) = page.map_or((None, Vec::new()), |p| (p.earlier, p.updates));
        ServerMsg::Transcript {
            pane,
            fresh: true,
            earlier,
            updates,
        }
    }

    /// The part of a pane's conversation before `before`. Blocking file
    /// I/O; call it off the runtime's threads.
    pub fn earlier_transcript(&self, pane: PaneId, before: Earlier) -> ServerMsg {
        let page = self.history(pane).and_then(|history| {
            let path = history.files.get(before.file as usize)?;
            history.page(before.file, path, before.offset)
        });
        let (earlier, updates) = page.map_or((None, Vec::new()), |p| (p.earlier, p.updates));
        ServerMsg::EarlierTranscript {
            pane,
            before,
            earlier,
            updates,
        }
    }
}

/// Follows a pane's file for as long as anyone watches it, sending what
/// each new line says. `start` of `None` means the pane had no file yet, so
/// whatever it names first is sent from its tail, fresh.
async fn follow(
    daemon: Arc<Daemon>,
    pane: PaneId,
    tx: broadcast::Sender<ServerMsg>,
    start: Option<Position>,
) {
    let mut at = start;
    let mut poll = tokio::time::interval(POLL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        poll.tick().await;
        let Some(history) = daemon.history(pane) else {
            continue;
        };
        let step = tokio::task::spawn_blocking(move || history.step(at)).await;
        let Ok(Some((msg, next))) = step else {
            continue;
        };
        at = Some(next);
        if let Some((fresh, earlier, updates)) = msg {
            let _ = tx.send(ServerMsg::Transcript {
                pane,
                fresh,
                earlier,
                updates,
            });
        }
    }
}

/// One poll's message, when there is anything to say: whether it replaces
/// what followers hold, where to ask for earlier history if it does, and
/// the updates.
type Said = Option<(bool, Option<Earlier>, Vec<Update>)>;

impl History {
    fn current(&self) -> (u32, &Path) {
        let file = self.files.len() - 1;
        (file as u32, &self.files[file])
    }

    /// One poll of the pane's current file from `at`: what it says, and
    /// where the next poll starts.
    ///
    /// Following with nowhere to start from yet sends the file's tail
    /// fresh. The pane having moved to a new file sends where the old
    /// conversation ends, then the new file's tail. A file shorter than
    /// where following had got to was replaced rather than appended to, and
    /// is sent again from its tail, fresh.
    fn step(&self, at: Option<Position>) -> Option<(Said, Position)> {
        let (file, path) = self.current();
        let len = std::fs::metadata(path).ok()?.len();
        // A tail sent from scratch, or — when the pane has moved files —
        // below the divider. A tail reaching the new file's start already
        // opens with it.
        let restart = |fresh: bool| {
            let page = self.page(file, path, len)?;
            let mut updates = page.updates;
            if !fresh && updates.first() != Some(&divider(file)) {
                updates.insert(0, divider(file));
            }
            let earlier = fresh.then_some(page.earlier).flatten();
            Some((Some((fresh, earlier, updates)), Position { file, offset: page.end }))
        };
        let Some(at) = at else {
            return restart(true);
        };
        if at.file != file {
            return restart(false);
        }
        if len < at.offset {
            return restart(true);
        }
        if len == at.offset {
            return Some((None, at));
        }

        let to = len.min(at.offset + MAX_READ);
        let lines = read_lines(path, at.offset, to, false).ok()?;
        // A single line longer than a whole read would never finish; it is
        // given up on rather than read forever.
        let end = if lines.lines.is_empty() && to - at.offset == MAX_READ {
            to
        } else {
            lines.end
        };
        let updates = self.updates_of(file, &lines.lines);
        let said = (!updates.is_empty()).then_some((false, None, updates));
        Some((said, Position { file, offset: end }))
    }

    /// The end of the pane's current file.
    fn tail(&self) -> Option<Page> {
        let (file, path) = self.current();
        let len = std::fs::metadata(path).ok()?.len();
        self.page(file, path, len)
    }

    /// The whole lines in the [`WINDOW`] of `path` that ends at `end`, read
    /// into updates, and where to ask for what came before them. Reaching
    /// the start of a file that is not the pane's first begins the page
    /// with the divider separating it from the file before, which is where
    /// asking goes next.
    fn page(&self, file: u32, path: &Path, end: u64) -> Option<Page> {
        let start = end.saturating_sub(WINDOW);
        let lines = read_lines(path, start, end, start > 0).ok()?;
        let first = lines.lines.first().map_or(lines.end, |(offset, _)| *offset);
        let mut updates = Vec::new();
        if first == 0 && file > 0 {
            updates.push(divider(file));
        }
        updates.extend(self.updates_of(file, &lines.lines));

        let earlier = if first > 0 {
            Some(Earlier { file, offset: first })
        } else if file > 0 {
            let previous = &self.files[file as usize - 1];
            std::fs::metadata(previous).ok().map(|m| Earlier {
                file: file - 1,
                offset: m.len(),
            })
        } else {
            None
        };
        Some(Page {
            earlier,
            updates,
            end: lines.end,
        })
    }

    fn updates_of(&self, file: u32, lines: &[(u64, String)]) -> Vec<Update> {
        lines
            .iter()
            .flat_map(|(offset, line)| self.dialect.read(line, &format!("{file}:{offset}")))
            .collect()
    }
}

/// Where one file of a pane's conversation gives way to the next. Named by
/// the file it opens, so the one a follower is sent live and the one a page
/// of history begins with are the same entry.
fn divider(file: u32) -> Update {
    Update::Upsert(Entry {
        id: format!("{file}:divider"),
        at: None,
        body: Body::Divider {
            text: "New conversation".to_string(),
        },
    })
}

/// The byte after the last complete line of `path`: where following it can
/// start without landing inside a line still being written. Looks back a
/// step at a time; a file with no line end in the last few steps is
/// followed from its end.
fn last_line_end(path: &Path) -> u64 {
    let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else {
        return 0;
    };
    let mut end = len;
    for _ in 0..16 {
        let start = end.saturating_sub(LINE_SEARCH);
        let Ok(bytes) = read_range(path, start, end) else {
            return len;
        };
        if let Some(newline) = bytes.iter().rposition(|&b| b == b'\n') {
            return start + newline as u64 + 1;
        }
        if start == 0 {
            return 0;
        }
        end = start;
    }
    len
}

fn read_range(path: &Path, start: u64, end: u64) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(end.saturating_sub(start) as usize);
    file.take(end.saturating_sub(start)).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The complete lines of `path` between `start` and `end`. `mid_line` says
/// `start` may fall inside a line, whose remainder is skipped; a line still
/// being written at `end` is left for the next read.
fn read_lines(path: &Path, start: u64, end: u64, mid_line: bool) -> std::io::Result<Lines> {
    // One byte early, so a window that happens to start exactly on a line
    // does not lose that line to the skip.
    let from = if mid_line { start - 1 } else { start };
    let bytes = read_range(path, from, end)?;

    let mut at = 0;
    if mid_line {
        match bytes.iter().position(|&b| b == b'\n') {
            Some(newline) => at = newline + 1,
            None => {
                return Ok(Lines {
                    lines: Vec::new(),
                    end: start,
                })
            }
        }
    }
    let mut lines = Vec::new();
    while let Some(len) = bytes[at..].iter().position(|&b| b == b'\n') {
        let line = String::from_utf8_lossy(&bytes[at..at + len]);
        if !line.trim().is_empty() {
            lines.push((from + at as u64, line.into_owned()));
        }
        at += len + 1;
    }
    Ok(Lines {
        lines,
        end: from + at as u64,
    })
}
