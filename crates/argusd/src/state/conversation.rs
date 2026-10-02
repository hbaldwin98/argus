//! A pane's conversation as a client is shown it — one read model over the
//! files its harness writes, the entries a harness pushed and the draft of
//! what the agent is writing now — and the clients following it.
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
//!
//! A harness whose conversation is no file Argus can read — opencode keeps
//! it in a database — pushes entries instead. Those are held in memory, as
//! a bounded tail, since there is nothing to read them back from; the
//! plugin replays its whole session whenever it starts, which covers a
//! daemon that restarted.
//!
//! The conversation lives on its pane, under the tree's lock, so its
//! followers end when the pane leaves the tree, whichever way it goes. Its
//! files are never read under that lock: what reading them needs is copied
//! out first.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use argus_protocol::{Body, Draft, Earlier, Entry, Update};

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

/// How many pushed entries a pane keeps: the conversation's recent end,
/// which is what a phone opens on.
const MAX_PUSHED: usize = 500;

/// One pane's conversation: what it is read from, what it holds, and who
/// follows it. Written when a file is named, when entries are pushed and
/// when the draft changes; read as a tail, a page before it, or followed.
pub(super) struct Conversation {
    pane: PaneId,
    /// Every file the conversation has been written to, as its own agent's
    /// hooks named them. A conversation that starts over moves to a new
    /// file, and the old one stays readable above it for as long as the
    /// daemon remembers the pane. Not persisted: a restored agent names its
    /// file again on its first hook.
    history: Option<History>,
    /// What a harness whose transcript is no file pushed of it.
    pushed: Option<Pushed>,
    /// What the agent is writing right now, as the tee reads it off its
    /// reply: reasoning or not, and the text so far.
    draft: Option<(bool, String)>,
    followers: Option<Followers>,
}

impl Conversation {
    pub(super) fn new(pane: PaneId) -> Self {
        Self {
            pane,
            history: None,
            pushed: None,
            draft: None,
            followers: None,
        }
    }

    /// Whether there is a conversation to offer: a file named, or entries
    /// pushed. A draft alone is not one; it is only ever part of a reply.
    pub(super) fn offered(&self) -> bool {
        self.history.is_some() || self.pushed.is_some()
    }

    /// Takes the file the conversation is written to now. A file it is not
    /// already on starts a new part of it, below everything before. Says
    /// whether this is what first made the conversation offered.
    pub(super) fn name_file(&mut self, path: PathBuf, dialect: Dialect) -> bool {
        let was_offered = self.offered();
        let history = self.history.get_or_insert_with(|| History {
            files: Vec::new(),
            dialect,
        });
        if history.files.last() != Some(&path) {
            history.files.push(path);
        }
        !was_offered
    }

    /// Takes entries a harness pushed, and hands them to the followers.
    /// `fresh` replaces everything pushed before. Says whether this is what
    /// first made the conversation offered.
    pub(super) fn push(&mut self, fresh: bool, updates: Vec<Update>) -> bool {
        let was_offered = self.offered();
        let pushed = self.pushed.get_or_insert_with(Pushed::default);
        if fresh {
            *pushed = Pushed::default();
        }
        for update in &updates {
            pushed.apply(update);
        }
        self.tell(fresh, updates);
        !was_offered
    }

    /// Applies draft changes, and hands them to the followers. The draft in
    /// progress is held, so a follower arriving mid-reply is sent what has
    /// been written so far.
    pub(super) fn draft(&mut self, changes: Vec<Draft>) {
        if changes.is_empty() {
            return;
        }
        for change in &changes {
            match change {
                Draft::Start { thinking } => self.draft = Some((*thinking, String::new())),
                Draft::More { text } => {
                    if let Some((_, held)) = &mut self.draft {
                        held.push_str(text);
                    }
                }
                Draft::Done => self.draft = None,
            }
        }
        self.tell(false, changes.into_iter().map(Update::Draft).collect());
    }

    /// Hands the followers, if there are any, what a push or a draft
    /// brings, which no file will.
    fn tell(&self, fresh: bool, updates: Vec<Update>) {
        if let Some(followers) = &self.followers {
            let _ = followers.tx.send(ServerMsg::Transcript {
                pane: self.pane,
                fresh,
                earlier: None,
                updates,
            });
        }
    }

    /// What a fresh tail is made of, copied out so that its files can be
    /// read without the tree's lock.
    pub(super) fn tail(&self) -> Tail {
        let draft = match &self.draft {
            Some((thinking, text)) => vec![
                Update::Draft(Draft::Start { thinking: *thinking }),
                Update::Draft(Draft::More { text: text.clone() }),
            ],
            None => Vec::new(),
        };
        Tail {
            history: self.history.clone(),
            pushed: self.pushed.as_ref().map_or_else(Vec::new, Pushed::tail),
            draft,
        }
    }

    /// The files, for paging back through and for following.
    fn history(&self) -> Option<History> {
        self.history.clone()
    }

    /// Joins the followers, if anyone is following.
    fn join(&mut self) -> Option<broadcast::Receiver<ServerMsg>> {
        let followers = self.followers.as_mut()?;
        followers.watchers += 1;
        Some(followers.tx.subscribe())
    }

    /// Joins the followers, starting them if nobody was following: a task
    /// reading the files from `start`, which finds them through `files` on
    /// every poll since more may be named while it runs.
    pub(super) fn follow<F>(
        &mut self,
        start: Option<Position>,
        files: F,
    ) -> broadcast::Receiver<ServerMsg>
    where
        F: Fn() -> Option<History> + Send + 'static,
    {
        if let Some(rx) = self.join() {
            return rx;
        }
        let (tx, rx) = broadcast::channel(64);
        let task = tokio::spawn(follow(self.pane, tx.clone(), start, files));
        self.followers = Some(Followers {
            tx,
            watchers: 1,
            task,
        });
        rx
    }

    /// Leaves the followers, stopping them when nobody is left.
    pub(super) fn leave(&mut self) {
        let Some(followers) = &mut self.followers else {
            return;
        };
        followers.watchers = followers.watchers.saturating_sub(1);
        if followers.watchers == 0 {
            self.followers = None;
        }
    }

    #[cfg(test)]
    pub(super) fn followed(&self) -> bool {
        self.followers.is_some()
    }
}

/// A pushed conversation's latest entries, in order.
#[derive(Default)]
struct Pushed {
    order: std::collections::VecDeque<String>,
    entries: HashMap<String, Entry>,
}

impl Pushed {
    fn apply(&mut self, update: &Update) {
        match update {
            Update::Upsert(entry) => {
                if self.entries.insert(entry.id.clone(), entry.clone()).is_none() {
                    self.order.push_back(entry.id.clone());
                }
                while self.order.len() > MAX_PUSHED {
                    if let Some(oldest) = self.order.pop_front() {
                        self.entries.remove(&oldest);
                    }
                }
            }
            Update::AppendText { id, delta } => {
                if let Some(entry) = self.entries.get_mut(id) {
                    if let Body::Prompt { text } | Body::Reply { text } | Body::Thinking { text } =
                        &mut entry.body
                    {
                        text.push_str(delta);
                    }
                }
            }
            Update::ToolState { id, state } => {
                if let Some(Entry {
                    body: Body::ToolCall { state: held, .. },
                    ..
                }) = self.entries.get_mut(id)
                {
                    *held = *state;
                }
            }
            // A draft is what is being written now; nothing keeps it.
            Update::Draft(_) => {}
        }
    }

    fn tail(&self) -> Vec<Update> {
        self.order
            .iter()
            .filter_map(|id| self.entries.get(id))
            .map(|entry| Update::Upsert(entry.clone()))
            .collect()
    }
}

/// A conversation's followers: the channel its updates go out on, how many
/// connections hold it, and the task reading its files while any do.
/// Dropping them stops the task, which is how a pane leaving the tree ends
/// every watch of it.
struct Followers {
    tx: broadcast::Sender<ServerMsg>,
    watchers: usize,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Followers {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A conversation's tail as copied out of the tree, still to be read.
#[derive(Default)]
pub(super) struct Tail {
    history: Option<History>,
    pushed: Vec<Update>,
    draft: Vec<Update>,
}

impl Tail {
    /// The tail's updates, and where to ask for what came before them.
    /// Blocking file I/O; call it off the runtime's threads.
    ///
    /// A pane can have both a file and pushed entries — Codex's live
    /// channel pushes its items as they happen, and its approvals, while
    /// the rollout file records the items afterwards. The file's entries
    /// come first, and a pushed one only where the file has none by its id,
    /// so an item read both ways is one entry and a question nothing wrote
    /// down is still shown. The draft comes last, for a watcher arriving
    /// mid-reply: it is what is being written after everything else.
    pub(super) fn read(self) -> (Option<Earlier>, Vec<Update>) {
        let page = self.history.as_ref().and_then(History::tail);
        let (earlier, mut updates) = page.map_or((None, Vec::new()), |p| (p.earlier, p.updates));
        let read: std::collections::HashSet<String> = updates
            .iter()
            .filter_map(|u| match u {
                Update::Upsert(entry) => Some(entry.id.clone()),
                _ => None,
            })
            .collect();
        updates.extend(
            self.pushed
                .into_iter()
                .filter(|u| !matches!(u, Update::Upsert(entry) if read.contains(&entry.id))),
        );
        updates.extend(self.draft);
        (earlier, updates)
    }
}

/// Every file a pane's conversation has lived in, oldest first, and how to
/// read them.
#[derive(Clone)]
pub(super) struct History {
    files: Vec<PathBuf>,
    dialect: Dialect,
}

/// Where following a pane has got to: which of its files, and the byte the
/// next unread line starts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Position {
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
    /// the pane's.
    pub(crate) fn report_transcript(&self, pane: PaneId, reporter: Option<&str>, raw: &str) {
        if self.child_of(pane, reporter).is_some() {
            return;
        }
        let path = PathBuf::from(raw.trim());
        if !path.is_absolute() {
            return;
        }
        let offered = {
            let mut inner = self.inner.lock().unwrap();
            let Some(p) = find_pane(&mut inner.projects, pane) else {
                return;
            };
            // An exited pane still takes it: a hook can race its process's
            // exit, and what a finished agent did is still worth reading.
            let Some(dialect) = p.harness.as_deref().and_then(|h| self.dialect_of(h)) else {
                return;
            };
            p.conversation.name_file(path, dialect)
        };
        // The one change a client can see without watching: the pane now
        // has a conversation to offer.
        if offered {
            self.broadcast_tree();
        }
    }

    /// Takes entries a harness pushed, from the pane's own agent only.
    pub(super) fn report_pushed(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
        push: argus_protocol::Push,
    ) {
        if self.child_of(pane, reporter).is_some() {
            return;
        }
        let updates: Vec<Update> = push
            .updates
            .into_iter()
            .map(argus_protocol::transcript::clipped)
            .collect();
        let offered = {
            let mut inner = self.inner.lock().unwrap();
            let Some(p) = find_pane(&mut inner.projects, pane) else {
                return;
            };
            p.conversation.push(push.fresh, updates)
        };
        if offered {
            self.broadcast_tree();
        }
    }

    /// Takes what a plugin inside the agent saw of its reply as it
    /// streamed. Only the pane's own session drafts the pane's reply, and a
    /// pane on the tee has its reply read off the wire already, which would
    /// otherwise be drafted twice.
    pub(super) fn report_draft(&self, pane: PaneId, reporter: Option<&str>, changes: Vec<Draft>) {
        if self.child_of(pane, reporter).is_some()
            || self.tee_upstreams.lock().unwrap().contains_key(&pane)
        {
            return;
        }
        self.draft(pane, changes);
    }

    /// Takes what the tee read off a pane's reply as it streamed.
    pub(super) fn draft(&self, pane: PaneId, changes: Vec<Draft>) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(p) = find_pane(&mut inner.projects, pane) {
            p.conversation.draft(changes);
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
        find_pane_ref(&inner.projects, pane)?.conversation.history()
    }

    /// Joins a pane's followers, starting them if nobody was following it.
    /// What the files already hold is [`Daemon::transcript_tail`]'s to send;
    /// this is everything after. A pane that is gone has no followers, and
    /// its watch ends at once.
    ///
    /// Where a new reader starts is settled here, before any tail can be
    /// read, so that no tail can end before it. Settling it reads the file,
    /// so it is done outside the tree's lock, and anyone who started
    /// following meanwhile is joined instead.
    pub fn watch_transcript(self: &Arc<Self>, pane: PaneId) -> broadcast::Receiver<ServerMsg> {
        let history = {
            let mut inner = self.inner.lock().unwrap();
            let Some(p) = find_pane(&mut inner.projects, pane) else {
                return ended();
            };
            if let Some(rx) = p.conversation.join() {
                return rx;
            }
            p.conversation.history()
        };
        let start = history.map(|history| history.end());

        // Weak, so a follower does not keep the daemon it reads alive.
        let daemon = Arc::downgrade(self);
        let mut inner = self.inner.lock().unwrap();
        let Some(p) = find_pane(&mut inner.projects, pane) else {
            return ended();
        };
        p.conversation.follow(start, move || daemon.upgrade()?.history(pane))
    }

    /// Leaves a pane's followers, stopping its reader when nobody is left.
    pub fn unwatch_transcript(&self, pane: PaneId) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(p) = find_pane(&mut inner.projects, pane) {
            p.conversation.leave();
        }
    }

    /// Whether anyone is following a pane's conversation.
    #[cfg(test)]
    pub(crate) fn transcript_watched(&self, pane: PaneId) -> bool {
        let inner = self.inner.lock().unwrap();
        find_pane_ref(&inner.projects, pane).is_some_and(|p| p.conversation.followed())
    }

    /// The end of a pane's conversation as a fresh copy: what a client
    /// starts from, and what one that fell behind starts again from.
    /// Blocking file I/O; call it off the runtime's threads.
    pub fn transcript_tail(&self, pane: PaneId) -> ServerMsg {
        let tail = {
            let inner = self.inner.lock().unwrap();
            let tail = find_pane_ref(&inner.projects, pane).map(|p| p.conversation.tail());
            tail
        };
        let (earlier, updates) = tail.unwrap_or_default().read();
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
        let (earlier, updates) = self
            .history(pane)
            .map_or((None, Vec::new()), |history| history.earlier(before));
        ServerMsg::EarlierTranscript {
            pane,
            before,
            earlier,
            updates,
        }
    }
}

/// A watch of nothing: its sender is gone before it starts, so it reads as
/// ended.
fn ended() -> broadcast::Receiver<ServerMsg> {
    broadcast::channel(1).1
}

/// Follows a pane's files for as long as anyone watches it, sending what
/// each new line says. `start` of `None` means the pane had no file yet, so
/// whatever it names first is sent from its tail, fresh.
async fn follow<F>(
    pane: PaneId,
    tx: broadcast::Sender<ServerMsg>,
    start: Option<Position>,
    files: F,
) where
    F: Fn() -> Option<History>,
{
    let mut at = start;
    let mut poll = tokio::time::interval(POLL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        poll.tick().await;
        let Some(history) = files() else {
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

    /// Where following starts: after the last complete line of the current
    /// file.
    fn end(&self) -> Position {
        let (file, path) = self.current();
        Position {
            file,
            offset: last_line_end(path),
        }
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

    /// The part of the conversation before `before`, and where to ask for
    /// what came before that.
    fn earlier(&self, before: Earlier) -> (Option<Earlier>, Vec<Update>) {
        let page = self
            .files
            .get(before.file as usize)
            .and_then(|path| self.page(before.file, path, before.offset));
        page.map_or((None, Vec::new()), |p| (p.earlier, p.updates))
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
