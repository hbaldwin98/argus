//! What the recent-sessions dialog holds — whose sessions it lists, the
//! search and the harness and checkout filters over them, the session
//! under the cursor and whether its full transcript is open in place of
//! the list — what it draws and offers of them, and what the page last
//! heard of it.
//!
//! Live sessions are listed first and go to their pane. An ended one
//! resumes in a new pane by the command the footer names, unless its
//! worktree is gone or its harness cannot resume one by id: then its
//! transcript is all there is to open. A session whose harness keeps no
//! transcript, or whose transcript is missing, is listed and previewed but
//! has nothing to open. The sessions, what they said and the resume
//! commands are the daemon's, held by `sessions`.

use argus_protocol::sessions::Session;
use argus_protocol::transcript::Conversation;
use argus_protocol::{CheckoutId, HeldTree, PaneId, ProjectInfo, RepositoryId};
use serde::Serialize;

use crate::keymap::Action;
use crate::page::{Event, ReaderPage, Resumed, TranscriptLine};
use crate::sent::Sent;
use crate::sessions::{self, Sessions};
use crate::transcript_reader::{self, TranscriptReader};

/// The recent-sessions dialog: one repository's agent sessions, live ones
/// first, and what the one under the cursor said.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionsPage {
    pub open: bool,
    pub repository: String,
    /// What the search field says; the rows are the sessions it matches.
    pub search: String,
    /// The harnesses and the checkouts the listing holds, for the menus,
    /// and what each menu is set to; `None` is all.
    pub harnesses: Vec<String>,
    pub harness: Option<String>,
    pub checkouts: Vec<String>,
    pub checkout: Option<String>,
    pub loading: bool,
    /// Why there are no rows: none yet, none matching, or a daemon that
    /// cannot list them.
    pub empty: Option<String>,
    pub rows: Vec<SessionRowView>,
    /// `None` while the full transcript is open, which draws it instead.
    pub preview: Option<SessionPreview>,
    /// The full transcript, read in place of the list.
    pub reader: Option<ReaderPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionRowView {
    /// What a click on it sends back.
    pub row: usize,
    pub title: String,
    pub harness: String,
    /// Its checkout, or the removed worktree it ran in.
    pub checkout: String,
    /// Running in a pane now.
    pub live: bool,
    pub turns: Option<u32>,
    /// How long since its transcript was written: `2h`, or `—`.
    pub ago: String,
    /// It ran in a worktree since removed: drawn dimmed.
    pub removed: bool,
    pub on: bool,
}

/// The session under the cursor: what it said, and what can be done with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionPreview {
    pub title: String,
    pub harness: String,
    pub checkout: String,
    pub session_id: String,
    /// When its transcript starts, as the harness wrote it.
    pub started: Option<String>,
    /// What it said has been asked for and not yet read.
    pub loading: bool,
    /// Said in place of the transcript: none kept, or one gone missing.
    pub notice: Option<String>,
    /// Oldest first.
    pub entries: Vec<TranscriptLine>,
    /// How many older entries the daemon left out.
    pub earlier: u32,
    /// `go` to its pane, `resume` in a new one, or only its `transcript`.
    #[cfg_attr(test, ts(type = "\"go\" | \"resume\" | \"transcript\""))]
    pub offer: &'static str,
    /// The exact command a resume runs, as the footer names it.
    pub command: Option<String>,
    /// Why it cannot be resumed, when it offers only its transcript.
    pub why: Option<String>,
    /// Its full transcript can be opened: not one its harness keeps none
    /// of, or one gone missing.
    pub readable: bool,
}

pub struct SessionsDialog {
    repository: RepositoryId,
    name: String,
    /// The daemon cannot list sessions: the dialog says so rather than
    /// waiting on an answer that will not come.
    unsupported: bool,
    /// The session under the cursor, by id, so the cursor stays on it
    /// however a filter moves it in the list.
    on: Option<String>,
    /// What the search field says: the sessions shown are those whose
    /// title, harness, checkout or id holds every word of it.
    search: String,
    harness: Option<String>,
    /// A checkout's name, or a removed worktree's, as the rows say it.
    checkout: Option<String>,
    /// The full transcript of the session under the cursor, open in place
    /// of the list, which holds the keys until it closes.
    reader: Option<TranscriptReader>,
}

/// What a gesture in the dialog comes to.
#[derive(Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    /// Go to the pane the session is running in.
    GoTo(PaneId),
    /// Resume the session in a new agent pane, which will say it was
    /// resumed `from`.
    Resume { checkout: CheckoutId, harness: String, session_id: String, from: Resumed },
    /// Put the session's id on the clipboard.
    Copy(String),
    /// Why the key did nothing, for the person to hear.
    Say(String),
}

/// What the session under the cursor offers.
enum Offer {
    Go(PaneId),
    Resume(Vec<String>),
    /// Only its transcript, and why it cannot be resumed.
    Read(String),
}

impl SessionsDialog {
    /// `unsupported` when the daemon was not asked, since it cannot say.
    pub fn new(repository: RepositoryId, name: String, unsupported: bool) -> SessionsDialog {
        SessionsDialog {
            repository,
            name,
            unsupported,
            on: None,
            search: String::new(),
            harness: None,
            checkout: None,
            reader: None,
        }
    }

    pub fn repository(&self) -> RepositoryId {
        self.repository
    }

    /// Another daemon answered, and was asked again unless it cannot say.
    /// Its listing may not hold the session being read, so the list is
    /// shown again.
    pub fn reasked(&mut self, asked: bool) {
        self.unsupported = !asked;
        self.reader = None;
    }

    fn listed<'a>(&self, sessions: &'a Sessions) -> &'a [Session] {
        sessions.listed(self.repository).unwrap_or(&[])
    }

    /// The listing's indices, live sessions first and each part newest
    /// first, as the daemon ordered them.
    fn ordered(listed: &[Session]) -> Vec<usize> {
        let mut order: Vec<usize> = (0..listed.len()).collect();
        order.sort_by_key(|&i| listed[i].pane.is_none());
        order
    }

    /// The ordered indices the search and the filters show.
    fn shown(&self, listed: &[Session], tree: &[ProjectInfo]) -> Vec<usize> {
        let words: Vec<String> = self.search.split_whitespace().map(str::to_lowercase).collect();
        Self::ordered(listed)
            .into_iter()
            .filter(|&i| {
                let session = &listed[i];
                let place = place(session, tree);
                let text = format!(
                    "{} {} {} {}",
                    session.title.as_deref().unwrap_or_default(),
                    session.harness,
                    place,
                    session.session_id
                )
                .to_lowercase();
                self.harness.as_ref().is_none_or(|h| *h == session.harness)
                    && self.checkout.as_ref().is_none_or(|c| *c == place)
                    && words.iter().all(|w| text.contains(w.as_str()))
            })
            .collect()
    }

    /// The listing index under the cursor: its session while shown, else
    /// the first shown.
    fn cursor(&self, listed: &[Session], shown: &[usize]) -> Option<usize> {
        shown
            .iter()
            .copied()
            .find(|&i| Some(&listed[i].session_id) == self.on.as_ref())
            .or_else(|| shown.first().copied())
    }

    /// The session the dialog acts on: the one read whole while it is
    /// open, else the one under the cursor.
    fn selected<'a>(&self, sessions: &'a Sessions, tree: &[ProjectInfo]) -> Option<&'a Session> {
        let listed = self.listed(sessions);
        if let Some(reader) = &self.reader {
            return listed.iter().find(|s| s.session_id == reader.session_id());
        }
        let shown = self.shown(listed, tree);
        self.cursor(listed, &shown).map(|i| &listed[i])
    }

    /// The cursor moves over the sessions shown, and stops at either end.
    pub fn move_by(&mut self, delta: isize, sessions: &Sessions, tree: &[ProjectInfo]) {
        let listed = self.listed(sessions);
        let shown = self.shown(listed, tree);
        let Some(at) = self.cursor(listed, &shown).and_then(|c| shown.iter().position(|&i| i == c)) else {
            return;
        };
        let to = at.saturating_add_signed(delta).min(shown.len() - 1);
        self.on = Some(listed[shown[to]].session_id.clone());
    }

    pub fn ends(&mut self, bottom: bool, sessions: &Sessions, tree: &[ProjectInfo]) {
        let listed = self.listed(sessions);
        let shown = self.shown(listed, tree);
        let row = if bottom { shown.last() } else { shown.first() };
        if let Some(&row) = row {
            self.on = Some(listed[row].session_id.clone());
        }
    }

    /// A click on a row; one the filters hide is not there to click.
    pub fn select_row(&mut self, row: usize, sessions: &Sessions, tree: &[ProjectInfo]) {
        let listed = self.listed(sessions);
        if self.shown(listed, tree).contains(&row) {
            self.on = Some(listed[row].session_id.clone());
        }
    }

    pub fn set_search(&mut self, search: String) {
        self.search = search;
    }

    pub fn set_filters(&mut self, harness: Option<String>, checkout: Option<String>) {
        self.harness = harness;
        self.checkout = checkout;
    }

    /// `h`: the next harness the listing holds, then all of them again.
    pub fn cycle_harness(&mut self, sessions: &Sessions) {
        let options = harnesses(self.listed(sessions));
        self.harness = next(&options, self.harness.as_ref());
    }

    /// `c`: the next checkout the listing holds, then all of them again.
    pub fn cycle_checkout(&mut self, sessions: &Sessions, tree: &[ProjectInfo]) {
        let options = places(self.listed(sessions), tree);
        self.checkout = next(&options, self.checkout.as_ref());
    }

    pub fn reading(&self) -> bool {
        self.reader.is_some()
    }

    /// `o`: the full transcript of the session under the cursor, in place
    /// of the list, unless there is none to open.
    pub fn open_reader(&mut self, sessions: &Sessions, tree: &[ProjectInfo]) -> Then {
        let Some(session) = self.selected(sessions, tree) else {
            return Then::Nothing;
        };
        if let Some(why) = unreadable(session, sessions.transcript(&session.session_id)) {
            return Then::Say(why);
        }
        self.on = Some(session.session_id.clone());
        self.reader = Some(TranscriptReader::new(session.session_id.clone()));
        Then::Nothing
    }

    /// Escape in the reader: the list again, the cursor where it was.
    pub fn close_reader(&mut self) -> bool {
        self.reader.take().is_some()
    }

    /// A key of the full transcript, or with `row` a click on one of the
    /// person's turns.
    pub fn reader_action(&mut self, action: Action, row: Option<usize>, sessions: &Sessions, tree: &[ProjectInfo]) -> Then {
        let lines = self.reader_lines(sessions);
        let Some(reader) = &mut self.reader else {
            return Then::Nothing;
        };
        match (action, row) {
            (_, Some(turn)) => reader.jump(turn, &lines),
            (Action::Next | Action::Previous, _) => reader.move_turn(if action == Action::Next { 1 } else { -1 }, &lines),
            (Action::First | Action::Last, _) => reader.ends(action == Action::Last, &lines),
            (Action::NextMatch | Action::PreviousMatch, _) => {
                reader.move_match(if action == Action::NextMatch { 1 } else { -1 }, &lines);
            }
            (Action::CopyId, _) => return Then::Copy(reader.session_id().to_owned()),
            (Action::Resume, _) => return self.resume(sessions, tree),
            // Only its pane: the reader is already what Enter opens of an
            // ended session, and resuming one is said by its own key.
            (Action::Choose, _) => {
                return match self.selected(sessions, tree).map(|s| offer(s, sessions, tree)) {
                    Some(Offer::Go(pane)) => Then::GoTo(pane),
                    _ => Then::Nothing,
                };
            }
            _ => {}
        }
        Then::Nothing
    }

    /// What the full transcript's find field says now.
    pub fn find(&mut self, find: String, sessions: &Sessions) {
        let lines = self.reader_lines(sessions);
        if let Some(reader) = &mut self.reader {
            reader.set_find(find, &lines);
        }
    }

    /// What the session read whole said, as the reader draws it.
    fn reader_lines(&self, sessions: &Sessions) -> Vec<TranscriptLine> {
        match self.reader.as_ref().and_then(|r| sessions.transcript(r.session_id())) {
            Some(Conversation::Read(transcript)) => transcript.entries.iter().filter_map(transcript_reader::line).collect(),
            _ => Vec::new(),
        }
    }

    /// The read the session under the cursor wants: what it said, asked
    /// once, while nothing of it is held or on its way.
    pub fn wants_read<'a>(&self, sessions: &'a Sessions, tree: &[ProjectInfo]) -> Option<&'a Session> {
        let session = self.selected(sessions, tree)?;
        let held = sessions.transcript(&session.session_id).is_some() || sessions.reading(&session.session_id);
        (!held).then_some(session)
    }

    /// Enter: a live session's pane, an ended one resumed, and one that
    /// cannot be its transcript whole.
    pub fn choose(&mut self, sessions: &Sessions, tree: &[ProjectInfo]) -> Then {
        let Some(session) = self.selected(sessions, tree) else {
            return Then::Nothing;
        };
        match offer(session, sessions, tree) {
            Offer::Go(pane) => Then::GoTo(pane),
            Offer::Resume(_) => resume(session),
            Offer::Read(_) => self.open_reader(sessions, tree),
        }
    }

    /// `r`: resume the session under the cursor, or say why it cannot be.
    pub fn resume(&self, sessions: &Sessions, tree: &[ProjectInfo]) -> Then {
        let Some(session) = self.selected(sessions, tree) else {
            return Then::Nothing;
        };
        match offer(session, sessions, tree) {
            Offer::Go(_) => Then::Say("it is running: Enter goes to its pane".into()),
            Offer::Resume(_) => resume(session),
            Offer::Read(why) => Then::Say(format!("it cannot be resumed: {why}")),
        }
    }

    /// `now` is seconds since the Unix epoch.
    pub fn page(&self, sessions: &Sessions, tree: &[ProjectInfo], now: u64) -> SessionsPage {
        let mut page = SessionsPage {
            open: true,
            repository: self.name.clone(),
            search: self.search.clone(),
            harness: self.harness.clone(),
            checkout: self.checkout.clone(),
            ..SessionsPage::default()
        };
        if self.unsupported {
            page.empty = Some("This argusd cannot list sessions: update it, and restart it.".into());
            return page;
        }
        let Some(listed) = sessions.listed(self.repository) else {
            page.loading = true;
            return page;
        };
        page.harnesses = harnesses(listed);
        page.checkouts = places(listed, tree);
        let shown = self.shown(listed, tree);
        let cursor = self.cursor(listed, &shown);
        page.rows = shown
            .iter()
            .map(|&i| {
                let session = &listed[i];
                SessionRowView {
                    row: i,
                    title: title(session),
                    harness: session.harness.clone(),
                    checkout: place(session, tree),
                    live: session.pane.is_some(),
                    turns: session.turns,
                    ago: argus_protocol::ago(session.modified.map(|ms| ms / 1000), now),
                    removed: session.removed_worktree.is_some(),
                    on: Some(i) == cursor,
                }
            })
            .collect();
        page.empty = if listed.is_empty() {
            Some(format!("No agent has worked in {} yet.", self.name))
        } else if page.rows.is_empty() {
            Some("No session matches.".into())
        } else {
            None
        };
        let preview = cursor.map(|i| preview(&listed[i], sessions, tree));
        match (self.reader.as_ref().zip(self.selected(sessions, tree)), preview) {
            (Some((reader, session)), Some(preview)) => page.reader = Some(reader.page(session, &self.name, preview)),
            (_, preview) => page.preview = preview,
        }
        page
    }
}

/// The dialog as the window holds it, while open, and what the page last
/// heard of it.
#[derive(Default)]
pub struct Recent {
    dialog: Option<SessionsDialog>,
    sent: Sent<SessionsPage>,
}

impl Recent {
    pub fn open(&mut self, dialog: SessionsDialog) {
        self.dialog = Some(dialog);
    }

    pub fn close(&mut self) {
        self.dialog = None;
    }

    pub fn get(&self) -> Option<&SessionsDialog> {
        self.dialog.as_ref()
    }

    pub fn get_mut(&mut self) -> Option<&mut SessionsDialog> {
        self.dialog.as_mut()
    }

    /// What the page has not heard of the dialog. `now` is seconds since
    /// the Unix epoch.
    pub fn take(&mut self, sessions: &Sessions, tree: &[ProjectInfo], now: u64) -> Option<Event> {
        let page = self.dialog.as_ref().map(|d| d.page(sessions, tree, now));
        self.sent.take(page).map(|page| Event::Sessions(Box::new(page)))
    }

    /// The page reloaded and holds nothing: an open dialog is sent again.
    pub fn reload(&mut self) {
        self.sent.forget();
    }
}

fn resume(session: &Session) -> Then {
    Then::Resume {
        checkout: session.checkout,
        harness: session.harness.clone(),
        session_id: session.session_id.clone(),
        from: sessions::resumed_from(session),
    }
}

/// Why `session` has no full transcript to open, once its read says so: a
/// harness that keeps none lists its sessions but cannot show them.
fn unreadable(session: &Session, read: Option<&Conversation>) -> Option<String> {
    match read? {
        Conversation::NoTranscript => Some(format!("{} keeps no transcript Argus can read", session.harness)),
        Conversation::Missing => Some("its transcript is missing: moved, cleaned up, or never written".into()),
        Conversation::Read(_) => None,
    }
}

fn offer(session: &Session, sessions: &Sessions, tree: &[ProjectInfo]) -> Offer {
    if let Some(pane) = session.pane {
        return Offer::Go(pane);
    }
    // Its checkout is the primary one now, which is not where it ran: a
    // resume there would be a conversation about files that are not there.
    if session.removed_worktree.is_some() {
        return Offer::Read(format!("its worktree {} was removed", place(session, tree)));
    }
    match sessions.resume_command(&session.harness, &session.session_id) {
        Some(argv) => Offer::Resume(argv),
        None => Offer::Read(format!("{} cannot resume a session by its id", session.harness)),
    }
}

fn preview(session: &Session, sessions: &Sessions, tree: &[ProjectInfo]) -> SessionPreview {
    let conversation = sessions.transcript(&session.session_id);
    let (entries, earlier, notice) = match conversation {
        None => (Vec::new(), 0, None),
        Some(Conversation::NoTranscript) => {
            (Vec::new(), 0, Some(format!("{} keeps no transcript Argus can read.", session.harness)))
        }
        Some(Conversation::Missing) => {
            (Vec::new(), 0, Some("Its transcript is missing: moved, cleaned up, or never written.".into()))
        }
        Some(Conversation::Read(transcript)) => {
            let lines: Vec<TranscriptLine> = transcript.entries.iter().filter_map(transcript_reader::line).collect();
            let notice = lines.is_empty().then(|| "Nothing was said in it yet.".to_owned());
            (lines, transcript.earlier, notice)
        }
    };
    let (offer, command, why) = match offer(session, sessions, tree) {
        Offer::Go(_) => ("go", None, None),
        Offer::Resume(argv) => ("resume", Some(shell_words(&argv)), None),
        Offer::Read(why) => ("transcript", None, Some(why)),
    };
    SessionPreview {
        title: title(session),
        harness: session.harness.clone(),
        checkout: place(session, tree),
        session_id: session.session_id.clone(),
        started: session.started.clone(),
        loading: conversation.is_none(),
        notice,
        entries,
        earlier,
        offer,
        command,
        why,
        readable: unreadable(session, conversation).is_none(),
    }
}

fn title(session: &Session) -> String {
    session.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "Untitled session".into())
}

/// Where a session ran, as a row names it: its checkout, or the last part
/// of a removed worktree's path, since the checkout it is listed under is
/// the primary one standing in for it.
fn place(session: &Session, tree: &[ProjectInfo]) -> String {
    if let Some(path) = &session.removed_worktree {
        return path.rsplit(['/', '\\']).find(|part| !part.is_empty()).unwrap_or(path).to_owned();
    }
    tree.checkout(session.checkout).map_or_else(|| "?".into(), |c| c.name.clone())
}

/// Each harness the listing holds, in its order.
fn harnesses(listed: &[Session]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for session in listed {
        if !seen.contains(&session.harness) {
            seen.push(session.harness.clone());
        }
    }
    seen
}

/// Each place the listing holds, in its order.
fn places(listed: &[Session], tree: &[ProjectInfo]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for session in listed {
        let place = place(session, tree);
        if !seen.contains(&place) {
            seen.push(place);
        }
    }
    seen
}

/// The option after `current`, with none after the last.
fn next(options: &[String], current: Option<&String>) -> Option<String> {
    match current.and_then(|c| options.iter().position(|o| o == c)) {
        Some(at) => options.get(at + 1).cloned(),
        None => options.first().cloned(),
    }
}

/// An argv as a person would type it: the daemon runs it without a shell,
/// so this is only for reading, quoting what a shell would split.
fn shell_words(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            if !arg.is_empty() && !arg.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
                arg.clone()
            } else {
                format!("\"{}\"", arg.replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, project, repository};
    use crate::status::StatusBar;
    use argus_protocol::sessions::Resume;
    use argus_protocol::transcript::{Entry, Transcript};
    use argus_protocol::{ClientMsg, Hello, ServerMsg, SESSIONS};

    fn tree() -> Vec<ProjectInfo> {
        vec![project(1, "src", vec![repository(10, "argus", None, vec![
            checkout(100, "main", true, Vec::new()),
            checkout(101, "spike", false, Vec::new()),
        ])])]
    }

    fn session(id: &str, harness: &str, checkout: u64) -> Session {
        Session {
            harness: harness.into(),
            session_id: id.into(),
            checkout: CheckoutId(checkout),
            removed_worktree: None,
            title: Some(format!("{id} work")),
            started: None,
            ended: None,
            modified: Some(1_000_000),
            turns: Some(2),
            pane: None,
        }
    }

    /// The daemon's listing, newest first: an ended claude session, a live
    /// codex one, a session from a removed worktree, and a generic one.
    fn listing() -> Vec<Session> {
        vec![
            session("old", "claude", 100),
            Session { pane: Some(PaneId(7)), ..session("live", "codex", 101) },
            Session { removed_worktree: Some("/src/.worktrees/gone".into()), ..session("gone", "claude", 100) },
            session("plain", "generic", 100),
        ]
    }

    /// Sessions that have heard the listing and the daemon's resumes:
    /// claude's by id, and none for codex or generic.
    fn held(listing: Vec<Session>) -> Sessions {
        let mut status = StatusBar::new(false);
        status.connected(Some(Hello { capabilities: vec![SESSIONS.into()], ..Hello::this_build() }), None);
        let mut sessions = Sessions::default();
        sessions.list(&status, RepositoryId(10), 1);
        sessions.ask_resumes(&status, 2);
        sessions.receive(ServerMsg::Sessions { request_id: 1, repository: RepositoryId(10), sessions: listing });
        let argv = vec!["claude".to_string(), "--model".into(), "opus 4".into(), "--resume".into(), "{session_id}".into()];
        sessions.receive(ServerMsg::Resumes { request_id: 2, resumes: vec![Resume { harness: "claude".into(), argv }] });
        sessions
    }

    fn dialog() -> SessionsDialog {
        SessionsDialog::new(RepositoryId(10), "argus".into(), false)
    }

    fn ids(page: &SessionsPage) -> Vec<String> {
        page.rows.iter().map(|r| r.title.trim_end_matches(" work").to_owned()).collect()
    }

    #[test]
    fn live_sessions_come_first_then_the_rest_newest_first() {
        let sessions = held(listing());
        let page = dialog().page(&sessions, &tree(), 1_000);
        assert_eq!(ids(&page), ["live", "old", "gone", "plain"]);
        assert!(page.rows[0].live && !page.rows[1].live);
        assert!(page.rows[0].on, "the cursor starts on the first");
        assert_eq!((page.rows[1].harness.as_str(), page.rows[1].checkout.as_str()), ("claude", "main"));
        assert_eq!((page.rows[1].turns, page.rows[1].ago.as_str()), (Some(2), "1m"));
        assert_eq!(page.harnesses, ["claude", "codex", "generic"]);
        assert_eq!(page.checkouts, ["main", "spike", "gone"]);
    }

    #[test]
    fn a_session_from_a_removed_worktree_is_dimmed_and_named_after_it() {
        let sessions = held(listing());
        let page = dialog().page(&sessions, &tree(), 1_000);
        let gone = page.rows.iter().find(|r| r.title == "gone work").unwrap();
        assert!(gone.removed);
        assert_eq!(gone.checkout, "gone");
        assert!(page.rows.iter().filter(|r| r.title != "gone work").all(|r| !r.removed));
    }

    #[test]
    fn the_search_and_the_filters_show_only_what_they_match() {
        let sessions = held(listing());
        let mut d = dialog();
        d.set_search("CLAUDE".into());
        assert_eq!(ids(&d.page(&sessions, &tree(), 0)), ["old", "gone"]);
        d.set_search("old main".into());
        assert_eq!(ids(&d.page(&sessions, &tree(), 0)), ["old"]);
        d.set_search(String::new());
        d.cycle_harness(&sessions);
        assert_eq!(d.page(&sessions, &tree(), 0).harness.as_deref(), Some("claude"));
        assert_eq!(ids(&d.page(&sessions, &tree(), 0)), ["old", "gone"]);
        d.cycle_checkout(&sessions, &tree());
        assert_eq!(ids(&d.page(&sessions, &tree(), 0)), ["old"], "claude in main");
        d.set_filters(Some("codex".into()), Some("main".into()));
        assert_eq!(d.page(&sessions, &tree(), 0).empty.as_deref(), Some("No session matches."));
        d.set_filters(None, Some("spike".into()));
        assert_eq!(ids(&d.page(&sessions, &tree(), 0)), ["live"]);
    }

    #[test]
    fn a_filter_cycles_through_what_the_listing_holds_and_back_to_all() {
        let sessions = held(listing());
        let mut d = dialog();
        let mut seen = Vec::new();
        for _ in 0..4 {
            d.cycle_harness(&sessions);
            seen.push(d.page(&sessions, &tree(), 0).harness);
        }
        assert_eq!(seen, [Some("claude".into()), Some("codex".into()), Some("generic".into()), None]);
    }

    #[test]
    fn the_cursor_moves_over_the_shown_sessions_and_stays_on_its_own() {
        let sessions = held(listing());
        let mut d = dialog();
        d.move_by(1, &sessions, &tree());
        d.move_by(1, &sessions, &tree());
        let on = |d: &SessionsDialog| d.page(&sessions, &tree(), 0).rows.into_iter().find(|r| r.on).map(|r| r.title);
        assert_eq!(on(&d).as_deref(), Some("gone work"));
        d.move_by(5, &sessions, &tree());
        assert_eq!(on(&d).as_deref(), Some("plain work"), "held at the last");
        d.set_search("codex".into());
        assert_eq!(on(&d).as_deref(), Some("live work"), "a hidden cursor lights the first shown");
        d.set_search(String::new());
        assert_eq!(on(&d).as_deref(), Some("plain work"), "and comes back to its own");
        d.ends(false, &sessions, &tree());
        assert_eq!(on(&d).as_deref(), Some("live work"));
        d.set_filters(Some("claude".into()), None);
        d.select_row(1, &sessions, &tree());
        assert_eq!(on(&d).as_deref(), Some("old work"), "a click must land on a shown row");
    }

    #[test]
    fn a_live_session_goes_to_its_pane_and_offers_no_resume() {
        let sessions = held(listing());
        let mut d = dialog();
        assert_eq!(d.choose(&sessions, &tree()), Then::GoTo(PaneId(7)));
        let preview = d.page(&sessions, &tree(), 0).preview.unwrap();
        assert_eq!((preview.offer, preview.command), ("go", None));
        assert!(matches!(d.resume(&sessions, &tree()), Then::Say(_)));
    }

    #[test]
    fn an_ended_session_resumes_by_the_exact_command_the_footer_names() {
        let sessions = held(listing());
        let mut d = dialog();
        d.move_by(1, &sessions, &tree());
        let preview = d.page(&sessions, &tree(), 0).preview.unwrap();
        assert_eq!(preview.offer, "resume");
        assert_eq!(preview.command.as_deref(), Some("claude --model \"opus 4\" --resume old"));
        let from = Resumed { at: Some("1970-01-01T00:16:40Z".into()), turns: Some(2) };
        let resumed = Then::Resume { checkout: CheckoutId(100), harness: "claude".into(), session_id: "old".into(), from };
        assert_eq!(d.resume(&sessions, &tree()), resumed);
        assert_eq!(d.choose(&sessions, &tree()), resumed);
    }

    #[test]
    fn a_removed_worktree_or_a_harness_without_resume_id_offers_only_the_transcript() {
        let sessions = held(listing());
        let mut d = dialog();
        for (title, why) in [("gone", "its worktree gone was removed"), ("plain", "generic cannot resume a session by its id")] {
            d.set_search(title.into());
            let preview = d.page(&sessions, &tree(), 0).preview.unwrap();
            assert_eq!((preview.offer, preview.command, preview.why.as_deref()), ("transcript", None, Some(why)));
            assert_eq!(d.resume(&sessions, &tree()), Then::Say(format!("it cannot be resumed: {why}")));
            assert_eq!(d.choose(&sessions, &tree()), Then::Nothing, "Enter opens the transcript whole");
            assert!(d.page(&sessions, &tree(), 0).reader.is_some());
            assert!(d.close_reader());
        }
    }

    #[test]
    fn a_session_whose_harness_resumes_is_not_offered_before_the_daemon_says_how() {
        let mut status = StatusBar::new(false);
        status.connected(Some(Hello { capabilities: vec![SESSIONS.into()], ..Hello::this_build() }), None);
        let mut sessions = Sessions::default();
        sessions.list(&status, RepositoryId(10), 1);
        sessions.receive(ServerMsg::Sessions { request_id: 1, repository: RepositoryId(10), sessions: vec![session("old", "claude", 100)] });
        let d = dialog();
        assert_eq!(d.page(&sessions, &tree(), 0).preview.unwrap().offer, "transcript");
    }

    #[test]
    fn the_transcript_is_read_once_on_demand_and_says_plainly_when_there_is_none() {
        let mut status = StatusBar::new(false);
        status.connected(Some(Hello { capabilities: vec![SESSIONS.into()], ..Hello::this_build() }), None);
        let mut sessions = held(listing());
        let d = dialog();
        let wanted = d.wants_read(&sessions, &tree()).cloned().expect("the cursor's session is read");
        assert_eq!(wanted.session_id, "live");
        assert!(d.page(&sessions, &tree(), 0).preview.unwrap().loading);
        let Some(ClientMsg::ReadSession { request_id, .. }) = sessions.read(&status, &wanted, 5) else {
            panic!("asked");
        };
        assert!(d.wants_read(&sessions, &tree()).is_none(), "asked once");
        let read = Conversation::Read(Transcript {
            entries: vec![
                Entry::User { text: "Fix the build".into(), at: None },
                Entry::Tool { tool: "Bash".into(), summary: "cargo test".into(), at: None, failed: true },
                Entry::Unknown,
                Entry::Agent { text: "Fixed.".into(), at: None },
            ],
            earlier: 3,
            ..Transcript::default()
        });
        sessions.receive(ServerMsg::SessionTranscript { request_id, session_id: "live".into(), conversation: Box::new(read) });
        let preview = d.page(&sessions, &tree(), 0).preview.unwrap();
        assert!(!preview.loading && preview.notice.is_none());
        let kinds: Vec<_> = preview.entries.iter().map(|e| (e.kind, e.text.as_str())).collect();
        assert_eq!(kinds, [("user", "Fix the build"), ("tool", "cargo test"), ("agent", "Fixed.")]);
        assert!(preview.entries[1].failed && preview.entries[1].tool == "Bash");
        assert_eq!(preview.earlier, 3);

        for (conversation, notice) in [
            (Conversation::NoTranscript, "codex keeps no transcript Argus can read."),
            (Conversation::Missing, "Its transcript is missing: moved, cleaned up, or never written."),
        ] {
            sessions.read(&status, &wanted, 6);
            sessions.receive(ServerMsg::SessionTranscript { request_id: 6, session_id: "live".into(), conversation: Box::new(conversation) });
            assert_eq!(d.page(&sessions, &tree(), 0).preview.unwrap().notice.as_deref(), Some(notice));
        }
    }

    #[test]
    fn a_session_with_no_transcript_still_shows_its_title_time_and_checkout_and_says_why_it_is_bare() {
        let mut status = StatusBar::new(false);
        status.connected(Some(Hello { capabilities: vec![SESSIONS.into()], ..Hello::this_build() }), None);
        // What the daemon saw of a generic agent's session, its pane gone.
        let seen = Session {
            started: Some("2026-10-14T09:00:00Z".into()),
            ended: Some("2026-10-14T09:20:00Z".into()),
            turns: None,
            ..session("plain", "generic", 101)
        };
        let mut sessions = held(vec![seen]);
        let d = dialog();
        let wanted = d.wants_read(&sessions, &tree()).cloned().expect("read like any other");
        let Some(ClientMsg::ReadSession { request_id, .. }) = sessions.read(&status, &wanted, 5) else {
            panic!("asked");
        };
        sessions.receive(ServerMsg::SessionTranscript {
            request_id,
            session_id: "plain".into(),
            conversation: Box::new(Conversation::NoTranscript),
        });
        let page = d.page(&sessions, &tree(), 0);
        assert_eq!((page.rows[0].title.as_str(), page.rows[0].checkout.as_str()), ("plain work", "spike"));
        let preview = page.preview.unwrap();
        assert_eq!((preview.title.as_str(), preview.checkout.as_str()), ("plain work", "spike"));
        assert_eq!(preview.started.as_deref(), Some("2026-10-14T09:00:00Z"));
        assert!(preview.entries.is_empty() && !preview.loading);
        assert_eq!(preview.notice.as_deref(), Some("generic keeps no transcript Argus can read."));
    }

    #[test]
    fn a_seen_session_from_a_removed_worktree_is_dimmed_and_says_where_it_ran() {
        // What the daemon lists of a generic agent's session whose worktree
        // was removed after it ran: Argus saw it, no transcript holds it.
        let seen = Session {
            removed_worktree: Some("C:\\src\\.argus\\worktrees\\spike-2".into()),
            turns: None,
            ..session("plain", "generic", 100)
        };
        let sessions = held(vec![seen]);
        let mut d = dialog();
        let page = d.page(&sessions, &tree(), 0);
        assert!(page.rows[0].removed);
        assert_eq!(page.rows[0].checkout, "spike-2");
        let why = "its worktree spike-2 was removed";
        assert_eq!(page.preview.unwrap().why.as_deref(), Some(why), "the worktree, not the harness, is why");
        assert_eq!(d.resume(&sessions, &tree()), Then::Say(format!("it cannot be resumed: {why}")));
        assert_eq!(d.choose(&sessions, &tree()), Then::Nothing);
    }

    /// `held`'s listing with what `id` said read: two turns, each answered.
    fn read(id: &str) -> Sessions {
        let mut status = StatusBar::new(false);
        status.connected(Some(Hello { capabilities: vec![SESSIONS.into()], ..Hello::this_build() }), None);
        let mut sessions = held(listing());
        let wanted = listing().into_iter().find(|s| s.session_id == id).unwrap();
        sessions.read(&status, &wanted, 9);
        let transcript = Transcript {
            entries: vec![
                Entry::User { text: "Fix the build".into(), at: Some("2026-10-14T12:40:00Z".into()) },
                Entry::Agent { text: "Fixed.".into(), at: None },
                Entry::User { text: "Commit it".into(), at: Some("2026-10-14T13:50:00Z".into()) },
                Entry::Agent { text: "Committed.".into(), at: None },
            ],
            ..Transcript::default()
        };
        sessions.receive(ServerMsg::SessionTranscript {
            request_id: 9,
            session_id: id.into(),
            conversation: Box::new(Conversation::Read(transcript)),
        });
        sessions
    }

    #[test]
    fn the_full_transcript_opens_in_place_of_the_list_and_escape_gives_the_list_back() {
        let sessions = read("old");
        let mut d = dialog();
        d.move_by(1, &sessions, &tree());
        assert_eq!(d.open_reader(&sessions, &tree()), Then::Nothing);
        let page = d.page(&sessions, &tree(), 0);
        let reader = page.reader.expect("open");
        assert!(page.preview.is_none(), "drawn once, as the reader");
        assert_eq!((reader.session.session_id.as_str(), reader.repository.as_str()), ("old", "argus"));
        assert_eq!(reader.jumps.iter().map(|j| j.text.as_str()).collect::<Vec<_>>(), ["Fix the build", "Commit it"]);
        assert_eq!(reader.jumps[1].at.as_deref(), Some("2026-10-14T13:50:00Z"));
        assert!(d.close_reader());
        let page = d.page(&sessions, &tree(), 0);
        assert!(page.reader.is_none());
        assert_eq!(page.rows.iter().find(|r| r.on).map(|r| r.title.as_str()), Some("old work"), "the cursor stayed");
    }

    #[test]
    fn the_reader_jumps_between_turns_finds_copies_and_resumes_its_session() {
        let sessions = read("old");
        let mut d = dialog();
        d.move_by(1, &sessions, &tree());
        d.open_reader(&sessions, &tree());
        let target = |d: &SessionsDialog| d.page(&sessions, &tree(), 0).reader.unwrap().target;
        d.reader_action(Action::Next, None, &sessions, &tree());
        assert_eq!(target(&d), Some(0));
        d.reader_action(Action::Last, None, &sessions, &tree());
        assert_eq!(target(&d), Some(2));
        d.reader_action(Action::Next, Some(0), &sessions, &tree());
        assert_eq!(target(&d), Some(0), "a click on the jump list");
        d.find("committed".into(), &sessions);
        assert_eq!(target(&d), Some(3));
        d.reader_action(Action::NextMatch, None, &sessions, &tree());
        assert_eq!(target(&d), Some(3), "the only match");
        assert_eq!(d.reader_action(Action::CopyId, None, &sessions, &tree()), Then::Copy("old".into()));
        assert_eq!(d.reader_action(Action::Choose, None, &sessions, &tree()), Then::Nothing, "an ended one has no pane");
        assert!(matches!(
            d.reader_action(Action::Resume, None, &sessions, &tree()),
            Then::Resume { ref session_id, .. } if session_id == "old"
        ));
    }

    #[test]
    fn the_reader_of_a_live_session_goes_to_its_pane() {
        let sessions = read("live");
        let mut d = dialog();
        d.open_reader(&sessions, &tree());
        assert_eq!(d.page(&sessions, &tree(), 0).reader.unwrap().session.offer, "go");
        assert_eq!(d.reader_action(Action::Choose, None, &sessions, &tree()), Then::GoTo(PaneId(7)));
    }

    #[test]
    fn the_reader_of_a_session_that_cannot_resume_says_why_and_still_copies_its_id() {
        let sessions = read("plain");
        let mut d = dialog();
        d.set_search("plain".into());
        d.open_reader(&sessions, &tree());
        let reader = d.page(&sessions, &tree(), 0).reader.expect("open");
        assert_eq!(reader.session.offer, "transcript");
        assert_eq!(reader.session.why.as_deref(), Some("generic cannot resume a session by its id"));
        assert!(matches!(d.reader_action(Action::Resume, None, &sessions, &tree()), Then::Say(_)));
        assert_eq!(d.reader_action(Action::CopyId, None, &sessions, &tree()), Then::Copy("plain".into()));
    }

    #[test]
    fn a_session_without_a_transcript_is_listed_but_has_nothing_to_open() {
        let mut status = StatusBar::new(false);
        status.connected(Some(Hello { capabilities: vec![SESSIONS.into()], ..Hello::this_build() }), None);
        let mut sessions = held(listing());
        let d0 = dialog();
        assert!(d0.page(&sessions, &tree(), 0).preview.unwrap().readable, "not known to be bare until read");
        for (conversation, why) in [
            (Conversation::NoTranscript, "codex keeps no transcript Argus can read"),
            (Conversation::Missing, "its transcript is missing: moved, cleaned up, or never written"),
        ] {
            let mut d = dialog();
            let wanted = d.wants_read(&sessions, &tree()).cloned().unwrap_or_else(|| listing()[1].clone());
            sessions.read(&status, &wanted, 5);
            sessions.receive(ServerMsg::SessionTranscript { request_id: 5, session_id: "live".into(), conversation: Box::new(conversation) });
            assert!(!d.page(&sessions, &tree(), 0).preview.unwrap().readable);
            assert_eq!(d.open_reader(&sessions, &tree()), Then::Say(why.into()));
            assert!(!d.reading());
        }
    }

    #[test]
    fn a_daemon_that_cannot_list_sessions_is_said_to_and_not_waited_on() {
        let page = SessionsDialog::new(RepositoryId(10), "argus".into(), true).page(&Sessions::default(), &tree(), 0);
        assert!(!page.loading && page.empty.is_some_and(|e| e.contains("cannot list sessions")));
        let page = dialog().page(&Sessions::default(), &tree(), 0);
        assert!(page.loading, "asked and not answered");
    }

    #[test]
    fn an_empty_listing_says_so() {
        let page = dialog().page(&held(Vec::new()), &tree(), 0);
        assert_eq!(page.empty.as_deref(), Some("No agent has worked in argus yet."));
        assert!(page.preview.is_none());
    }
}
