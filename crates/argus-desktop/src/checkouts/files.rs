//! The files panel of the Checkouts stage as the window holds it once open:
//! which checkout's folder it reads, the folders open, what each listing
//! said, the selected row, the mode and the filter, what its keys and
//! clicks ask of the daemon and leave to the window, and the page it draws.
//! Folders are read a level at a time, as they are opened, and the listings
//! are shown as the daemon sent them; nothing is predicted, so a folder
//! that will not read says so where its rows would be.

use std::collections::{BTreeSet, HashMap};

use argus_protocol::{CheckoutId, CheckoutInfo, ClientMsg, CheckoutDir, RequestRefusal};
use serde::Deserialize;

use crate::checkouts::files_page::{self, FileRow, FilesPage, Listing, Mode, View, REVEAL_LABEL};
use crate::keys::keymap::Action;

/// What the window knows of where the daemon is, which decides what the
/// panel may offer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Daemon {
    /// It listed `FILES`.
    pub lists_files: bool,
    /// It runs on this machine, whose file manager can show a folder.
    pub local: bool,
}

/// What a row is opened in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum OpenIn {
    Code,
    Editor,
}

/// What was done in the panel.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum FilesGesture {
    /// The panel's close button.
    Close,
    /// A key of the panel, or a button standing for one.
    Action { action: Action },
    /// A row clicked: a folder opens or closes, a file is only selected.
    Click { path: String },
    /// A row picked without opening it, as a right-click does.
    Select { path: String },
    /// A file opened in Code or the editor, by a double click, a button or
    /// the menu.
    Open { path: String, how: OpenIn },
    Mode { mode: Mode },
    Filter { filter: String },
    /// Enter in the filter field after a `#`: the checkout's text searched
    /// in the Code stage, which the field's hint promises.
    Search { query: String },
    /// The open folders read again.
    Reread,
    /// An entry shown in the file manager; with no `path`, the checkout's
    /// own folder.
    Reveal { path: Option<String> },
    /// A shell on the checkout.
    Shell,
}

/// What a gesture leaves for the window to do beyond the daemon's messages.
#[derive(Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    /// The panel is put away.
    Close,
    Code { checkout: CheckoutId, path: String },
    Edit { checkout: CheckoutId, path: String },
    Search { checkout: CheckoutId, query: String },
    Reveal { checkout: CheckoutId, path: Option<String> },
    Shell { checkout: CheckoutId },
}

/// One checkout's folder, read as far as the person has opened it.
pub struct Files {
    checkout: CheckoutId,
    expanded: BTreeSet<String>,
    listings: HashMap<String, Listing>,
    /// The folder each request still waiting on its answer asked for.
    asked: HashMap<u64, String>,
    selected: Option<String>,
    mode: Mode,
    filter: String,
    failed: Option<String>,
}

impl Files {
    /// The panel opened on `checkout`: its root is asked for.
    pub fn open(checkout: CheckoutId, id: impl FnMut() -> u64) -> (Files, Vec<ClientMsg>) {
        let mut files = Files {
            checkout,
            expanded: BTreeSet::new(),
            listings: HashMap::new(),
            asked: HashMap::new(),
            selected: None,
            mode: Mode::All,
            filter: String::new(),
            failed: None,
        };
        let asks = files.ask(&[String::new()], id);
        (files, asks)
    }

    pub fn checkout(&self) -> CheckoutId {
        self.checkout
    }

    fn ask(&mut self, folders: &[String], mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        folders
            .iter()
            .map(|path| {
                let request_id = id();
                self.asked.insert(request_id, path.clone());
                ClientMsg::ListDir { request_id, checkout: self.checkout, path: path.clone() }
            })
            .collect()
    }

    fn view(&self) -> View<'_> {
        View { mode: self.mode, filter: &self.filter }
    }

    fn rows(&self) -> Vec<FileRow> {
        files_page::rows(&self.listings, &self.expanded, &self.view())
    }

    /// A folder's listing. One for a request the panel no longer waits on —
    /// another checkout's, or a folder read again since — is dropped.
    pub fn receive(&mut self, dir: CheckoutDir) -> bool {
        if dir.checkout != self.checkout || self.asked.remove(&dir.request_id).is_none() {
            return false;
        }
        self.failed = None;
        self.listings.insert(dir.path, Listing { entries: dir.entries, truncated: dir.truncated });
        true
    }

    /// Whether `refusal` is of a folder the panel asked for; the folder is
    /// shut again and the page says why.
    pub fn refused(&mut self, refusal: &RequestRefusal) -> bool {
        let Some(path) = self.asked.remove(&refusal.request_id) else {
            return false;
        };
        self.expanded.remove(&path);
        self.failed = Some(refusal.reason.clone());
        true
    }

    /// The open folders and the root asked for again; what was read stays
    /// up until the new answer replaces it.
    pub fn reread(&mut self, id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        self.failed = None;
        let mut folders = vec![String::new()];
        folders.extend(self.expanded.iter().filter(|p| self.listings.contains_key(*p)).cloned());
        self.ask(&folders, id)
    }

    fn toggle(&mut self, path: &str, id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        if self.expanded.remove(path) {
            return Vec::new();
        }
        self.expanded.insert(path.to_owned());
        self.ask(&[path.to_owned()], id)
    }

    fn selected_row(&self, rows: &[FileRow]) -> Option<usize> {
        let selected = self.selected.as_deref()?;
        rows.iter().position(|r| r.path == selected)
    }

    /// Moves the selection `by` rows, held at either end; from none, down
    /// lands on the first row and up on the last.
    fn step(&mut self, by: isize) {
        let rows = self.rows();
        let next = match self.selected_row(&rows) {
            Some(at) => at.saturating_add_signed(by).min(rows.len().saturating_sub(1)),
            None if by < 0 => rows.len().saturating_sub(1),
            None => 0,
        };
        self.select_at(&rows, next);
    }

    fn select_at(&mut self, rows: &[FileRow], at: usize) {
        if let Some(row) = rows.get(at) {
            self.selected = Some(row.path.clone());
        }
    }

    pub fn act(&mut self, action: Action, mut id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) -> Then {
        let rows = self.rows();
        let row = self.selected_row(&rows).map(|i| rows[i].clone());
        match action {
            Action::Next => self.step(1),
            Action::Previous => self.step(-1),
            Action::First => self.select_at(&rows, 0),
            Action::Last => self.select_at(&rows, rows.len().saturating_sub(1)),
            Action::Choose => match row {
                Some(row) if row.dir => out.extend(self.toggle(&row.path, &mut id)),
                Some(row) => return Then::Code { checkout: self.checkout, path: row.path },
                None => {}
            },
            Action::OpenEditor => {
                if let Some(row) = row.filter(|r| !r.dir) {
                    return Then::Edit { checkout: self.checkout, path: row.path };
                }
            }
            // Right opens a closed folder, and on an open one steps into it.
            Action::OpenFolder => {
                if let Some(row) = row.filter(|r| r.dir) {
                    if row.open {
                        self.step(1);
                    } else {
                        out.extend(self.toggle(&row.path, &mut id));
                    }
                }
            }
            // Left shuts an open folder, and on anything else steps out to
            // the folder it is in.
            Action::ShutFolder => {
                if let Some(row) = row {
                    if row.dir && row.open {
                        out.extend(self.toggle(&row.path, &mut id));
                    } else if let Some(parent) = Some(files_page::parent(&row.path)).filter(|p| !p.is_empty()) {
                        self.selected = Some(parent.to_owned());
                    }
                }
            }
            Action::CloseLayer => return Then::Close,
            _ => {}
        }
        Then::Nothing
    }

    pub fn gesture(&mut self, gesture: FilesGesture, id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) -> Then {
        match gesture {
            FilesGesture::Close => return Then::Close,
            FilesGesture::Action { action } => return self.act(action, id, out),
            FilesGesture::Click { path } => {
                let dir = files_page::find(&self.listings, &path).is_some_and(|e| e.dir);
                self.selected = Some(path.clone());
                if dir {
                    out.extend(self.toggle(&path, id));
                }
            }
            FilesGesture::Select { path } => self.selected = Some(path),
            FilesGesture::Open { path, how } => {
                self.selected = Some(path.clone());
                let checkout = self.checkout;
                return match how {
                    OpenIn::Code => Then::Code { checkout, path },
                    OpenIn::Editor => Then::Edit { checkout, path },
                };
            }
            FilesGesture::Mode { mode } => self.mode = mode,
            FilesGesture::Filter { filter } => self.filter = filter,
            FilesGesture::Search { query } => return Then::Search { checkout: self.checkout, query },
            FilesGesture::Reread => out.extend(self.reread(id)),
            FilesGesture::Reveal { path } => return Then::Reveal { checkout: self.checkout, path },
            FilesGesture::Shell => return Then::Shell { checkout: self.checkout },
        }
        Then::Nothing
    }

    /// The page, from the checkout as the tree has it now.
    pub fn page(&self, info: &CheckoutInfo, daemon: Daemon, now: u64) -> FilesPage {
        let rows = self.rows();
        let selected = self.selected_row(&rows).map(|i| rows[i].clone());
        let entry = selected.as_ref().and_then(|r| files_page::find(&self.listings, &r.path));
        let separator = if info.path.contains('\\') { "\\" } else { "/" };
        FilesPage {
            checkout: info.id.0,
            name: info.name.clone(),
            root: info.path.clone(),
            separator: separator.into(),
            mode: self.mode,
            changed: info.git.as_ref().map_or(0, |g| g.changed_files),
            filter: self.filter.clone(),
            crumb: files_page::crumb(&info.name, selected.as_ref().map(|r| (r.path.as_str(), r.dir))),
            selected: selected.map(|r| r.path),
            footer: entry.map(|e| files_page::footer(e, now)),
            rows,
            reveal: REVEAL_LABEL.into(),
            can_reveal: daemon.local,
            loading: !self.listings.contains_key("") && self.failed.is_none(),
            error: self.failed.clone(),
            truncated: self.listings.values().any(|l| l.truncated),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::checkout;
    use argus_protocol::{CheckoutEntry, EntryGit};

    const CO: CheckoutId = CheckoutId(100);

    fn dir_entry(name: &str, dir: bool, git: Option<EntryGit>) -> CheckoutEntry {
        CheckoutEntry { name: name.into(), dir, size: (!dir).then_some(10), modified: None, git }
    }

    fn answer(request_id: u64, path: &str, entries: Vec<CheckoutEntry>) -> CheckoutDir {
        CheckoutDir { request_id, checkout: CO, path: path.into(), entries, truncated: false }
    }

    /// Unique across calls, so two asks outstanding never share an id.
    fn ids() -> impl FnMut() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        || NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// A panel with the root read: `src/` and `a.rs`, and `src` holding `b.rs`.
    fn opened() -> Files {
        let (mut files, asks) = Files::open(CO, ids());
        let [ClientMsg::ListDir { request_id, path, .. }] = &asks[..] else {
            panic!("{asks:?}");
        };
        assert_eq!(path, "");
        assert!(files.receive(answer(*request_id, "", vec![dir_entry("src", true, None), dir_entry("a.rs", false, None)])));
        files
    }

    fn paths(files: &Files) -> Vec<String> {
        files.rows().into_iter().map(|r| r.path).collect()
    }

    #[test]
    fn opening_asks_for_the_root_and_nothing_shows_until_it_answers() {
        let (files, asks) = Files::open(CO, ids());
        assert!(matches!(&asks[..], [ClientMsg::ListDir { checkout: CheckoutId(100), path, .. }] if path.is_empty()));
        assert!(files.rows().is_empty());
        let info = checkout(100, "main", true, Vec::new());
        assert!(files.page(&info, Daemon::default(), 0).loading);
    }

    #[test]
    fn opening_a_folder_asks_for_it_and_closing_it_asks_nothing() {
        let mut files = opened();
        let mut out = Vec::new();
        files.gesture(FilesGesture::Click { path: "src".into() }, ids(), &mut out);
        assert!(matches!(&out[..], [ClientMsg::ListDir { path, .. }] if path == "src"));
        out.clear();
        files.gesture(FilesGesture::Click { path: "src".into() }, ids(), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn a_listing_for_a_request_no_longer_waited_on_is_dropped() {
        let mut files = opened();
        assert!(!files.receive(answer(99, "src", vec![dir_entry("x", false, None)])));
        assert!(!files.receive(CheckoutDir { checkout: CheckoutId(7), ..answer(2, "", Vec::new()) }));
        assert_eq!(paths(&files), ["src", "a.rs"]);
    }

    #[test]
    fn a_folder_that_will_not_read_is_shut_again_and_says_why() {
        let mut files = opened();
        let mut out = Vec::new();
        files.gesture(FilesGesture::Click { path: "src".into() }, ids(), &mut out);
        let [ClientMsg::ListDir { request_id, .. }] = &out[..] else {
            panic!("{out:?}");
        };
        let refusal = RequestRefusal { request_id: *request_id, reason: "no such folder".into(), read: true };
        assert!(files.refused(&refusal));
        let info = checkout(100, "main", true, Vec::new());
        let page = files.page(&info, Daemon::default(), 0);
        assert_eq!(page.error.as_deref(), Some("no such folder"));
        assert!(!page.rows[0].open);
        assert!(!files.refused(&refusal), "once");
    }

    #[test]
    fn reading_again_asks_for_the_root_and_each_open_folder_and_keeps_what_shows() {
        let mut files = opened();
        let mut out = Vec::new();
        files.gesture(FilesGesture::Click { path: "src".into() }, ids(), &mut out);
        let [ClientMsg::ListDir { request_id, .. }] = &out[..] else {
            panic!("{out:?}");
        };
        files.receive(answer(*request_id, "src", vec![dir_entry("b.rs", false, None)]));
        out.clear();
        files.gesture(FilesGesture::Reread, ids(), &mut out);
        let asked: Vec<_> = out.iter().map(|m| format!("{m:?}")).collect();
        assert_eq!(out.len(), 2, "{asked:?}");
        assert_eq!(paths(&files), ["src", "src/b.rs", "a.rs"], "the old rows stay up");
    }

    #[test]
    fn keys_move_over_the_rows_and_hold_at_the_ends() {
        let mut files = opened();
        let mut out = Vec::new();
        files.act(Action::Previous, ids(), &mut out);
        assert_eq!(files.selected.as_deref(), Some("a.rs"), "from nothing, up goes to the last");
        files.act(Action::Next, ids(), &mut out);
        assert_eq!(files.selected.as_deref(), Some("a.rs"));
        files.act(Action::First, ids(), &mut out);
        assert_eq!(files.selected.as_deref(), Some("src"));
    }

    #[test]
    fn enter_opens_a_file_in_code_and_toggles_a_folder_and_o_opens_a_file_in_the_editor() {
        let mut files = opened();
        let mut out = Vec::new();
        files.act(Action::First, ids(), &mut out);
        assert_eq!(files.act(Action::Choose, ids(), &mut out), Then::Nothing);
        assert!(matches!(&out[..], [ClientMsg::ListDir { path, .. }] if path == "src"));
        assert_eq!(files.act(Action::OpenEditor, ids(), &mut out), Then::Nothing, "a folder has no editor");
        files.act(Action::Last, ids(), &mut out);
        assert_eq!(files.act(Action::Choose, ids(), &mut out), Then::Code { checkout: CO, path: "a.rs".into() });
        assert_eq!(files.act(Action::OpenEditor, ids(), &mut out), Then::Edit { checkout: CO, path: "a.rs".into() });
    }

    #[test]
    fn right_opens_a_folder_then_steps_in_and_left_shuts_it_then_steps_out() {
        let mut files = opened();
        let mut out = Vec::new();
        files.act(Action::First, ids(), &mut out);
        files.act(Action::OpenFolder, ids(), &mut out);
        let [ClientMsg::ListDir { request_id, .. }] = &out[..] else {
            panic!("{out:?}");
        };
        files.receive(answer(*request_id, "src", vec![dir_entry("b.rs", false, None)]));
        files.act(Action::OpenFolder, ids(), &mut out);
        assert_eq!(files.selected.as_deref(), Some("src/b.rs"));
        files.act(Action::ShutFolder, ids(), &mut out);
        assert_eq!(files.selected.as_deref(), Some("src"), "out to the folder it is in");
        files.act(Action::ShutFolder, ids(), &mut out);
        assert_eq!(paths(&files), ["src", "a.rs"], "then the folder itself shuts");
    }

    #[test]
    fn a_hash_search_from_the_filter_field_is_the_code_stages_to_run() {
        let mut files = opened();
        let mut out = Vec::new();
        let then = files.gesture(FilesGesture::Search { query: "needle".into() }, ids(), &mut out);
        assert_eq!(then, Then::Search { checkout: CO, query: "needle".into() });
        assert!(out.is_empty(), "the daemon is asked by the Code stage, not the panel");
    }

    #[test]
    fn escape_puts_the_panel_away_and_a_click_on_a_file_only_selects_it() {
        let mut files = opened();
        let mut out = Vec::new();
        assert_eq!(files.act(Action::CloseLayer, ids(), &mut out), Then::Close);
        assert_eq!(files.gesture(FilesGesture::Click { path: "a.rs".into() }, ids(), &mut out), Then::Nothing);
        assert_eq!(files.selected.as_deref(), Some("a.rs"));
        assert!(out.is_empty());
    }

    #[test]
    fn the_page_says_the_selection_in_its_crumb_and_footer_and_only_a_local_daemon_can_reveal() {
        let mut files = opened();
        let mut out = Vec::new();
        files.gesture(FilesGesture::Select { path: "a.rs".into() }, ids(), &mut out);
        let mut info = checkout(100, "night", false, Vec::new());
        info.path = "C:\\work\\night".into();
        let page = files.page(&info, Daemon { lists_files: true, local: false }, 0);
        assert_eq!(page.crumb, ["night"]);
        assert_eq!(page.footer.map(|f| f.name), Some("a.rs".into()));
        assert_eq!(page.separator, "\\");
        assert!(!page.can_reveal);
        assert!(files.page(&info, Daemon { lists_files: true, local: true }, 0).can_reveal);
    }
}
