//! The Code stage as the window holds it once opened — the checkout it
//! reads, the version, that version's files and the ones it changed, the
//! folders open, the file read and the cursor in it, the comment being
//! written, whether the outline shows, the go-to-file field, the search and
//! how it reads its query, and the dialog it was opened from — the daemon's
//! answers it takes, what its gestures ask of the daemon and leave to the
//! window, and the page it draws and last sent. What the daemon read is
//! shown as it said it; nothing is predicted, so a file that will not read
//! says so where its lines would be. Whether a comment may go, and to whom,
//! is `agent_comment`'s.

use std::collections::HashSet;

use argus_protocol::{
    CheckoutId, CheckoutInfo, ClientMsg, CodeChange, CodeFile, CodeMatches, CodeTree, CodeVersion, HeldTree, PaneId,
    ProjectInfo, RequestRefusal, ReviewAnchor, ReviewBase, SearchOptions, ServerMsg,
};
use serde::{Deserialize, Serialize};

use crate::code;
use crate::code::tree::folders_of;
use crate::review::comment_box::{CommentBox, CommentPage};
use crate::review::file_jump::{FileJump, Jumped};
use crate::keys::keymap::Action;
use crate::connection::page::{Event, ReviewJumpGesture};
use crate::review::dialog::parts;
use crate::connection::sent::Sent;

/// What `B` goes back to: the dialog the stage was opened from, as the
/// button says it — `review · brief.rs:39`, `history`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Back {
    pub label: String,
}

/// The Code stage: one checkout's files, one of them read whole, its changes
/// marked and ruled, and what it defines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodePage {
    pub open: bool,
    pub checkout: u64,
    pub name: String,
    pub branch: String,
    /// What is read when it is not the working tree, as the header says it:
    /// `index`, or a commit's short oid.
    pub version: Option<String>,
    /// Whether the tree lists only the files this version changed.
    pub changed_only: bool,
    /// How many files this version changed.
    pub changed: usize,
    pub tree: Vec<CodeTreeRow>,
    pub tree_loading: bool,
    /// The file being read.
    pub path: Option<String>,
    /// Its language, by its extension, when there is a name for it.
    pub language: Option<&'static str>,
    pub added: u32,
    pub removed: u32,
    pub lines: Vec<CodeRow>,
    /// Lines removed after the file's last line.
    pub removed_at_end: u32,
    /// The cursor's line, from one.
    pub at: u32,
    /// Where in the whole file it changed, and where the search matched it.
    pub ruler: Vec<CodeMark>,
    /// Whether the outline is shown.
    pub outline: bool,
    pub symbols: Vec<CodeOutlineRow>,
    /// Each run of changed lines and each place lines were removed.
    pub changes_here: Vec<CodeChangeRow>,
    /// Why a file shows no lines: binary, or too large.
    pub note: Option<String>,
    pub file_loading: bool,
    pub jump: Option<CodeJump>,
    pub search: Option<CodeSearch>,
    /// What `B` goes back to, `review · brief.rs:39` or `history`, when the
    /// stage was opened from one.
    pub back: Option<String>,
    pub failed: Option<String>,
    /// The picked lines, `lines 3–5`, while a range or a comment is open.
    pub pick: Option<String>,
    /// The comment box, while it is open.
    pub comment: Option<CommentPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodeTreeRow {
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub folder: bool,
    pub open: bool,
    pub changed: bool,
    pub added: u32,
    pub removed: u32,
    /// The file being read.
    pub on: bool,
}

/// One line of the file being read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodeRow {
    pub no: u32,
    /// As a review line's: `[text, highlight]`.
    pub parts: Vec<(String, &'static str)>,
    /// This version added or changed it.
    pub added: bool,
    /// How many lines this version removed just above it.
    pub removed_before: u32,
    pub cursor: bool,
    pub picked: bool,
}

/// A mark on the change ruler, at `line` of `lines` in the whole file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodeMark {
    pub line: u32,
    /// How many lines the mark spans: a run of changed lines is as tall as
    /// it is long.
    pub span: u32,
    #[cfg_attr(test, ts(type = "\"added\" | \"removed\" | \"hit\""))]
    pub kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodeOutlineRow {
    pub name: String,
    /// Short: `fn`, `type`, `mod`.
    pub kind: &'static str,
    pub line: u32,
    /// The definition the cursor is in.
    pub on: bool,
    /// Something in it changed.
    pub changed: bool,
}

/// A run of changed lines, or a place lines were removed, by its first
/// line and what it says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodeChangeRow {
    pub line: u32,
    pub text: String,
    pub on: bool,
}

/// The go-to-file field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodeJump {
    pub query: String,
    /// The paths it matches, best first.
    pub matches: Vec<String>,
    pub at: Option<usize>,
}

/// The search across the checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodeSearch {
    pub query: String,
    pub glob: String,
    pub regex: bool,
    pub case: bool,
    pub word: bool,
    /// The matches are for the query and options as they stand.
    pub current: bool,
    pub loading: bool,
    /// Why the search could not run: a pattern or glob it could not read.
    pub error: Option<String>,
    pub total: usize,
    pub truncated: bool,
    pub files: Vec<CodeHitFile>,
}

/// The matches in one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodeHitFile {
    pub path: String,
    pub dir: String,
    pub name: String,
    pub count: usize,
    pub open: bool,
    pub hits: Vec<CodeHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodeHit {
    /// Which match it is, in the order the search returned them.
    pub index: usize,
    pub line: u32,
    /// The line as `[text, matched]`, cut short where it is long before its
    /// first match.
    pub parts: Vec<(String, bool)>,
    pub on: bool,
}

/// What a click in the Code stage landed on.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum CodeClick {
    /// A folder opens or closes; a file is read.
    Tree { path: String },
    /// With `extend`, the range's other end stays where it was.
    Line { line: u32, extend: bool },
    Recipient { pane: u64 },
    /// An outline row, a change, or a mark on the ruler.
    Go { line: u32 },
    ChangedOnly { only: bool },
    Outline,
    /// A search match, read where it is.
    Match { index: usize },
    /// A file of the search's results, folded or opened.
    HitFile { path: String },
    /// One of the search's options turned on or off.
    SearchOption {
        #[cfg_attr(test, ts(type = "\"regex\" | \"case\" | \"word\""))]
        option: String,
    },
    Back,
}

/// What was done in the Code stage.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum CodeGesture {
    Click { target: CodeClick },
    /// A key of the stage, or a button standing for one.
    Action { action: Action },
    /// The go-to-file field.
    Jump { gesture: ReviewJumpGesture },
    /// The search field.
    Search { gesture: ReviewJumpGesture },
    /// What the files-to-include field says; it is searched with on Enter.
    Glob { glob: String },
    /// The comment sent, or with no `body` the box put away.
    Comment { body: Option<String> },
}

/// What a gesture leaves for the window to do beyond the daemon's messages.
#[derive(Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    /// Back to the dialog the stage was opened from, over the stage it was
    /// opened over.
    Back,
    /// The file and line under the cursor, for the person's editor.
    Edit { checkout: CheckoutId, path: String, line: Option<u32> },
    /// Why the gesture did nothing, for the person to hear.
    Say(String),
}

/// How many of its matches the go-to-file field lists.
const CODE_JUMP_SHOWN: usize = 50;

#[derive(Default)]
struct Search {
    query: String,
    options: SearchOptions,
    asked: Option<u64>,
    found: Option<CodeMatches>,
    /// Why the last search could not run.
    error: Option<String>,
    /// The match the cursor is on, in the order the search returned them.
    at: usize,
    /// Files of the results folded shut.
    folded: HashSet<String>,
}

impl Search {
    /// Whether the matches held answer the query and options as they stand.
    fn current(&self) -> bool {
        self.found.as_ref().is_some_and(|f| f.query == self.query && f.options == self.options)
    }
}

/// The Code stage as the window holds it once opened: kept, checkout, file
/// and place, while another stage is up, and what the page last heard of it.
#[derive(Default)]
pub struct Code {
    stage: Option<CodeStage>,
    sent: Sent<CodePage>,
}

impl Code {
    /// `checkout` at `version`, reading `path` at `line` when one is given,
    /// in place of whatever stage was open: what to ask the daemon for it.
    pub fn open(
        &mut self,
        checkout: &CheckoutInfo,
        version: CodeVersion,
        path: Option<String>,
        line: Option<u32>,
        back: Option<Back>,
        id: impl FnMut() -> u64,
    ) -> Vec<ClientMsg> {
        let (stage, msgs) = CodeStage::open(checkout, version, path, line, back, id);
        self.stage = Some(stage);
        msgs
    }

    pub fn checkout(&self) -> Option<CheckoutId> {
        self.stage.as_ref().map(|s| s.checkout)
    }

    pub fn back(&self) -> Option<&Back> {
        self.stage.as_ref()?.back.as_ref()
    }

    /// The dialog it was opened from is back up: there is nothing more to
    /// go back to.
    pub fn forget_back(&mut self) {
        if let Some(stage) = &mut self.stage {
            stage.back = None;
        }
    }

    /// Takes the daemon's answers to the stage's asks, giving back any
    /// message that is not the stage's. An answer with no stage open is
    /// dropped, as one to an older ask is. A saved comment is given back
    /// too, for the dialogs to say: the save names the box it is for, which
    /// may be a review's. A refusal is the stage's only when it names one of
    /// its asks.
    pub fn receive(&mut self, msg: ServerMsg) -> Option<ServerMsg> {
        let stage = self.stage.as_mut();
        match msg {
            ServerMsg::CodeTree(tree) => {
                if let Some(stage) = stage {
                    stage.receive_tree(tree);
                }
            }
            ServerMsg::Code(file) => {
                if let Some(stage) = stage {
                    stage.receive_file(*file);
                }
            }
            ServerMsg::CodeMatches(found) => {
                if let Some(stage) = stage {
                    stage.receive_matches(found);
                }
            }
            ServerMsg::Refused(refusal) => {
                let mine = stage.is_some_and(|s| s.failed(refusal.request_id, refusal.reason.clone()));
                return (!mine).then_some(ServerMsg::Refused(refusal));
            }
            ServerMsg::ReviewCommentSaved { request_id, .. } => {
                if let Some(stage) = stage {
                    stage.commented(request_id);
                }
                return Some(msg);
            }
            msg => return Some(msg),
        }
        None
    }

    /// A refusal that may be of the stage's comment: its box lets go and
    /// keeps its words. Said by the caller, as a refused change is.
    pub fn comment_refused(&mut self, refusal: &RequestRefusal) -> bool {
        self.stage.as_mut().is_some_and(|s| s.comment_refused(refusal))
    }

    /// A new connection: the stage's comment is answered on none.
    pub fn forget_sent_comment(&mut self) {
        if let Some(stage) = &mut self.stage {
            stage.forget_sent_comment();
        }
    }

    /// A gesture in the stage, with the tree as it is now: its agents may
    /// have come or gone. B goes back even with no stage open.
    pub fn gesture(
        &mut self,
        gesture: CodeGesture,
        tree: &[ProjectInfo],
        mut id: impl FnMut() -> u64,
        out: &mut Vec<ClientMsg>,
    ) -> Then {
        if matches!(gesture, CodeGesture::Click { target: CodeClick::Back }) {
            return Then::Back;
        }
        let Some(stage) = &mut self.stage else {
            return Then::Nothing;
        };
        let checkout = tree.checkout(stage.checkout);
        match gesture {
            CodeGesture::Click { target } => out.extend(stage.click(target, id)),
            CodeGesture::Action { action } => return stage.act(action, checkout, id, out),
            CodeGesture::Jump { gesture } => out.extend(stage.jump(gesture, id)),
            CodeGesture::Search { gesture } => out.extend(stage.search(gesture, id)),
            CodeGesture::Glob { glob } => stage.set_glob(glob),
            CodeGesture::Comment { body: None } => stage.cancel_comment(),
            CodeGesture::Comment { body: Some(body) } => match stage.comment(checkout, &body, id()) {
                Ok(msg) => out.push(msg),
                Err(text) => return Then::Say(text.into()),
            },
        }
        Then::Nothing
    }

    /// What the page has not heard of the stage. Sent only while `up`, and
    /// never taken back when it goes: the page keeps what it last drew,
    /// scrolled where it was, for the person coming back to it.
    pub fn take(&mut self, up: bool, tree: &[ProjectInfo]) -> Option<Event> {
        let stage = self.stage.as_ref().filter(|_| up)?;
        let page = stage.page(tree.checkout(stage.checkout));
        self.sent.take(Some(page)).map(|page| Event::Code(Box::new(page)))
    }

    /// The page reloaded and holds nothing: the stage is sent again once it
    /// is up.
    pub fn reload(&mut self) {
        self.sent.forget();
    }

    /// The page as it stands, for a test to read without taking the
    /// window's events, notices among them.
    #[cfg(test)]
    pub fn page(&self, tree: &[ProjectInfo]) -> Option<CodePage> {
        let stage = self.stage.as_ref()?;
        Some(stage.page(tree.checkout(stage.checkout)))
    }
}

struct CodeStage {
    checkout: CheckoutId,
    name: String,
    branch: String,
    version: CodeVersion,
    changed_only: bool,
    files: Vec<String>,
    changes: Vec<CodeChange>,
    tree_asked: Option<u64>,
    open: HashSet<String>,
    path: Option<String>,
    file: Option<Box<CodeFile>>,
    file_asked: Option<u64>,
    /// The line the cursor is on, from zero.
    cursor: usize,
    /// The other end of a `v` range, from zero.
    mark: Option<usize>,
    comment_box: CommentBox,
    /// The line to put the cursor on once the file asked for arrives.
    land: Option<u32>,
    outline: bool,
    jump: Option<FileJump>,
    search: Option<Search>,
    back: Option<Back>,
    failed: Option<String>,
}

impl CodeStage {
    /// `checkout` at `version`, reading `path` at `line` when one is given.
    fn open(
        checkout: &CheckoutInfo,
        version: CodeVersion,
        path: Option<String>,
        line: Option<u32>,
        back: Option<Back>,
        mut id: impl FnMut() -> u64,
    ) -> (CodeStage, Vec<ClientMsg>) {
        let mut stage = CodeStage {
            checkout: checkout.id,
            name: checkout.name.clone(),
            branch: checkout.git.as_ref().and_then(|g| g.branch.clone()).unwrap_or_default(),
            version,
            changed_only: false,
            files: Vec::new(),
            changes: Vec::new(),
            tree_asked: None,
            open: HashSet::new(),
            path: None,
            file: None,
            file_asked: None,
            cursor: 0,
            mark: None,
            comment_box: CommentBox::default(),
            land: None,
            outline: true,
            jump: None,
            search: None,
            back,
            failed: None,
        };
        let mut msgs = vec![stage.ask_tree(id())];
        if let Some(path) = path {
            msgs.push(stage.read(path, line, id()));
        }
        (stage, msgs)
    }

    fn ask_tree(&mut self, request_id: u64) -> ClientMsg {
        self.tree_asked = Some(request_id);
        ClientMsg::ListCode { request_id, checkout: self.checkout, version: self.version.clone() }
    }

    /// Reads `path`, with the folders above it opened so the tree shows
    /// where it is.
    fn read(&mut self, path: String, line: Option<u32>, request_id: u64) -> ClientMsg {
        self.open.extend(folders_of(&path).map(str::to_owned));
        self.file_asked = Some(request_id);
        self.land = line;
        self.mark = None;
        self.comment_box.close();
        self.failed = None;
        self.path = Some(path.clone());
        ClientMsg::ReadCode { request_id, checkout: self.checkout, path, version: self.version.clone() }
    }

    /// Reads the version again: the agent kept working.
    fn refresh(&mut self, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        let mut msgs = vec![self.ask_tree(id())];
        if let Some(path) = self.path.clone() {
            let line = Some(self.cursor as u32 + 1);
            msgs.push(self.read(path, line, id()));
        }
        msgs
    }

    fn receive_tree(&mut self, tree: CodeTree) -> bool {
        if tree.checkout != self.checkout || self.tree_asked != Some(tree.request_id) {
            return false;
        }
        self.tree_asked = None;
        self.files = tree.files;
        self.changes = tree.changes;
        true
    }

    fn receive_file(&mut self, file: CodeFile) -> bool {
        if file.checkout != self.checkout || self.file_asked != Some(file.request_id) {
            return false;
        }
        self.file_asked = None;
        let last = file.lines.len().saturating_sub(1);
        self.cursor = self.land.take().map_or(0, |line| (line as usize).saturating_sub(1).min(last));
        self.file = Some(Box::new(file));
        true
    }

    fn receive_matches(&mut self, found: CodeMatches) -> bool {
        let Some(search) = self.search.as_mut() else {
            return false;
        };
        if found.checkout != self.checkout || search.asked != Some(found.request_id) {
            return false;
        }
        search.asked = None;
        search.at = 0;
        search.error = None;
        search.folded.clear();
        search.found = Some(found);
        true
    }

    /// A request of this stage's that could not be answered, known by its
    /// id alone: the window names every request from one count. A file
    /// that would not read leaves nothing of an older one on screen; a
    /// search that could not run says why in the search.
    fn failed(&mut self, request_id: u64, message: String) -> bool {
        if let Some(search) = self.search.as_mut().filter(|s| s.asked == Some(request_id)) {
            search.asked = None;
            search.error = Some(message);
            return true;
        }
        if self.file_asked == Some(request_id) {
            self.file_asked = None;
            self.file = None;
        } else if self.tree_asked == Some(request_id) {
            self.tree_asked = None;
        } else {
            return false;
        }
        self.failed = Some(message);
        true
    }

    /// A key of the stage. B goes back only to a dialog the stage was
    /// opened from; E opens the file at the cursor in the person's editor,
    /// as the review's does.
    fn act(&mut self, action: Action, checkout: Option<&CheckoutInfo>, id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) -> Then {
        match action {
            Action::GoBack if self.back.is_some() => return Then::Back,
            // Escape closes what is over the file and leaves the stage up:
            // the stage keys and B are the ways out.
            Action::CloseLayer => {
                self.escape();
            }
            Action::ToggleOutline => self.toggle_outline(),
            Action::Next => self.move_by(1),
            Action::Previous => self.move_by(-1),
            Action::First => self.ends(false),
            Action::Last => self.ends(true),
            Action::NextFile => self.next_change(true),
            Action::PreviousFile => self.next_change(false),
            Action::Filter => self.open_jump(),
            Action::Search => self.open_search(),
            Action::Pick => self.toggle_pick(),
            Action::Comment => {
                if let Err(text) = self.compose(checkout) {
                    return Then::Say(text.into());
                }
            }
            Action::Refresh => out.extend(self.refresh(id)),
            Action::OpenEditor => {
                return match self.editor_target() {
                    Some((checkout, path, line)) => Then::Edit { checkout, path, line },
                    None => Then::Say("no file to open".into()),
                }
            }
            _ => {}
        }
        Then::Nothing
    }

    fn click(&mut self, target: CodeClick, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        match target {
            CodeClick::Tree { path } => {
                if self.files.binary_search(&path).is_ok() {
                    return vec![self.read(path, None, id())];
                }
                if !self.open.remove(&path) {
                    self.open.insert(path);
                }
            }
            CodeClick::Line { line, extend } => {
                if extend && self.mark.is_none() {
                    self.mark = Some(self.cursor);
                } else if !extend {
                    self.mark = None;
                }
                self.go_to(line);
            }
            CodeClick::Go { line } => self.go_to(line),
            CodeClick::Recipient { pane } => self.comment_box.choose(PaneId(pane)),
            CodeClick::ChangedOnly { only } => self.changed_only = only,
            CodeClick::Outline => self.toggle_outline(),
            CodeClick::Match { index } => {
                if let Some(search) = &mut self.search {
                    search.at = index;
                }
                return self.choose_search(id);
            }
            CodeClick::HitFile { path } => {
                if let Some(search) = &mut self.search {
                    if !search.folded.remove(&path) {
                        search.folded.insert(path);
                    }
                }
            }
            CodeClick::SearchOption { option } => return self.toggle_option(&option, id),
            // `Code::gesture` hands this to the window.
            CodeClick::Back => {}
        }
        Vec::new()
    }

    /// `O`: the outline shown, or put away for the file's width.
    fn toggle_outline(&mut self) {
        self.outline = !self.outline;
    }

    /// Escape: what is over the file closes, one thing at a time — the
    /// comment, the search, the go-to-file list, the picked range — and the
    /// file is what is left. Never the stage: false when there was nothing
    /// to close.
    fn escape(&mut self) -> bool {
        if self.comment_box.is_open() {
            self.comment_box.close();
        } else if self.search.is_some() {
            self.search = None;
        } else if self.jump.is_some() {
            self.jump = None;
        } else if self.mark.is_some() {
            self.mark = None;
        } else {
            return false;
        }
        true
    }

    fn lines(&self) -> usize {
        self.file.as_ref().map_or(0, |f| f.lines.len())
    }

    fn go_to(&mut self, line: u32) {
        self.cursor = (line as usize).saturating_sub(1).min(self.lines().saturating_sub(1));
    }

    fn move_by(&mut self, delta: isize) {
        let last = self.lines().saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, last.max(0)) as usize;
    }

    fn ends(&mut self, bottom: bool) {
        self.cursor = if bottom { self.lines().saturating_sub(1) } else { 0 };
    }

    /// The file read, when it is the one the cursor is in.
    fn shown(&self) -> Option<&CodeFile> {
        self.file.as_deref().filter(|f| Some(&f.path) == self.path.as_ref())
    }

    /// The changes in the file read: each run of changed lines by its first
    /// line, from zero, and how long it is, and each place lines were
    /// removed, by the line after them.
    fn runs(&self) -> Vec<Run> {
        let Some(file) = self.shown() else {
            return Vec::new();
        };
        let mut runs = Vec::new();
        let mut i = 0;
        while i < file.lines.len() {
            if file.lines[i].added {
                let start = i;
                while i < file.lines.len() && file.lines[i].added {
                    i += 1;
                }
                runs.push(Run::Added { start, len: i - start });
            } else {
                i += 1;
            }
        }
        for removed in &file.removed {
            runs.push(Run::Removed { before: removed.before as usize - 1, count: removed.count });
        }
        runs.sort_by_key(Run::start);
        runs.dedup_by_key(|r| r.start());
        runs
    }

    /// `]` and `[`: the next or previous change, whether lines changed or
    /// were removed there.
    fn next_change(&mut self, forward: bool) {
        let starts: Vec<usize> = self.runs().iter().map(Run::start).collect();
        let found = if forward {
            starts.iter().find(|&&s| s > self.cursor)
        } else {
            starts.iter().rev().find(|&&s| s < self.cursor)
        };
        if let Some(&line) = found {
            self.cursor = line.min(self.lines().saturating_sub(1));
        }
    }

    /// `E`: the file and line under the cursor, for the person's editor.
    fn editor_target(&self) -> Option<(CheckoutId, String, Option<u32>)> {
        let path = self.path.clone()?;
        Some((self.checkout, path, self.file.as_ref().map(|_| self.cursor as u32 + 1)))
    }

    // ---- comments --------------------------------------------------------

    /// `v`: a range starts at the cursor, or is let go.
    fn toggle_pick(&mut self) {
        self.mark = match self.mark {
            Some(_) => None,
            None => Some(self.cursor),
        };
    }

    /// The picked lines, from zero, top first: the cursor's alone with no
    /// range.
    fn picked(&self) -> (usize, usize) {
        let mark = self.mark.unwrap_or(self.cursor);
        (mark.min(self.cursor), mark.max(self.cursor))
    }

    /// What a comment written now attaches to: the picked lines of the file
    /// read, on the side of a review the version is — the working tree as
    /// unstaged work, the index as staged, a commit as that commit. A line
    /// read here has only the version's number; the other side's is not
    /// known without a diff, and a comment needs no more than one.
    fn anchor(&self) -> Option<ReviewAnchor> {
        let file = self.shown()?;
        let (from, to) = self.picked();
        let lines = file.lines.get(from..=to)?;
        let (base, commit) = match &self.version {
            CodeVersion::WorkingTree => (ReviewBase::Unstaged, None),
            CodeVersion::Index => (ReviewBase::Staged, None),
            CodeVersion::Head => (ReviewBase::Commit, None),
            CodeVersion::Commit(oid) => (ReviewBase::Commit, Some(oid.clone())),
        };
        Some(ReviewAnchor {
            base,
            commit,
            path: file.path.clone(),
            old_path: None,
            old_start: None,
            old_end: None,
            new_start: Some(from as u32 + 1),
            new_end: Some(to as u32 + 1),
            text: lines.iter().map(|l| format!("{}{}", if l.added { '+' } else { ' ' }, l.text)).collect(),
        })
    }

    /// `c`: the comment box, under the picked lines, when there are lines
    /// and an agent to read it.
    fn compose(&mut self, checkout: Option<&CheckoutInfo>) -> Result<(), &'static str> {
        self.comment_box.open(self.anchor().as_ref(), checkout)
    }

    fn cancel_comment(&mut self) {
        self.comment_box.close();
    }

    /// The comment, sent through the review's path under `request_id`:
    /// stored under the checkout, then told to the agent. The box stays
    /// until it is saved.
    fn comment(&mut self, checkout: Option<&CheckoutInfo>, body: &str, request_id: u64) -> Result<ClientMsg, &'static str> {
        let anchor = self.anchor();
        self.comment_box.send(anchor, checkout, body, request_id)
    }

    /// The daemon saved the comment sent under `request_id`. When it was
    /// this box's, the box closes and the range with it; another box's save
    /// leaves both.
    fn commented(&mut self, request_id: u64) {
        if self.comment_box.saved(request_id) {
            self.mark = None;
        }
    }

    fn comment_refused(&mut self, refusal: &RequestRefusal) -> bool {
        self.comment_box.refused(refusal)
    }

    fn forget_sent_comment(&mut self) {
        self.comment_box.forget_sent();
    }

    // ---- go to a file ----------------------------------------------------

    fn open_jump(&mut self) {
        self.search = None;
        if self.jump.is_none() {
            self.jump = Some(FileJump::new(CODE_JUMP_SHOWN));
        }
    }

    fn jump(&mut self, gesture: ReviewJumpGesture, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        let Some(jump) = &mut self.jump else {
            return Vec::new();
        };
        match jump.gesture(gesture, &self.files) {
            Jumped::Kept => Vec::new(),
            Jumped::Closed => {
                self.jump = None;
                Vec::new()
            }
            Jumped::Open(file) => {
                self.jump = None;
                vec![self.read(self.files[file].clone(), None, id())]
            }
        }
    }

    // ---- search ----------------------------------------------------------

    fn open_search(&mut self) {
        self.jump = None;
        if self.search.is_none() {
            self.search = Some(Search::default());
        }
    }

    /// The search field: typing changes the query, Enter searches for it —
    /// and with the matches current, reads the one the cursor is on.
    fn search(&mut self, gesture: ReviewJumpGesture, id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        let Some(search) = &mut self.search else {
            return Vec::new();
        };
        let found = search.found.as_ref().map_or(0, |f| f.matches.len());
        match gesture {
            ReviewJumpGesture::Query { text } => search.query = text,
            ReviewJumpGesture::Step { by } => search.at = search.at.saturating_add_signed(by).min(found.saturating_sub(1)),
            ReviewJumpGesture::Close => self.search = None,
            ReviewJumpGesture::Choose => return self.choose_search(id),
        }
        Vec::new()
    }

    /// What the files-to-include field says. It is searched with on Enter.
    fn set_glob(&mut self, glob: String) {
        if let Some(search) = &mut self.search {
            search.options.glob = glob;
        }
    }

    /// One of the search's options flipped. A search already made is made
    /// again at once: the person is looking at its matches.
    fn toggle_option(&mut self, option: &str, id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        let Some(search) = &mut self.search else {
            return Vec::new();
        };
        let flag = match option {
            "regex" => &mut search.options.regex,
            "case" => &mut search.options.case,
            "word" => &mut search.options.word,
            _ => return Vec::new(),
        };
        *flag = !*flag;
        if search.found.is_some() || search.error.is_some() {
            return self.ask_search(id).into_iter().collect();
        }
        Vec::new()
    }

    fn ask_search(&mut self, mut id: impl FnMut() -> u64) -> Option<ClientMsg> {
        let checkout = self.checkout;
        let search = self.search.as_mut()?;
        if search.query.trim().is_empty() {
            return None;
        }
        let request_id = id();
        search.asked = Some(request_id);
        search.error = None;
        Some(ClientMsg::SearchCode { request_id, checkout, query: search.query.clone(), options: search.options.clone() })
    }

    fn choose_search(&mut self, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        let Some(search) = &self.search else {
            return Vec::new();
        };
        if !search.current() {
            return self.ask_search(id).into_iter().collect();
        }
        let Some(hit) = search.found.as_ref().and_then(|f| f.matches.get(search.at)).cloned() else {
            return Vec::new();
        };
        // The match is in the working tree, so that is the version read.
        if self.version != CodeVersion::WorkingTree {
            self.version = CodeVersion::WorkingTree;
            let msg = self.ask_tree(id());
            return vec![msg, self.read(hit.path, Some(hit.line), id())];
        }
        if self.path.as_deref() == Some(hit.path.as_str()) && self.file_asked.is_none() {
            self.go_to(hit.line);
            return Vec::new();
        }
        vec![self.read(hit.path, Some(hit.line), id())]
    }

    // ---- the page --------------------------------------------------------

    /// The stage as the page draws it, given the checkout as the tree has
    /// it now: its agents may have come or gone.
    fn page(&self, checkout: Option<&CheckoutInfo>) -> CodePage {
        let (from, to) = self.picked();
        let picking = self.mark.is_some() || self.comment_box.is_open();
        let shown = self.shown();
        let change = self.path.as_ref().and_then(|p| self.changes.iter().find(|c| &c.path == p));
        let mut lines: Vec<CodeRow> = shown
            .map(|f| {
                f.lines
                    .iter()
                    .enumerate()
                    .map(|(i, l)| CodeRow {
                        no: i as u32 + 1,
                        parts: parts(&l.text, &l.spans),
                        added: l.added,
                        removed_before: 0,
                        cursor: i == self.cursor,
                        picked: picking && (from..=to).contains(&i),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut removed_at_end = 0;
        for removed in shown.map(|f| f.removed.as_slice()).unwrap_or_default() {
            match lines.get_mut(removed.before as usize - 1) {
                Some(row) => row.removed_before += removed.count,
                None => removed_at_end += removed.count,
            }
        }
        let runs = self.runs();
        let last = lines.len().max(1) as u32;
        let mut ruler: Vec<CodeMark> = runs
            .iter()
            .map(|run| match *run {
                Run::Added { start, len } => CodeMark { line: start as u32 + 1, span: len as u32, kind: "added" },
                Run::Removed { before, .. } => CodeMark { line: (before as u32 + 1).min(last), span: 1, kind: "removed" },
            })
            .collect();
        let search = self.search.as_ref().filter(|s| s.current());
        let hits_here = search
            .and_then(|s| s.found.as_ref())
            .into_iter()
            .flat_map(|f| f.matches.iter())
            .filter(|m| Some(&m.path) == self.path.as_ref());
        ruler.extend(hits_here.map(|m| CodeMark { line: m.line, span: 1, kind: "hit" }));
        let at_line = self.cursor as u32 + 1;
        let symbols = shown.map(|f| f.symbols.as_slice()).unwrap_or_default();
        let inside = symbols.iter().rposition(|s| s.line <= at_line);
        let pick = (picking && shown.is_some()).then(|| {
            if from == to { format!("line {}", from + 1) } else { format!("lines {}–{}", from + 1, to + 1) }
        });
        let comment = self
            .comment_box
            .page(checkout, || format!("{} · {}", pick.as_deref().unwrap_or(""), self.path.as_deref().unwrap_or("")));
        CodePage {
            open: true,
            checkout: self.checkout.0,
            name: self.name.clone(),
            branch: self.branch.clone(),
            version: match &self.version {
                CodeVersion::WorkingTree => None,
                CodeVersion::Index => Some("index".to_owned()),
                CodeVersion::Head => Some("HEAD".to_owned()),
                CodeVersion::Commit(oid) => Some(oid.chars().take(7).collect()),
            },
            changed_only: self.changed_only,
            changed: self.changes.len(),
            tree: code::tree::rows(&self.files, &self.changes, &self.open, self.changed_only)
                .into_iter()
                .map(|r| CodeTreeRow {
                    on: !r.folder && Some(&r.path) == self.path.as_ref(),
                    path: r.path,
                    name: r.name,
                    depth: r.depth,
                    folder: r.folder,
                    open: r.open,
                    changed: r.changed,
                    added: r.added,
                    removed: r.removed,
                })
                .collect(),
            tree_loading: self.tree_asked.is_some() && self.files.is_empty(),
            path: self.path.clone(),
            language: self.path.as_deref().and_then(language),
            added: change.map_or(0, |c| c.added),
            removed: change.map_or(0, |c| c.removed),
            lines,
            removed_at_end,
            at: at_line,
            ruler,
            outline: self.outline,
            symbols: symbols
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let end = symbols.get(i + 1).map_or(u32::MAX, |next| next.line);
                    let changed = shown.is_some_and(|f| {
                        f.lines.iter().enumerate().any(|(n, l)| l.added && (s.line..end).contains(&(n as u32 + 1)))
                    });
                    CodeOutlineRow { name: s.name.clone(), kind: short_kind(&s.kind), line: s.line, on: Some(i) == inside, changed }
                })
                .collect(),
            changes_here: runs
                .iter()
                .map(|run| {
                    let (start, text) = match *run {
                        Run::Added { start, .. } => {
                            let text = shown.and_then(|f| f.lines.get(start)).map_or("", |l| l.text.trim());
                            (start, text.chars().take(48).collect())
                        }
                        Run::Removed { before, count } => {
                            (before, format!("− {count} {} removed", if count == 1 { "line" } else { "lines" }))
                        }
                    };
                    let on = run.covers(self.cursor);
                    CodeChangeRow { line: start as u32 + 1, text, on }
                })
                .collect(),
            note: shown.and_then(|f| f.note.clone()),
            file_loading: self.file_asked.is_some(),
            jump: self.jump.as_ref().map(|j| {
                let m = j.matches(&self.files);
                CodeJump {
                    query: j.query().to_owned(),
                    at: m.lit,
                    matches: m.found.into_iter().map(|i| self.files[i].clone()).collect(),
                }
            }),
            search: self.search.as_ref().map(search_page),
            back: self.back.as_ref().map(|b| b.label.clone()),
            pick,
            comment,
            failed: self.failed.clone(),
        }
    }
}

/// A change in the file read, as `runs` finds it.
enum Run {
    Added { start: usize, len: usize },
    Removed { before: usize, count: u32 },
}

impl Run {
    fn start(&self) -> usize {
        match *self {
            Run::Added { start, .. } => start,
            Run::Removed { before, .. } => before,
        }
    }

    fn covers(&self, line: usize) -> bool {
        match *self {
            Run::Added { start, len } => (start..start + len).contains(&line),
            Run::Removed { before, .. } => line == before,
        }
    }
}

/// The search as the page draws it: its results grouped by file, each file
/// folded or open.
fn search_page(s: &Search) -> CodeSearch {
    let current = s.current();
    let matches = s.found.iter().flat_map(|f| f.matches.iter()).enumerate();
    let mut files: Vec<CodeHitFile> = Vec::new();
    for (index, m) in matches {
        if files.last().is_none_or(|f| f.path != m.path) {
            let (dir, name) = match m.path.rfind('/') {
                Some(at) => (m.path[..=at].to_owned(), m.path[at + 1..].to_owned()),
                None => (String::new(), m.path.clone()),
            };
            files.push(CodeHitFile {
                open: !s.folded.contains(&m.path),
                path: m.path.clone(),
                dir,
                name,
                count: 0,
                hits: Vec::new(),
            });
        }
        let file = files.last_mut().expect("pushed above");
        file.count += 1;
        file.hits.push(CodeHit { index, line: m.line, parts: hit_parts(&m.text, &m.ranges), on: current && index == s.at });
    }
    CodeSearch {
        query: s.query.clone(),
        glob: s.options.glob.clone(),
        regex: s.options.regex,
        case: s.options.case,
        word: s.options.word,
        current,
        loading: s.asked.is_some(),
        error: s.error.clone(),
        total: s.found.as_ref().map_or(0, |f| f.matches.len()),
        truncated: s.found.as_ref().is_some_and(|f| f.truncated),
        files,
    }
}

/// A matched line as `[text, matched]` runs, its leading indent dropped and
/// cut with an ellipsis where it runs long before its first match, so that
/// match is in sight. A range off the line's character boundaries is not
/// trusted.
fn hit_parts(text: &str, ranges: &[(u32, u32)]) -> Vec<(String, bool)> {
    let ranges: Vec<(usize, usize)> = ranges
        .iter()
        .map(|&(a, b)| (a as usize, b as usize))
        .filter(|&(a, b)| a < b && b <= text.len() && text.is_char_boundary(a) && text.is_char_boundary(b))
        .collect();
    let lead = text.len() - text.trim_start().len();
    let first = ranges.first().map_or(lead, |r| r.0);
    let from = if first.saturating_sub(lead) > 32 {
        let mut at = first - 24;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    } else {
        lead.min(first)
    };
    let mut parts = Vec::new();
    if from > lead {
        parts.push(("…".to_owned(), false));
    }
    let mut at = from;
    for (a, b) in ranges.into_iter().filter(|&(a, _)| a >= from) {
        if a > at {
            parts.push((text[at..a].to_owned(), false));
        }
        parts.push((text[a..b].to_owned(), true));
        at = b;
    }
    if at < text.len() {
        parts.push((text[at..].to_owned(), false));
    }
    parts
}

/// A tag query's word for what a name is, as short as the outline shows it.
fn short_kind(kind: &str) -> &'static str {
    match kind {
        "function" | "method" => "fn",
        "class" => "type",
        "interface" => "iface",
        "module" => "mod",
        "macro" => "macro",
        "constant" => "const",
        _ => "def",
    }
}

/// What a file is written in, by its extension, as the file header names it.
fn language(path: &str) -> Option<&'static str> {
    let (_, ext) = path.rsplit('/').next()?.rsplit_once('.')?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "rs" => "Rust",
        "ts" | "mts" | "cts" => "TypeScript",
        "tsx" => "TSX",
        "js" | "mjs" | "cjs" | "jsx" => "JavaScript",
        "py" | "pyi" => "Python",
        "cs" => "C#",
        "css" => "CSS",
        "html" => "HTML",
        "md" | "markdown" => "Markdown",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        "json" | "jsonc" => "JSON",
        "sh" => "Shell",
        "ps1" => "PowerShell",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane, project, repository};
    use argus_protocol::{CodeLine, CodeMatch, CodeSymbol, PaneKind, PaneStatus, Removed, RequestRefusal};

    fn stage(path: Option<&str>, line: Option<u32>) -> (CodeStage, Vec<ClientMsg>) {
        let mut ids = 0;
        CodeStage::open(
            &checkout(100, "main", true, Vec::new()),
            CodeVersion::WorkingTree,
            path.map(str::to_owned),
            line,
            None,
            || {
                ids += 1;
                ids
            },
        )
    }

    fn tree(request_id: u64) -> CodeTree {
        CodeTree {
            request_id,
            checkout: CheckoutId(100),
            version: CodeVersion::WorkingTree,
            files: vec!["README.md".into(), "src/a.rs".into(), "src/b.rs".into()],
            changes: vec![CodeChange { path: "src/a.rs".into(), added: 2, removed: 1 }],
        }
    }

    fn line(text: &str, added: bool) -> CodeLine {
        CodeLine { text: text.into(), spans: Vec::new(), added }
    }

    fn file(request_id: u64, path: &str) -> CodeFile {
        CodeFile {
            request_id,
            checkout: CheckoutId(100),
            path: path.into(),
            version: CodeVersion::WorkingTree,
            lines: vec![line("fn a() {", false), line("    new()", true), line("    more()", true), line("}", false), line("fn b() {}", false)],
            removed: vec![Removed { before: 2, count: 1 }, Removed { before: 6, count: 2 }],
            symbols: vec![
                CodeSymbol { name: "a".into(), kind: "function".into(), line: 1 },
                CodeSymbol { name: "b".into(), kind: "function".into(), line: 5 },
            ],
            note: None,
        }
    }

    fn reading(path: &str, line: Option<u32>) -> CodeStage {
        let (mut s, _) = stage(Some(path), line);
        s.receive_tree(tree(1));
        s.receive_file(file(2, path));
        s
    }

    #[test]
    fn opening_at_a_file_asks_for_the_version_and_the_file_and_lands_on_the_line() {
        let (mut s, sent) = stage(Some("src/a.rs"), Some(3));
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::ListCode { request_id: 1, .. }, ClientMsg::ReadCode { request_id: 2, path, .. }] if path == "src/a.rs"
        ));
        assert!(s.receive_tree(tree(1)));
        assert!(s.receive_file(file(2, "src/a.rs")));
        let page = s.page(None);
        assert_eq!((page.at, page.lines.iter().position(|r| r.cursor)), (3, Some(2)));
        let names: Vec<(&str, bool)> = page.tree.iter().map(|r| (r.name.as_str(), r.on)).collect();
        assert_eq!(names, [("README.md", false), ("src", false), ("a.rs", true), ("b.rs", false)], "its folder opened");
        assert_eq!((page.added, page.removed, page.changed, page.language), (2, 1, 1, Some("Rust")));
        assert_eq!(page.version, None, "the working tree goes without saying");
    }

    #[test]
    fn an_answer_to_an_older_ask_is_dropped() {
        let (mut s, _) = stage(Some("src/a.rs"), None);
        s.receive_tree(tree(1));
        s.click(CodeClick::Tree { path: "src/b.rs".into() }, || 9);
        assert!(!s.receive_file(file(2, "src/a.rs")));
        assert!(s.page(None).file_loading);
        assert!(s.receive_file(file(9, "src/b.rs")));
    }

    #[test]
    fn removed_lines_are_counted_above_the_line_after_them() {
        let page = reading("src/a.rs", None).page(None);
        assert_eq!(page.lines[1].removed_before, 1);
        assert_eq!(page.removed_at_end, 2, "after the last line");
    }

    #[test]
    fn the_ruler_marks_each_change_where_it_is_in_the_whole_file() {
        let page = reading("src/a.rs", None).page(None);
        let marks: Vec<(u32, u32, &str)> = page.ruler.iter().map(|m| (m.line, m.span, m.kind)).collect();
        assert_eq!(marks, [(2, 2, "added"), (5, 1, "removed")], "a removal past the end sits on the last line");
    }

    #[test]
    fn the_changes_in_the_file_are_listed_with_the_cursors_marked() {
        let mut s = reading("src/a.rs", Some(3));
        let rows: Vec<(u32, String, bool)> = s.page(None).changes_here.into_iter().map(|c| (c.line, c.text, c.on)).collect();
        assert_eq!(rows, [(2, "new()".into(), true), (6, "− 2 lines removed".into(), false)]);
        s.click(CodeClick::Go { line: 1 }, || 0);
        assert!(s.page(None).changes_here.iter().all(|c| !c.on));
    }

    #[test]
    fn the_change_keys_step_between_changes() {
        let mut s = reading("src/a.rs", None);
        s.next_change(true);
        assert_eq!(s.cursor, 1, "the run at line 2, where a line was also removed");
        s.next_change(true);
        assert_eq!(s.cursor, 4, "the removal past the end, held to the last line");
        s.next_change(false);
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn the_outline_is_short_marks_where_the_cursor_is_and_what_changed() {
        let mut s = reading("src/a.rs", Some(4));
        let rows: Vec<(&str, bool, bool)> = s.page(None).symbols.iter().map(|r| (r.kind, r.on, r.changed)).collect();
        assert_eq!(rows, [("fn", true, true), ("fn", false, false)]);
        s.click(CodeClick::Go { line: 5 }, || 0);
        assert_eq!(s.page(None).symbols.iter().map(|r| r.on).collect::<Vec<_>>(), [false, true]);
    }

    #[test]
    fn the_outline_can_be_put_away_and_brought_back() {
        let mut s = reading("src/a.rs", None);
        assert!(s.page(None).outline);
        s.click(CodeClick::Outline, || 0);
        assert!(!s.page(None).outline);
        s.toggle_outline();
        assert!(s.page(None).outline);
    }

    #[test]
    fn escape_closes_what_is_over_the_file_one_thing_at_a_time_and_never_the_stage() {
        let mut s = reading("src/a.rs", None);
        s.toggle_pick();
        s.open_search();
        assert!(s.escape() && s.page(None).search.is_none());
        assert!(s.escape() && s.page(None).pick.is_none(), "then the range");
        assert!(!s.escape(), "nothing left: the file stays");
        assert_eq!(s.page(None).path.as_deref(), Some("src/a.rs"));
    }

    #[test]
    fn a_folder_click_opens_and_closes_it_and_a_file_click_reads_it() {
        let (mut s, _) = stage(None, None);
        s.receive_tree(tree(1));
        assert_eq!(s.page(None).tree.len(), 2, "README.md and a closed src");
        assert!(s.click(CodeClick::Tree { path: "src".into() }, || 5).is_empty());
        assert_eq!(s.page(None).tree.len(), 4);
        assert!(matches!(s.click(CodeClick::Tree { path: "src/b.rs".into() }, || 5).as_slice(), [ClientMsg::ReadCode { .. }]));
        s.click(CodeClick::Tree { path: "src".into() }, || 6);
        assert_eq!(s.page(None).tree.len(), 2);
    }

    #[test]
    fn changed_only_lists_what_this_version_changed() {
        let (mut s, _) = stage(None, None);
        s.receive_tree(tree(1));
        s.click(CodeClick::ChangedOnly { only: true }, || 0);
        let names: Vec<String> = s.page(None).tree.into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["src", "a.rs"]);
    }

    #[test]
    fn a_version_other_than_the_working_tree_is_named() {
        let mut ids = 0;
        let (s, _) = CodeStage::open(
            &checkout(100, "main", true, Vec::new()),
            CodeVersion::Commit("abc1234def".into()),
            None,
            None,
            None,
            || {
                ids += 1;
                ids
            },
        );
        assert_eq!(s.page(None).version.as_deref(), Some("abc1234"));
    }

    #[test]
    fn go_to_file_reads_the_best_match() {
        let (mut s, _) = stage(None, None);
        s.receive_tree(tree(1));
        s.open_jump();
        s.jump(ReviewJumpGesture::Query { text: "b.rs".into() }, || 0);
        assert_eq!(s.page(None).jump.unwrap().matches[0], "src/b.rs");
        let sent = s.jump(ReviewJumpGesture::Choose, || 7);
        assert!(matches!(sent.as_slice(), [ClientMsg::ReadCode { request_id: 7, path, .. }] if path == "src/b.rs"));
        assert!(s.page(None).jump.is_none());
    }

    #[test]
    fn go_to_file_lights_only_a_match_it_shows() {
        let (mut s, _) = stage(None, None);
        let mut many = tree(1);
        many.files = (0..60).map(|i| format!("src/f{i:02}.rs")).collect();
        s.receive_tree(many);
        s.open_jump();
        s.jump(ReviewJumpGesture::Step { by: 55 }, || 0);
        let jump = s.page(None).jump.unwrap();
        assert_eq!((jump.matches.len(), jump.at), (50, Some(49)));
        let sent = s.jump(ReviewJumpGesture::Choose, || 7);
        assert!(matches!(sent.as_slice(), [ClientMsg::ReadCode { path, .. }] if path == "src/f49.rs"), "the row lit is the one read");
    }

    fn found(request_id: u64, query: &str, options: SearchOptions) -> CodeMatches {
        let hit = |path: &str, line, text: &str, ranges| CodeMatch { path: path.into(), line, text: text.into(), ranges };
        CodeMatches {
            request_id,
            checkout: CheckoutId(100),
            query: query.into(),
            options,
            matches: vec![
                hit("src/a.rs", 3, "    more()", vec![(4, 8)]),
                hit("src/a.rs", 5, "fn b() { more(); more(); }", vec![(9, 13), (17, 21)]),
                hit("src/b.rs", 1, "more", vec![(0, 4)]),
            ],
            truncated: false,
        }
    }

    #[test]
    fn enter_searches_with_the_options_and_enter_again_reads_the_match() {
        let mut s = reading("src/a.rs", None);
        s.open_search();
        s.search(ReviewJumpGesture::Query { text: "more".into() }, || 0);
        s.click(CodeClick::SearchOption { option: "word".into() }, || 0);
        s.set_glob("src/**".into());
        let sent = s.search(ReviewJumpGesture::Choose, || 20);
        let [ClientMsg::SearchCode { request_id: 20, query, options, .. }] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!((query.as_str(), options.word, options.glob.as_str()), ("more", true, "src/**"));
        let options = options.clone();
        assert!(s.page(None).search.unwrap().loading);
        assert!(s.receive_matches(found(20, "more", options)));

        let search = s.page(None).search.unwrap();
        assert_eq!((search.total, search.files.len(), search.files[0].count), (3, 2, 2));
        let parts = &search.files[0].hits[1].parts;
        assert_eq!(parts.iter().filter(|p| p.1).count(), 2, "every match on the line: {parts:?}");
        let marks: Vec<&str> = s.page(None).ruler.iter().map(|m| m.kind).collect();
        assert_eq!(marks.iter().filter(|k| **k == "hit").count(), 2, "this file's matches are on its ruler");

        s.search(ReviewJumpGesture::Step { by: 1 }, || 0);
        assert!(s.search(ReviewJumpGesture::Choose, || 21).is_empty(), "already reading src/a.rs");
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn an_option_flipped_on_a_search_made_searches_again() {
        let mut s = reading("src/a.rs", None);
        s.open_search();
        s.search(ReviewJumpGesture::Query { text: "more".into() }, || 0);
        s.search(ReviewJumpGesture::Choose, || 20);
        s.receive_matches(found(20, "more", SearchOptions::default()));
        let sent = s.click(CodeClick::SearchOption { option: "case".into() }, || 21);
        assert!(matches!(sent.as_slice(), [ClientMsg::SearchCode { options, .. }] if options.case));
        assert!(!s.page(None).search.unwrap().current, "the matches held are for the old options");
    }

    #[test]
    fn a_search_that_cannot_run_says_why_in_the_search() {
        let mut s = reading("src/a.rs", None);
        s.open_search();
        s.search(ReviewJumpGesture::Query { text: "(".into() }, || 0);
        s.search(ReviewJumpGesture::Choose, || 20);
        assert!(s.failed(20, "“(” is not a regular expression this can read".into()));
        let page = s.page(None);
        assert!(page.search.unwrap().error.is_some());
        assert!(page.failed.is_none() && !page.lines.is_empty(), "the file stays");
    }

    #[test]
    fn a_files_matches_can_be_folded() {
        let mut s = reading("src/a.rs", None);
        s.open_search();
        s.search(ReviewJumpGesture::Query { text: "more".into() }, || 0);
        s.search(ReviewJumpGesture::Choose, || 20);
        s.receive_matches(found(20, "more", SearchOptions::default()));
        s.click(CodeClick::HitFile { path: "src/a.rs".into() }, || 0);
        assert!(!s.page(None).search.unwrap().files[0].open);
    }

    #[test]
    fn a_long_line_is_cut_so_its_first_match_is_in_sight() {
        let text = format!("{}needle", "x".repeat(60));
        let parts = hit_parts(&text, &[(60, 66)]);
        assert_eq!(parts[0], ("…".to_owned(), false));
        assert!(parts.iter().any(|p| p == &("needle".to_owned(), true)));
        assert_eq!(hit_parts("    a b", &[(4, 5)]), [("a".to_owned(), true), (" b".to_owned(), false)], "indent dropped");
    }

    #[test]
    fn a_file_that_will_not_read_says_why_where_its_lines_would_be() {
        let (mut s, _) = stage(Some("gone.rs"), None);
        assert!(s.failed(2, "gone.rs is not in this version".into()));
        let page = s.page(None);
        assert_eq!(page.failed.as_deref(), Some("gone.rs is not in this version"));
        assert!(page.lines.is_empty() && !page.file_loading);
        assert!(!s.failed(99, "not mine".into()));
    }

    #[test]
    fn opened_from_a_review_it_says_where_b_goes_back_to() {
        let mut ids = 0;
        let (s, _) = CodeStage::open(
            &checkout(100, "main", true, Vec::new()),
            CodeVersion::WorkingTree,
            Some("src/brief.rs".into()),
            Some(39),
            Some(Back { label: "review · brief.rs:39".into() }),
            || {
                ids += 1;
                ids
            },
        );
        assert_eq!(s.page(None).back.as_deref(), Some("review · brief.rs:39"));
    }

    fn with_agent() -> CheckoutInfo {
        checkout(100, "main", true, vec![pane(7, PaneKind::Agent, "claude", PaneStatus::Working)])
    }

    #[test]
    fn a_picked_range_is_commented_on_through_the_reviews_path() {
        let mut s = reading("src/a.rs", Some(2));
        s.toggle_pick();
        s.move_by(1);
        let co = with_agent();
        assert_eq!(s.page(Some(&co)).pick.as_deref(), Some("lines 2–3"));
        s.compose(Some(&co)).unwrap();
        let ClientMsg::ReviewComment { recipient, anchor, body, .. } = s.comment(Some(&co), " why two? ", 3).unwrap() else {
            panic!()
        };
        assert_eq!((recipient, body.as_str()), (PaneId(7), "why two?"));
        assert_eq!(anchor.base, ReviewBase::Unstaged, "the working tree is unstaged work");
        assert_eq!((anchor.new_start, anchor.new_end), (Some(2), Some(3)));
        assert_eq!(anchor.text, ["+    new()", "+    more()"]);
        s.commented(3);
        let page = s.page(Some(&co));
        assert!(page.comment.is_none() && page.pick.is_none() && page.lines.iter().all(|r| !r.picked));
    }

    #[test]
    fn a_comment_needs_a_file_and_an_agent() {
        let (mut s, _) = stage(Some("src/a.rs"), None);
        assert_eq!(s.compose(Some(&with_agent())), Err("pick a line to comment on"));
        s.receive_file(file(2, "src/a.rs"));
        assert_eq!(s.compose(Some(&checkout(100, "main", true, Vec::new()))), Err("no agent running in this checkout"));
        assert!(s.page(None).comment.is_none());
    }

    #[test]
    fn a_comment_on_a_commit_records_the_commit() {
        let mut ids = 0;
        let (mut s, _) = CodeStage::open(
            &with_agent(),
            CodeVersion::Commit("abc123".into()),
            Some("src/a.rs".into()),
            None,
            None,
            || {
                ids += 1;
                ids
            },
        );
        let mut read = file(2, "src/a.rs");
        read.version = CodeVersion::Commit("abc123".into());
        s.receive_file(read);
        s.compose(Some(&with_agent())).unwrap();
        let ClientMsg::ReviewComment { anchor, .. } = s.comment(Some(&with_agent()), "x", 1).unwrap() else {
            panic!()
        };
        assert_eq!((anchor.base, anchor.commit.as_deref()), (ReviewBase::Commit, Some("abc123")));
    }

    #[test]
    fn a_comment_to_an_agent_that_has_exited_is_refused_as_in_a_review() {
        let mut s = reading("src/a.rs", None);
        let mut co = checkout(100, "main", true, vec![
            pane(7, PaneKind::Agent, "claude", PaneStatus::Working),
            pane(8, PaneKind::Agent, "codex", PaneStatus::Working),
        ]);
        s.compose(Some(&co)).unwrap();
        s.click(CodeClick::Recipient { pane: 8 }, || 0);
        co.panes[1].status = PaneStatus::Exited { code: Some(0) };
        assert_eq!(s.comment(Some(&co), "why?", 1).err(), Some("that agent is no longer running"));
        assert_eq!(s.page(Some(&co)).comment.expect("the box stays open").to, None);
    }

    #[test]
    fn a_comment_saved_from_the_review_leaves_the_code_stages_box_open() {
        let mut s = reading("src/a.rs", None);
        s.toggle_pick();
        s.compose(Some(&with_agent())).unwrap();
        s.commented(4);
        let page = s.page(Some(&with_agent()));
        assert!(page.comment.is_some() && page.pick.is_some());
    }

    #[test]
    fn a_save_of_another_comment_leaves_the_code_stages_box_open() {
        let mut s = reading("src/a.rs", None);
        s.toggle_pick();
        s.compose(Some(&with_agent())).unwrap();
        s.comment(Some(&with_agent()), "why?", 3).unwrap();
        s.commented(4);
        let page = s.page(Some(&with_agent()));
        assert!(page.comment.is_some() && page.pick.is_some(), "another box's save");
        s.commented(3);
        let page = s.page(Some(&with_agent()));
        assert!(page.comment.is_none() && page.pick.is_none());
    }

    #[test]
    fn a_shift_click_extends_the_range_from_the_cursor() {
        let mut s = reading("src/a.rs", Some(1));
        s.click(CodeClick::Line { line: 4, extend: true }, || 0);
        assert_eq!(s.page(None).pick.as_deref(), Some("lines 1–4"));
        s.click(CodeClick::Line { line: 2, extend: false }, || 0);
        assert!(s.page(None).pick.is_none());
    }

    fn holding(stage: CodeStage) -> Code {
        Code { stage: Some(stage), sent: Sent::default() }
    }

    /// A tree holding `co` alone.
    fn held(co: CheckoutInfo) -> Vec<ProjectInfo> {
        vec![project(1, "src", vec![repository(10, "argus", None, vec![co])])]
    }

    /// `gesture` on `code`, with what it sent.
    fn gesture(code: &mut Code, gesture: CodeGesture, tree: &[ProjectInfo]) -> (Then, Vec<ClientMsg>) {
        let mut out = Vec::new();
        let then = code.gesture(gesture, tree, || 9, &mut out);
        (then, out)
    }

    fn key(action: Action) -> CodeGesture {
        CodeGesture::Action { action }
    }

    #[test]
    fn b_goes_back_only_to_the_dialog_the_stage_was_opened_from() {
        let mut code = holding(reading("src/a.rs", None));
        assert_eq!(gesture(&mut code, key(Action::GoBack), &[]).0, Then::Nothing);
        code.stage.as_mut().unwrap().back = Some(Back { label: "history".into() });
        assert_eq!(gesture(&mut code, key(Action::GoBack), &[]).0, Then::Back);
        code.forget_back();
        assert_eq!(gesture(&mut code, key(Action::GoBack), &[]).0, Then::Nothing, "the dialog is back up");
    }

    #[test]
    fn the_back_button_is_the_windows_to_take_even_with_no_stage_open() {
        let back = CodeGesture::Click { target: CodeClick::Back };
        let (then, sent) = gesture(&mut Code::default(), back, &[]);
        assert!(then == Then::Back && sent.is_empty());
    }

    #[test]
    fn e_hands_the_window_the_file_and_line_under_the_cursor() {
        let mut code = holding(reading("src/a.rs", Some(3)));
        let edit = Then::Edit { checkout: CheckoutId(100), path: "src/a.rs".into(), line: Some(3) };
        assert_eq!(gesture(&mut code, key(Action::OpenEditor), &[]).0, edit);
        let mut none = holding(stage(None, None).0);
        assert_eq!(gesture(&mut none, key(Action::OpenEditor), &[]).0, Then::Say("no file to open".into()));
    }

    #[test]
    fn c_says_why_a_comment_cannot_be_written() {
        let mut code = holding(reading("src/a.rs", None));
        let projects = held(checkout(100, "main", true, Vec::new()));
        let said = gesture(&mut code, key(Action::Comment), &projects).0;
        assert_eq!(said, Then::Say("no agent running in this checkout".into()));
        assert!(code.page(&projects).unwrap().comment.is_none(), "not composing");
    }

    #[test]
    fn an_empty_comment_is_refused_and_nothing_is_sent() {
        let mut code = holding(reading("src/a.rs", None));
        let projects = held(with_agent());
        assert_eq!(gesture(&mut code, key(Action::Comment), &projects).0, Then::Nothing);
        let (said, sent) = gesture(&mut code, CodeGesture::Comment { body: Some("  ".into()) }, &projects);
        assert_eq!(said, Then::Say("the comment is empty".into()));
        assert!(sent.is_empty());
    }

    #[test]
    fn a_gesture_with_no_stage_open_sends_nothing() {
        let mut code = Code::default();
        let projects = held(with_agent());
        for g in [
            key(Action::Next),
            key(Action::Refresh),
            key(Action::GoBack),
            CodeGesture::Click { target: CodeClick::Tree { path: "src/a.rs".into() } },
            CodeGesture::Search { gesture: ReviewJumpGesture::Choose },
            CodeGesture::Glob { glob: "src/**".into() },
            CodeGesture::Comment { body: Some("why".into()) },
        ] {
            let (then, sent) = gesture(&mut code, g, &projects);
            assert!(then == Then::Nothing && sent.is_empty(), "{then:?}");
        }
        assert!(code.take(true, &projects).is_none());
    }

    #[test]
    fn the_daemons_code_answers_are_the_stages_and_a_saved_comment_is_the_reviews_too() {
        let mut code = holding(stage(Some("src/a.rs"), None).0);
        let projects = held(with_agent());
        assert!(code.receive(ServerMsg::CodeTree(tree(1))).is_none());
        assert!(!code.page(&projects).unwrap().tree.is_empty(), "the tree is held");
        assert!(code.receive(ServerMsg::Code(Box::new(file(2, "src/a.rs")))).is_none());
        gesture(&mut code, key(Action::Comment), &projects);
        let (_, sent) = gesture(&mut code, CodeGesture::Comment { body: Some("why".into()) }, &projects);
        assert!(matches!(sent.as_slice(), [ClientMsg::ReviewComment { .. }]));

        let other = code.receive(ServerMsg::ReviewCommentSaved { id: 1, delivered: true, request_id: 8 });
        assert!(matches!(other, Some(ServerMsg::ReviewCommentSaved { .. })), "the review's save is given back");
        assert!(code.page(&projects).unwrap().comment.is_some(), "and leaves this box open");
        let saved = code.receive(ServerMsg::ReviewCommentSaved { id: 2, delivered: true, request_id: 9 });
        assert!(matches!(saved, Some(ServerMsg::ReviewCommentSaved { .. })), "the review says it");
        assert!(code.page(&projects).unwrap().comment.is_none(), "the box closed");
        assert!(code.receive(ServerMsg::Tree(Vec::new())).is_some());
    }

    #[test]
    fn code_answers_with_no_stage_open_are_dropped() {
        let mut code = Code::default();
        assert!(code.receive(ServerMsg::CodeTree(tree(1))).is_none());
        assert!(code.receive(ServerMsg::Code(Box::new(file(2, "src/a.rs")))).is_none());
        assert!(code.receive(ServerMsg::CodeMatches(found(3, "a", SearchOptions::default()))).is_none());
    }

    #[test]
    fn a_refusal_of_the_stages_read_is_its_own_and_any_other_is_given_back() {
        let refused = |request_id| {
            ServerMsg::Refused(RequestRefusal { request_id, reason: "gone.rs is not in this version".into(), read: true })
        };
        let mut code = holding(stage(Some("gone.rs"), None).0);
        assert!(code.receive(refused(2)).is_none(), "the file it asked for");
        assert_eq!(code.page(&[]).unwrap().failed.as_deref(), Some("gone.rs is not in this version"));
        assert!(matches!(code.receive(refused(99)), Some(ServerMsg::Refused(r)) if r.request_id == 99));
        assert!(Code::default().receive(refused(2)).is_some(), "with no stage open");
    }

    #[test]
    fn the_page_is_sent_only_while_the_stage_is_up_and_never_taken_back() {
        let mut code = holding(reading("src/a.rs", None));
        assert!(matches!(code.take(true, &[]), Some(Event::Code(_))));
        assert!(code.take(false, &[]).is_none(), "no closing is sent");
        assert!(code.take(false, &[]).is_none());
        assert!(code.take(true, &[]).is_none(), "unchanged");
    }

    #[test]
    fn a_reloaded_page_hears_the_stage_again_once_it_is_up() {
        let mut code = holding(reading("src/a.rs", None));
        code.take(true, &[]);
        code.reload();
        assert!(code.take(false, &[]).is_none(), "not while it is down");
        assert!(matches!(code.take(true, &[]), Some(Event::Code(_))));
    }
}
