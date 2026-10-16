//! What the Code stage holds — the checkout it reads, the version, that
//! version's files and the ones it changed, the folders open, the file read
//! and the cursor in it, the go-to-file field, the search, and the review it
//! was opened from — what its gestures ask of the daemon, and the page it
//! draws. What the daemon read is shown as it said it; nothing is
//! predicted, so a file that will not read says so where its lines would be.

use std::collections::HashSet;

use argus_protocol::fuzzy::Fuzzy;
use argus_protocol::{
    CheckoutId, CheckoutInfo, ClientMsg, CodeChange, CodeFile, CodeMatch, CodeMatches, CodeTree, CodeVersion,
};

use crate::code_tree::{self, folders_of};
use crate::page::{
    CodeClick, CodeJump, CodeMatchRow, CodeOutlineRow, CodePage, CodeRow, CodeSearch, CodeTreeRow, ReviewJumpGesture,
};
use crate::review::parts;

/// Where Escape goes back to: the review the stage was opened from, at the
/// line it was on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Back {
    pub path: String,
    pub line: u32,
}

#[derive(Default)]
struct Jump {
    query: String,
    at: usize,
}

#[derive(Default)]
struct Search {
    query: String,
    asked: Option<u64>,
    found: Option<CodeMatches>,
    at: usize,
}

pub struct CodeStage {
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
    /// The line to put the cursor on once the file asked for arrives.
    land: Option<u32>,
    jump: Option<Jump>,
    search: Option<Search>,
    back: Option<Back>,
    failed: Option<String>,
}

impl CodeStage {
    /// `checkout` at `version`, reading `path` at `line` when one is given.
    pub fn open(
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
            land: None,
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

    pub fn checkout(&self) -> CheckoutId {
        self.checkout
    }

    pub fn back(&self) -> Option<&Back> {
        self.back.as_ref()
    }

    /// The review it was opened from is back up: Escape now only leaves.
    pub fn forget_back(&mut self) {
        self.back = None;
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
        self.failed = None;
        self.path = Some(path.clone());
        ClientMsg::ReadCode { request_id, checkout: self.checkout, path, version: self.version.clone() }
    }

    /// Reads the version again: the agent kept working.
    pub fn refresh(&mut self, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        let mut msgs = vec![self.ask_tree(id())];
        if let Some(path) = self.path.clone() {
            let line = Some(self.cursor as u32 + 1);
            msgs.push(self.read(path, line, id()));
        }
        msgs
    }

    pub fn receive_tree(&mut self, tree: CodeTree) -> bool {
        if tree.checkout != self.checkout || self.tree_asked != Some(tree.request_id) {
            return false;
        }
        self.tree_asked = None;
        self.files = tree.files;
        self.changes = tree.changes;
        true
    }

    pub fn receive_file(&mut self, file: CodeFile) -> bool {
        if file.checkout != self.checkout || self.file_asked != Some(file.request_id) {
            return false;
        }
        self.file_asked = None;
        let last = file.lines.len().saturating_sub(1);
        self.cursor = self.land.take().map_or(0, |line| (line as usize).saturating_sub(1).min(last));
        self.file = Some(Box::new(file));
        true
    }

    pub fn receive_matches(&mut self, found: CodeMatches) -> bool {
        let Some(search) = self.search.as_mut() else {
            return false;
        };
        if found.checkout != self.checkout || search.asked != Some(found.request_id) {
            return false;
        }
        search.asked = None;
        search.at = 0;
        search.found = Some(found);
        true
    }

    /// A request of this stage's that could not be answered. A file that
    /// would not read leaves nothing of an older one on screen.
    pub fn failed(&mut self, checkout: CheckoutId, request_id: u64, message: String) -> bool {
        if checkout != self.checkout {
            return false;
        }
        if self.file_asked == Some(request_id) {
            self.file_asked = None;
            self.file = None;
        } else if self.tree_asked == Some(request_id) {
            self.tree_asked = None;
        } else if let Some(search) = self.search.as_mut().filter(|s| s.asked == Some(request_id)) {
            search.asked = None;
        } else {
            return false;
        }
        self.failed = Some(message);
        true
    }

    pub fn click(&mut self, target: CodeClick, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        match target {
            CodeClick::Tree { path } => {
                if self.files.binary_search(&path).is_ok() {
                    return vec![self.read(path, None, id())];
                }
                if !self.open.remove(&path) {
                    self.open.insert(path);
                }
            }
            CodeClick::Line { line } | CodeClick::Symbol { line } => self.go_to(line),
            CodeClick::Version { version } => {
                let version = match version.as_str() {
                    "index" => CodeVersion::Index,
                    "head" => CodeVersion::Head,
                    _ => CodeVersion::WorkingTree,
                };
                if version != self.version {
                    self.version = version;
                    return self.refresh(id);
                }
            }
            CodeClick::ChangedOnly { only } => self.changed_only = only,
            CodeClick::Match { index } => {
                if let Some(search) = &mut self.search {
                    search.at = index;
                }
                return self.choose_search(id);
            }
            // The session takes this: it reopens the review.
            CodeClick::Back => {}
        }
        Vec::new()
    }

    fn lines(&self) -> usize {
        self.file.as_ref().map_or(0, |f| f.lines.len())
    }

    fn go_to(&mut self, line: u32) {
        self.cursor = (line as usize).saturating_sub(1).min(self.lines().saturating_sub(1));
    }

    pub fn move_by(&mut self, delta: isize) {
        let last = self.lines().saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, last.max(0)) as usize;
    }

    pub fn ends(&mut self, bottom: bool) {
        self.cursor = if bottom { self.lines().saturating_sub(1) } else { 0 };
    }

    /// `]` and `[`: the start of the next or previous run of changed lines,
    /// or a place lines were removed.
    pub fn next_change(&mut self, forward: bool) {
        let Some(file) = &self.file else {
            return;
        };
        let removed_before: HashSet<usize> = file.removed.iter().map(|r| r.before as usize - 1).collect();
        let starts = |i: usize| {
            let added = file.lines[i].added && (i == 0 || !file.lines[i - 1].added);
            added || removed_before.contains(&i)
        };
        let found = if forward {
            (self.cursor + 1..file.lines.len()).find(|&i| starts(i))
        } else {
            (0..self.cursor).rev().find(|&i| starts(i))
        };
        if let Some(i) = found {
            self.cursor = i;
        }
    }

    /// `E`: the file and line under the cursor, for the person's editor.
    pub fn editor_target(&self) -> Option<(CheckoutId, String, Option<u32>)> {
        let path = self.path.clone()?;
        Some((self.checkout, path, self.file.as_ref().map(|_| self.cursor as u32 + 1)))
    }

    // ---- go to a file ----------------------------------------------------

    pub fn open_jump(&mut self) {
        self.search = None;
        self.jump = Some(Jump::default());
    }

    fn jump_matches(&self) -> Vec<usize> {
        match &self.jump {
            Some(jump) => Fuzzy::paths().filter(&jump.query, &self.files),
            None => Vec::new(),
        }
    }

    pub fn jump(&mut self, gesture: ReviewJumpGesture, id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        let last = self.jump_matches().len().saturating_sub(1);
        let Some(jump) = &mut self.jump else {
            return Vec::new();
        };
        match gesture {
            ReviewJumpGesture::Query { text } => {
                jump.query = text;
                jump.at = 0;
            }
            ReviewJumpGesture::Step { by } => jump.at = jump.at.saturating_add_signed(by).min(last),
            ReviewJumpGesture::Close => self.jump = None,
            ReviewJumpGesture::Choose => {
                let at = jump.at;
                let path = self.jump_matches().get(at).map(|&i| self.files[i].clone());
                self.jump = None;
                if let Some(path) = path {
                    let mut id = id;
                    return vec![self.read(path, None, id())];
                }
            }
        }
        Vec::new()
    }

    // ---- search ----------------------------------------------------------

    pub fn open_search(&mut self) {
        self.jump = None;
        if self.search.is_none() {
            self.search = Some(Search::default());
        }
    }

    /// The search field: typing changes the query, Enter searches for it —
    /// and on a query already searched, reads the match it is on.
    pub fn search(&mut self, gesture: ReviewJumpGesture, id: impl FnMut() -> u64) -> Vec<ClientMsg> {
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

    fn choose_search(&mut self, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        let checkout = self.checkout;
        let Some(search) = &mut self.search else {
            return Vec::new();
        };
        let searched = search.found.as_ref().is_some_and(|f| f.query == search.query);
        if !searched {
            if search.query.trim().is_empty() {
                return Vec::new();
            }
            let request_id = id();
            search.asked = Some(request_id);
            return vec![ClientMsg::SearchCode { request_id, checkout, query: search.query.clone() }];
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

    pub fn page(&self) -> CodePage {
        let shown = self.file.as_ref().filter(|f| Some(&f.path) == self.path.as_ref());
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
        let at_line = self.cursor as u32 + 1;
        let inside = shown.and_then(|f| f.symbols.iter().rposition(|s| s.line <= at_line));
        CodePage {
            open: true,
            checkout: self.checkout.0,
            name: self.name.clone(),
            branch: self.branch.clone(),
            version: match self.version {
                CodeVersion::WorkingTree => "working",
                CodeVersion::Index => "index",
                CodeVersion::Head => "head",
                CodeVersion::Commit(_) => "commit",
            },
            commit: match &self.version {
                CodeVersion::Commit(oid) => Some(oid.chars().take(7).collect()),
                _ => None,
            },
            changed_only: self.changed_only,
            changed: self.changes.len(),
            tree: code_tree::rows(&self.files, &self.changes, &self.open, self.changed_only)
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
            added: change.map_or(0, |c| c.added),
            removed: change.map_or(0, |c| c.removed),
            lines,
            removed_at_end,
            symbols: shown
                .map(|f| {
                    f.symbols
                        .iter()
                        .enumerate()
                        .map(|(i, s)| CodeOutlineRow {
                            name: s.name.clone(),
                            kind: s.kind.clone(),
                            line: s.line,
                            on: Some(i) == inside,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            note: shown.and_then(|f| f.note.clone()),
            file_loading: self.file_asked.is_some(),
            jump: self.jump.as_ref().map(|j| {
                let matches = self.jump_matches();
                CodeJump {
                    query: j.query.clone(),
                    at: (!matches.is_empty()).then_some(j.at),
                    matches: matches.into_iter().take(50).map(|i| self.files[i].clone()).collect(),
                }
            }),
            search: self.search.as_ref().map(|s| CodeSearch {
                query: s.query.clone(),
                searched: s.found.as_ref().map(|f| f.query.clone()),
                loading: s.asked.is_some(),
                matches: s.found.iter().flat_map(|f| f.matches.iter()).map(match_row).collect(),
                truncated: s.found.as_ref().is_some_and(|f| f.truncated),
                at: s.found.as_ref().filter(|f| !f.matches.is_empty()).map(|_| s.at),
            }),
            back: self.back.as_ref().map(|b| {
                let name = b.path.rsplit('/').next().unwrap_or(&b.path);
                format!("{name}:{}", b.line)
            }),
            failed: self.failed.clone(),
        }
    }
}

/// A match's line, cut so the match shows however long the line is.
fn match_row(m: &CodeMatch) -> CodeMatchRow {
    let (start, end) = (m.start as usize, m.end as usize);
    let fits = end <= m.text.len() && m.text.is_char_boundary(start) && m.text.is_char_boundary(end);
    let (before, hit, after) = if fits {
        (&m.text[..start], &m.text[start..end], &m.text[end..])
    } else {
        (m.text.as_str(), "", "")
    };
    let before = before.trim_start();
    let keep: String = before.chars().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect();
    CodeMatchRow {
        path: m.path.clone(),
        line: m.line,
        before: if keep.len() < before.len() { format!("…{keep}") } else { keep },
        hit: hit.to_owned(),
        after: after.chars().take(80).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::checkout;
    use argus_protocol::{CodeLine, CodeSymbol, Removed};

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

    #[test]
    fn opening_at_a_file_asks_for_the_version_and_the_file_and_lands_on_the_line() {
        let (mut s, sent) = stage(Some("src/a.rs"), Some(3));
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::ListCode { request_id: 1, .. }, ClientMsg::ReadCode { request_id: 2, path, .. }] if path == "src/a.rs"
        ));
        assert!(s.receive_tree(tree(1)));
        assert!(s.receive_file(file(2, "src/a.rs")));
        let page = s.page();
        assert_eq!(page.lines.iter().position(|r| r.cursor), Some(2));
        let names: Vec<(&str, bool)> = page.tree.iter().map(|r| (r.name.as_str(), r.on)).collect();
        assert_eq!(names, [("README.md", false), ("src", false), ("a.rs", true), ("b.rs", false)], "its folder opened");
        assert_eq!((page.added, page.removed, page.changed), (2, 1, 1));
    }

    #[test]
    fn an_answer_to_an_older_ask_is_dropped() {
        let (mut s, _) = stage(Some("src/a.rs"), None);
        s.receive_tree(tree(1));
        s.click(CodeClick::Tree { path: "src/b.rs".into() }, || 9);
        assert!(!s.receive_file(file(2, "src/a.rs")));
        assert!(s.page().file_loading);
        assert!(s.receive_file(file(9, "src/b.rs")));
    }

    #[test]
    fn removed_lines_are_counted_above_the_line_after_them() {
        let (mut s, _) = stage(Some("src/a.rs"), None);
        s.receive_file(file(2, "src/a.rs"));
        let page = s.page();
        assert_eq!(page.lines[1].removed_before, 1);
        assert_eq!(page.removed_at_end, 2, "after the last line");
    }

    #[test]
    fn the_change_keys_step_between_runs_of_changes() {
        let (mut s, _) = stage(Some("src/a.rs"), None);
        s.receive_file(file(2, "src/a.rs"));
        s.next_change(true);
        assert_eq!(s.cursor, 1, "the run starting at line 2, where a line was also removed");
        s.next_change(true);
        assert_eq!(s.cursor, 1, "no run after it starts inside the file");
        s.ends(true);
        s.next_change(false);
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn the_outline_marks_the_definition_the_cursor_is_in() {
        let (mut s, _) = stage(Some("src/a.rs"), Some(4));
        s.receive_file(file(2, "src/a.rs"));
        let on: Vec<bool> = s.page().symbols.iter().map(|r| r.on).collect();
        assert_eq!(on, [true, false]);
        s.click(CodeClick::Symbol { line: 5 }, || 0);
        assert_eq!(s.page().symbols.iter().map(|r| r.on).collect::<Vec<_>>(), [false, true]);
    }

    #[test]
    fn a_folder_click_opens_and_closes_it_and_a_file_click_reads_it() {
        let (mut s, _) = stage(None, None);
        s.receive_tree(tree(1));
        assert_eq!(s.page().tree.len(), 2, "README.md and a closed src");
        assert!(s.click(CodeClick::Tree { path: "src".into() }, || 5).is_empty());
        assert_eq!(s.page().tree.len(), 4);
        assert!(matches!(s.click(CodeClick::Tree { path: "src/b.rs".into() }, || 5).as_slice(), [ClientMsg::ReadCode { .. }]));
        s.click(CodeClick::Tree { path: "src".into() }, || 6);
        assert_eq!(s.page().tree.len(), 2);
    }

    #[test]
    fn changed_only_lists_what_this_version_changed() {
        let (mut s, _) = stage(None, None);
        s.receive_tree(tree(1));
        s.click(CodeClick::ChangedOnly { only: true }, || 0);
        let names: Vec<String> = s.page().tree.into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["src", "a.rs"]);
    }

    #[test]
    fn another_version_reads_the_tree_and_the_file_again() {
        let (mut s, _) = stage(Some("src/a.rs"), None);
        let mut ids = 10;
        let sent = s.click(CodeClick::Version { version: "head".into() }, || {
            ids += 1;
            ids
        });
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::ListCode { version: CodeVersion::Head, .. }, ClientMsg::ReadCode { version: CodeVersion::Head, .. }]
        ));
        assert_eq!(s.page().version, "head");
        assert!(s.click(CodeClick::Version { version: "head".into() }, || 0).is_empty(), "already there");
    }

    #[test]
    fn go_to_file_reads_the_best_match() {
        let (mut s, _) = stage(None, None);
        s.receive_tree(tree(1));
        s.open_jump();
        s.jump(ReviewJumpGesture::Query { text: "b.rs".into() }, || 0);
        assert_eq!(s.page().jump.unwrap().matches[0], "src/b.rs");
        let sent = s.jump(ReviewJumpGesture::Choose, || 7);
        assert!(matches!(sent.as_slice(), [ClientMsg::ReadCode { request_id: 7, path, .. }] if path == "src/b.rs"));
        assert!(s.page().jump.is_none());
    }

    #[test]
    fn enter_searches_and_enter_again_reads_the_match_at_its_line() {
        let (mut s, _) = stage(None, None);
        s.open_search();
        s.search(ReviewJumpGesture::Query { text: "more".into() }, || 0);
        let sent = s.search(ReviewJumpGesture::Choose, || 20);
        assert!(matches!(sent.as_slice(), [ClientMsg::SearchCode { request_id: 20, query, .. }] if query == "more"));
        assert!(s.page().search.unwrap().loading);
        let hit = CodeMatch { path: "src/a.rs".into(), line: 3, text: "    more()".into(), start: 4, end: 8 };
        assert!(s.receive_matches(CodeMatches {
            request_id: 20,
            checkout: CheckoutId(100),
            query: "more".into(),
            matches: vec![hit],
            truncated: false,
        }));
        let row = &s.page().search.unwrap().matches[0];
        assert_eq!((row.before.as_str(), row.hit.as_str(), row.after.as_str()), ("", "more", "()"));
        let sent = s.search(ReviewJumpGesture::Choose, || 21);
        assert!(matches!(sent.as_slice(), [ClientMsg::ReadCode { request_id: 21, .. }]));
        s.receive_file(file(21, "src/a.rs"));
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn a_file_that_will_not_read_says_why_where_its_lines_would_be() {
        let (mut s, _) = stage(Some("gone.rs"), None);
        assert!(s.failed(CheckoutId(100), 2, "gone.rs is not in this version".into()));
        let page = s.page();
        assert_eq!(page.failed.as_deref(), Some("gone.rs is not in this version"));
        assert!(page.lines.is_empty() && !page.file_loading);
        assert!(!s.failed(CheckoutId(100), 99, "not mine".into()));
    }

    #[test]
    fn opened_from_a_review_it_says_where_escape_goes_back_to() {
        let mut ids = 0;
        let (s, _) = CodeStage::open(
            &checkout(100, "main", true, Vec::new()),
            CodeVersion::WorkingTree,
            Some("src/brief.rs".into()),
            Some(39),
            Some(Back { path: "src/brief.rs".into(), line: 39 }),
            || {
                ids += 1;
                ids
            },
        );
        assert_eq!(s.page().back.as_deref(), Some("brief.rs:39"));
    }
}
