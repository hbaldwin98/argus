//! The directory browser behind "add project" and "add repository".
//!
//! A path typed blind into a text field is the one gesture in Argus that
//! could not be checked before it was sent: you found out you had the
//! wrong directory only when the project came up empty. This browses
//! instead — a live listing of wherever you are, fuzzy-filtered as you
//! type, with Git repositories marked, so the thing you are adding is on
//! screen before you add it.
//!
//! The widget owns no I/O. Keys turn into a [`DirAction`] the app carries
//! out, which is what lets the whole of the navigation be tested without a
//! daemon or a terminal.

use argus_protocol::{DirEntry, DirListing, ProjectId};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::fuzzy::Fuzzy;

/// What a confirmed directory becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirTarget {
    Project,
    Repository(ProjectId),
}

/// One row. The listing is preceded by the directory you are standing in,
/// so "add this one" is always on screen rather than behind a key you have
/// to know about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirRow {
    Here,
    Child { name: String, is_repo: bool },
}

/// What the app should do about a keystroke. The picker never touches the
/// filesystem or the socket itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirAction {
    None,
    /// Ask the daemon for this directory's listing.
    Browse(String),
    /// Add this directory, and close.
    Choose(String),
    Close,
}

pub struct DirPicker {
    pub target: DirTarget,
    /// The directory being shown, absolute. Empty only before the first
    /// listing arrives, when the daemon has not yet said where "start"
    /// was.
    pub path: String,
    pub parent: Option<String>,
    rows: Vec<DirRow>,
    /// Just the names, kept beside `rows` because that is what the matcher
    /// takes and rebuilding it per keystroke is pure waste.
    names: Vec<String>,
    pub query: String,
    /// Indices into `rows`, best match first. `sel` indexes into *this*.
    pub shown: Vec<usize>,
    pub sel: usize,
    /// Why the current directory listed nothing — gone, or not ours to
    /// read. An empty directory and an unreadable one look identical
    /// otherwise.
    pub error: Option<String>,
    /// The listing we are still waiting for. Replies for anything else are
    /// from a directory the user has already left.
    pub pending: Option<u64>,
}

impl DirPicker {
    pub fn new(target: DirTarget, request_id: u64) -> Self {
        DirPicker {
            target,
            path: String::new(),
            parent: None,
            rows: Vec::new(),
            names: Vec::new(),
            query: String::new(),
            shown: Vec::new(),
            sel: 0,
            error: None,
            pending: Some(request_id),
        }
    }

    pub fn title(&self) -> &'static str {
        match self.target {
            DirTarget::Project => "add project",
            DirTarget::Repository(_) => "add repository",
        }
    }

    /// Takes a listing the app has already matched to `pending`. The query
    /// is cleared: it filtered the directory we just left.
    pub fn show(&mut self, listing: DirListing) {
        self.path = listing.path;
        self.parent = listing.parent;
        self.error = listing.error;
        self.pending = None;
        self.query.clear();
        self.rows = std::iter::once(DirRow::Here)
            .chain(
                listing
                    .entries
                    .into_iter()
                    .map(|DirEntry { name, is_repo }| DirRow::Child { name, is_repo }),
            )
            .collect();
        self.names = self.rows.iter().map(row_name).collect();
        self.refilter();
    }

    #[cfg(test)]
    pub fn rows(&self) -> &[DirRow] {
        &self.rows
    }

    pub fn len(&self) -> usize {
        self.shown.len()
    }

    pub fn row(&self, i: usize) -> Option<&DirRow> {
        self.rows.get(*self.shown.get(i)?)
    }

    fn selected(&self) -> Option<&DirRow> {
        self.row(self.sel)
    }

    /// The absolute path a row stands for.
    fn path_of(&self, row: &DirRow) -> String {
        match row {
            DirRow::Here => self.path.clone(),
            DirRow::Child { name, .. } => join(&self.path, name),
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> DirAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => DirAction::Close,
            KeyCode::Down => self.moved(1),
            KeyCode::Up => self.moved(-1),
            KeyCode::Char('n') if ctrl => self.moved(1),
            KeyCode::Char('p') if ctrl => self.moved(-1),
            // Descend. Sideways movement is the one gesture a fuzzy picker
            // has spare — every printable key is already query text.
            KeyCode::Tab | KeyCode::Right => self.descend(),
            KeyCode::Left => self.ascend(),
            KeyCode::Backspace => {
                // Backspacing past the start of the query keeps deleting
                // the thing to its left, which is the last path segment.
                if self.query.pop().is_some() {
                    self.refilter();
                    DirAction::None
                } else {
                    self.ascend()
                }
            }
            KeyCode::Enter => self.confirm(),
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.refilter();
                DirAction::None
            }
            _ => DirAction::None,
        }
    }

    /// Pasted text goes in as query, minus the control characters a
    /// multi-line paste would otherwise smuggle in.
    pub fn paste(&mut self, text: &str) {
        self.query.extend(text.chars().filter(|c| !c.is_control()));
        self.refilter();
    }

    fn confirm(&mut self) -> DirAction {
        // A typed or pasted absolute path with nothing to show for it is
        // someone telling us where to go, not a filter that failed.
        if self.shown.is_empty() {
            return self.jump().unwrap_or(DirAction::None);
        }
        match self.selected() {
            Some(row) => DirAction::Choose(self.path_of(row)),
            None => DirAction::None,
        }
    }

    fn descend(&mut self) -> DirAction {
        if let Some(jump) = self.jump() {
            return jump;
        }
        match self.selected() {
            // "Here" is where we already are; descending into it would be
            // a listing of the same directory.
            Some(DirRow::Here) | None => DirAction::None,
            Some(row) => DirAction::Browse(self.path_of(row)),
        }
    }

    fn ascend(&mut self) -> DirAction {
        match &self.parent {
            Some(parent) => DirAction::Browse(parent.clone()),
            None => DirAction::None,
        }
    }

    /// Going straight to a path the user typed or pasted, rather than
    /// filtering by it. Only for text that is unmistakably a path — a
    /// query with a separator in it can never match a bare child name
    /// anyway.
    fn jump(&self) -> Option<DirAction> {
        let q = self.query.trim();
        let pathish = q.starts_with('~')
            || q.starts_with('/')
            || q.starts_with('\\')
            || (q.len() > 1 && q.as_bytes()[1] == b':')
            || q.contains('/')
            || q.contains('\\');
        pathish.then(|| DirAction::Browse(q.to_string()))
    }

    fn moved(&mut self, delta: isize) -> DirAction {
        let last = self.len().saturating_sub(1) as isize;
        self.sel = (self.sel as isize + delta).clamp(0, last) as usize;
        DirAction::None
    }

    fn refilter(&mut self) {
        // Names, not paths: every row shares the same parent, so path
        // scoring would only be scoring the breadcrumb over and over.
        self.shown = Fuzzy::new().filter(&self.query, &self.names);
        // "Here" is the directory you are standing in, not a candidate a
        // query is looking for; it earns its top row only while nothing
        // has been typed.
        if !self.query.trim().is_empty() {
            self.shown.retain(|&i| self.rows[i] != DirRow::Here);
        }
        self.sel = 0;
    }

    #[cfg(test)]
    pub fn type_query(&mut self, q: &str) {
        for c in q.chars() {
            self.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
    }
}

fn row_name(row: &DirRow) -> String {
    match row {
        DirRow::Here => ".".to_string(),
        DirRow::Child { name, .. } => name.clone(),
    }
}

/// Joining without `PathBuf`, so the separator already in `base` is the one
/// that comes back out: a Windows breadcrumb that grows a forward slash
/// halfway along reads like a bug even when the path resolves.
fn join(base: &str, name: &str) -> String {
    let sep = if base.contains('\\') && !base.contains('/') {
        '\\'
    } else {
        '/'
    };
    let trimmed = base.trim_end_matches(['/', '\\']);
    // A root is all separator; trimming it away would make the child
    // relative.
    if trimmed.is_empty() || trimmed.ends_with(':') {
        format!("{base}{name}")
            .replace("//", "/")
            .replace("\\\\", "\\")
    } else {
        format!("{trimmed}{sep}{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(path: &str, parent: Option<&str>, names: &[(&str, bool)]) -> DirListing {
        DirListing {
            request_id: 1,
            path: path.to_string(),
            parent: parent.map(str::to_string),
            entries: names
                .iter()
                .map(|(n, is_repo)| DirEntry {
                    name: n.to_string(),
                    is_repo: *is_repo,
                })
                .collect(),
            error: None,
        }
    }

    fn at(path: &str, parent: Option<&str>, names: &[(&str, bool)]) -> DirPicker {
        let mut p = DirPicker::new(DirTarget::Project, 1);
        p.show(listing(path, parent, names));
        p
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn press(p: &mut DirPicker, code: KeyCode) -> DirAction {
        p.on_key(key(code))
    }

    fn shown(p: &DirPicker) -> Vec<String> {
        (0..p.len())
            .filter_map(|i| p.row(i))
            .map(row_name)
            .collect()
    }

    #[test]
    fn a_listing_shows_this_directory_first_then_its_children() {
        // "Here" on top is how adding the directory you navigated to stays
        // a visible option rather than a hidden key.
        let p = at(
            "/home/u",
            Some("/home"),
            &[("code", false), ("docs", false)],
        );
        assert_eq!(shown(&p), vec![".", "code", "docs"]);
    }

    #[test]
    fn enter_on_the_first_row_adds_the_directory_you_are_in() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        assert_eq!(
            press(&mut p, KeyCode::Enter),
            DirAction::Choose("/home/u".to_string())
        );
    }

    #[test]
    fn enter_on_a_child_adds_that_child_by_its_full_path() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        press(&mut p, KeyCode::Down);
        assert_eq!(
            press(&mut p, KeyCode::Enter),
            DirAction::Choose("/home/u/code".to_string())
        );
    }

    #[test]
    fn typing_filters_the_children_and_drops_this_directory() {
        // "." would match nothing a user types, but leaving it in would
        // still put the cursor on a row that answers no query.
        let mut p = at(
            "/home/u",
            Some("/home"),
            &[("code", false), ("documents", false), ("music", false)],
        );
        p.type_query("doc");
        assert_eq!(shown(&p), vec!["documents"]);
    }

    #[test]
    fn tab_descends_into_the_highlighted_child() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        p.type_query("code");
        assert_eq!(
            press(&mut p, KeyCode::Tab),
            DirAction::Browse("/home/u/code".to_string())
        );
    }

    #[test]
    fn tab_on_this_directory_goes_nowhere() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        assert_eq!(press(&mut p, KeyCode::Tab), DirAction::None);
    }

    #[test]
    fn left_climbs_to_the_parent() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        assert_eq!(
            press(&mut p, KeyCode::Left),
            DirAction::Browse("/home".to_string())
        );
    }

    #[test]
    fn a_root_has_nowhere_to_climb_to() {
        let mut p = at("/", None, &[("home", false)]);
        assert_eq!(press(&mut p, KeyCode::Left), DirAction::None);
    }

    #[test]
    fn backspace_edits_the_query_before_it_climbs() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        p.type_query("co");
        assert_eq!(press(&mut p, KeyCode::Backspace), DirAction::None);
        assert_eq!(p.query, "c");
        press(&mut p, KeyCode::Backspace);
        assert_eq!(p.query, "");
        assert_eq!(
            press(&mut p, KeyCode::Backspace),
            DirAction::Browse("/home".to_string())
        );
    }

    #[test]
    fn a_pasted_absolute_path_jumps_there_instead_of_filtering() {
        // Pasting a path you already have is the fastest way to add it,
        // and no child is ever named `/src/thing`.
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        p.paste("/var/src");
        assert_eq!(
            press(&mut p, KeyCode::Tab),
            DirAction::Browse("/var/src".to_string())
        );
    }

    #[test]
    fn enter_on_a_query_that_matched_nothing_jumps_if_it_is_a_path() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        p.paste("~/projects");
        assert_eq!(
            press(&mut p, KeyCode::Enter),
            DirAction::Browse("~/projects".to_string())
        );
    }

    #[test]
    fn a_query_that_matched_nothing_and_is_not_a_path_does_nothing() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        p.type_query("zzzz");
        assert!(shown(&p).is_empty());
        assert_eq!(press(&mut p, KeyCode::Enter), DirAction::None);
    }

    #[test]
    fn a_new_listing_clears_the_query_that_filtered_the_old_one() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        p.type_query("co");
        p.show(listing("/home/u/code", Some("/home/u"), &[("argus", true)]));
        assert_eq!(p.query, "");
        assert_eq!(shown(&p), vec![".", "argus"]);
    }

    #[test]
    fn the_cursor_stays_inside_the_rows_it_has() {
        let mut p = at("/home/u", Some("/home"), &[("code", false)]);
        for _ in 0..10 {
            press(&mut p, KeyCode::Down);
        }
        assert_eq!(p.sel, 1);
        for _ in 0..10 {
            press(&mut p, KeyCode::Up);
        }
        assert_eq!(p.sel, 0);
    }

    #[test]
    fn esc_closes() {
        let mut p = at("/home/u", Some("/home"), &[]);
        assert_eq!(press(&mut p, KeyCode::Esc), DirAction::Close);
    }

    #[test]
    fn a_repository_is_marked_as_one() {
        // Which children are repos is the difference between a project
        // root and a repository, and it is invisible from the name.
        let p = at(
            "/home/u",
            Some("/home"),
            &[("argus", true), ("notes", false)],
        );
        assert_eq!(
            p.rows()[1],
            DirRow::Child {
                name: "argus".to_string(),
                is_repo: true
            }
        );
    }

    #[test]
    fn a_windows_path_keeps_its_backslashes_when_a_child_is_joined() {
        let mut p = at(r"C:\Source", Some(r"C:\"), &[("orion", true)]);
        press(&mut p, KeyCode::Down);
        assert_eq!(
            press(&mut p, KeyCode::Enter),
            DirAction::Choose(r"C:\Source\orion".to_string())
        );
    }

    #[test]
    fn a_child_of_a_drive_root_does_not_lose_its_separator() {
        let mut p = at(r"C:\", None, &[("Source", false)]);
        press(&mut p, KeyCode::Down);
        assert_eq!(
            press(&mut p, KeyCode::Enter),
            DirAction::Choose(r"C:\Source".to_string())
        );
    }

    #[test]
    fn a_child_of_a_unix_root_does_not_get_two_slashes() {
        let mut p = at("/", None, &[("home", false)]);
        press(&mut p, KeyCode::Down);
        assert_eq!(
            press(&mut p, KeyCode::Enter),
            DirAction::Choose("/home".to_string())
        );
    }

    #[test]
    fn an_unreadable_directory_says_why_it_is_empty() {
        let mut p = DirPicker::new(DirTarget::Project, 1);
        let mut l = listing("/root", Some("/"), &[]);
        l.error = Some("permission denied".to_string());
        p.show(l);
        assert_eq!(p.error.as_deref(), Some("permission denied"));
        // "Here" survives: you can still add a directory you cannot list.
        assert_eq!(shown(&p), vec!["."]);
    }
}
