//! What the Checkouts stage's files panel draws from the folders it has
//! read: the rows of the tree in the order they show, each entry's mark,
//! the breadcrumb of the selected row's folder, and the footer's size and
//! age. The page draws these as sent and decides nothing about them.

use std::collections::{BTreeSet, HashMap};

use argus_protocol::{CheckoutEntry, EntryGit};
use serde::{Deserialize, Serialize};

/// Which entries of the loaded folders show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    All,
    /// Only what git says is changed or untracked, and the folders on the
    /// way to it that are open.
    Changed,
}

/// One folder as the daemon listed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<CheckoutEntry>,
    pub truncated: bool,
}

/// What git makes of a row, as the page marks it: `M`, `?`, or faint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum FileMark {
    Changed,
    Untracked,
    Ignored,
}

/// One row of the tree, in the order the page lists them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    /// Relative to the checkout, `/`-separated: what the page sends back.
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub dir: bool,
    pub open: bool,
    pub mark: Option<FileMark>,
}

/// The selected entry, said along the panel's foot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FilesFooter {
    pub name: String,
    pub dir: bool,
    /// `212 KB · changed 4m ago`.
    pub detail: String,
}

/// The files panel as the page draws it; `None` of it on the Checkouts
/// table while it is closed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FilesPage {
    pub checkout: u64,
    pub name: String,
    /// The checkout's folder on the daemon's host, and the separator its
    /// paths use, so a path the page copies is spelled as that host does.
    pub root: String,
    pub separator: String,
    pub mode: Mode,
    /// How many paths git says changed here, beside the Changed button.
    pub changed: usize,
    pub filter: String,
    /// The checkout's name, then the folders down to the selected row's.
    pub crumb: Vec<String>,
    pub rows: Vec<FileRow>,
    pub selected: Option<String>,
    pub footer: Option<FilesFooter>,
    /// `Show in Explorer`, `Show in Finder` or `Show in Files`.
    pub reveal: String,
    /// The daemon runs on this machine, so the file manager can show what
    /// it lists.
    pub can_reveal: bool,
    /// The first listing has not come.
    pub loading: bool,
    /// Why a folder could not be read.
    pub error: Option<String>,
    /// A folder held more than a listing carries.
    pub truncated: bool,
}

impl FilesPage {
    /// The path of `relative` as the daemon's host spells it: the root without its trailing
    /// separators, then each folder after a separator of the host's own, so a path copied
    /// opens where it was copied from.
    pub fn path_of(&self, relative: &str) -> String {
        let tail = relative.replace('/', &self.separator);
        format!("{}{}{tail}", self.root.trim_end_matches(['/', '\\']), self.separator)
    }
}

pub const REVEAL_LABEL: &str = if cfg!(windows) {
    "Show in Explorer"
} else if cfg!(target_os = "macos") {
    "Show in Finder"
} else {
    "Show in Files"
};

/// What the rows are built under.
pub struct View<'a> {
    pub mode: Mode,
    pub filter: &'a str,
}

pub fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_owned() } else { format!("{folder}/{name}") }
}

/// The folder a path is in; empty for the root.
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn name_matches(name: &str, wanted: &str) -> bool {
    wanted.is_empty() || name.to_lowercase().contains(wanted)
}

fn mode_keeps(entry: &CheckoutEntry, mode: Mode) -> bool {
    mode == Mode::All || matches!(entry.git, Some(EntryGit::Changed | EntryGit::Untracked))
}

/// Whether `entry`, in `folder`, shows: it passes the mode, and the filter
/// matches its name or, for a folder read already, something under it.
fn keeps(listings: &HashMap<String, Listing>, view: &View, wanted: &str, folder: &str, entry: &CheckoutEntry) -> bool {
    mode_keeps(entry, view.mode)
        && (name_matches(&entry.name, wanted) || (entry.dir && below_matches(listings, view, wanted, &join(folder, &entry.name))))
}

/// Whether something read under `folder` shows, filter included.
fn below_matches(listings: &HashMap<String, Listing>, view: &View, wanted: &str, folder: &str) -> bool {
    !wanted.is_empty()
        && listings.get(folder).is_some_and(|l| l.entries.iter().any(|e| keeps(listings, view, wanted, folder, e)))
}

/// The tree in the order it shows: each read folder's entries as listed,
/// with the open folders' under them. A filter shows the names it matches
/// and the folders holding one, opened.
pub fn rows(listings: &HashMap<String, Listing>, expanded: &BTreeSet<String>, view: &View) -> Vec<FileRow> {
    let wanted = view.filter.trim().to_lowercase();
    let mut out = Vec::new();
    walk(listings, expanded, view, &wanted, "", 0, &mut out);
    out
}

fn walk(
    listings: &HashMap<String, Listing>,
    expanded: &BTreeSet<String>,
    view: &View,
    wanted: &str,
    folder: &str,
    depth: usize,
    out: &mut Vec<FileRow>,
) {
    let Some(listing) = listings.get(folder) else {
        return;
    };
    for entry in &listing.entries {
        if !keeps(listings, view, wanted, folder, entry) {
            continue;
        }
        let path = join(folder, &entry.name);
        let open = entry.dir && (expanded.contains(&path) || below_matches(listings, view, wanted, &path));
        out.push(FileRow {
            name: entry.name.clone(),
            depth,
            dir: entry.dir,
            open,
            mark: entry.git.map(|git| match git {
                EntryGit::Changed => FileMark::Changed,
                EntryGit::Untracked => FileMark::Untracked,
                EntryGit::Ignored => FileMark::Ignored,
            }),
            path: path.clone(),
        });
        if open {
            walk(listings, expanded, view, wanted, &path, depth + 1, out);
        }
    }
}

/// The entry at `path`, if its folder has been read.
pub fn find<'a>(listings: &'a HashMap<String, Listing>, path: &str) -> Option<&'a CheckoutEntry> {
    let name = path.rsplit('/').next()?;
    listings.get(parent(path))?.entries.iter().find(|e| e.name == name)
}

/// The checkout's name, then the folders down to the selected row's: a
/// folder selected is where it is, a file the folder it is in.
pub fn crumb(checkout: &str, selected: Option<(&str, bool)>) -> Vec<String> {
    let mut parts = vec![checkout.to_owned()];
    if let Some((path, dir)) = selected {
        let folder = if dir { path } else { parent(path) };
        parts.extend(folder.split('/').filter(|p| !p.is_empty()).map(str::to_owned));
    }
    parts
}

/// A size as a file manager says it: bytes to a kilobyte, then rounded
/// kilobytes, then megabytes to a tenth.
pub fn size_text(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{} KB", (bytes + 512) / 1024),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

pub fn footer(entry: &CheckoutEntry, now: u64) -> FilesFooter {
    let mut parts = Vec::new();
    if let Some(size) = entry.size {
        parts.push(size_text(size));
    }
    if entry.modified.is_some() {
        parts.push(format!("changed {} ago", argus_protocol::ago(entry.modified, now)));
    }
    FilesFooter { name: entry.name.clone(), dir: entry.dir, detail: parts.join(" · ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, dir: bool, git: Option<EntryGit>) -> CheckoutEntry {
        CheckoutEntry { name: name.into(), dir, size: (!dir).then_some(2048), modified: Some(100), git }
    }

    fn listing(entries: Vec<CheckoutEntry>) -> Listing {
        Listing { entries, truncated: false }
    }

    fn loaded() -> HashMap<String, Listing> {
        HashMap::from([
            (
                String::new(),
                listing(vec![
                    entry("crates", true, Some(EntryGit::Changed)),
                    entry("target", true, Some(EntryGit::Ignored)),
                    entry("README.md", false, None),
                ]),
            ),
            (
                "crates".into(),
                listing(vec![entry("argusd", true, None), entry("notes.txt", false, Some(EntryGit::Untracked))]),
            ),
            ("crates/argusd".into(), listing(vec![entry("main.rs", false, Some(EntryGit::Changed))])),
        ])
    }

    fn paths(rows: &[FileRow]) -> Vec<&str> {
        rows.iter().map(|r| r.path.as_str()).collect()
    }

    const ALL: View<'static> = View { mode: Mode::All, filter: "" };

    #[test]
    fn open_folders_show_their_entries_under_them_and_closed_ones_do_not() {
        let open = BTreeSet::from(["crates".to_owned()]);
        let shown = rows(&loaded(), &open, &ALL);
        assert_eq!(paths(&shown), ["crates", "crates/argusd", "crates/notes.txt", "target", "README.md"]);
        assert_eq!(shown[1].depth, 1);
        assert!(shown[0].open && !shown[1].open);
        assert_eq!(paths(&rows(&loaded(), &BTreeSet::new(), &ALL)), ["crates", "target", "README.md"]);
    }

    #[test]
    fn a_folder_that_was_never_read_shows_no_children_though_open() {
        let open = BTreeSet::from(["target".to_owned()]);
        assert_eq!(paths(&rows(&loaded(), &open, &ALL)), ["crates", "target", "README.md"]);
    }

    #[test]
    fn the_changed_mode_keeps_only_what_git_says_is_changed_or_untracked() {
        let open = BTreeSet::from(["crates".to_owned(), "crates/argusd".to_owned()]);
        let shown = rows(&loaded(), &open, &View { mode: Mode::Changed, filter: "" });
        assert_eq!(paths(&shown), ["crates", "crates/notes.txt"], "argusd is clean, so is README, target is ignored");
    }

    #[test]
    fn a_filter_shows_matching_names_and_opens_the_folders_that_hold_one() {
        let shown = rows(&loaded(), &BTreeSet::new(), &View { mode: Mode::All, filter: " MAIN " });
        assert_eq!(paths(&shown), ["crates", "crates/argusd", "crates/argusd/main.rs"]);
        assert!(shown[0].open && shown[1].open);
    }

    #[test]
    fn marks_follow_git() {
        let shown = rows(&loaded(), &BTreeSet::new(), &ALL);
        let marks: Vec<_> = shown.iter().map(|r| r.mark).collect();
        assert_eq!(marks, [Some(FileMark::Changed), Some(FileMark::Ignored), None]);
    }

    #[test]
    fn the_crumb_is_the_folder_of_the_selected_row() {
        assert_eq!(crumb("night", None), ["night"]);
        assert_eq!(crumb("night", Some(("crates/argusd/main.rs", false))), ["night", "crates", "argusd"]);
        assert_eq!(crumb("night", Some(("crates/argusd", true))), ["night", "crates", "argusd"]);
        assert_eq!(crumb("night", Some(("README.md", false))), ["night"]);
    }

    #[test]
    fn sizes_and_ages_are_said_the_way_a_file_manager_says_them() {
        assert_eq!(size_text(12), "12 B");
        assert_eq!(size_text(212 * 1024), "212 KB");
        assert_eq!(size_text(3 * 1_048_576 / 2), "1.5 MB");
        let f = footer(&entry("a.rs", false, None), 100 + 240);
        assert_eq!(f.detail, "2 KB · changed 4m ago");
        assert_eq!(footer(&entry("src", true, None), 100 + 7200).detail, "changed 2h ago");
    }

    #[test]
    fn a_path_is_spelled_as_the_daemons_host_spells_it() {
        let windows = FilesPage { root: "C:\\src\\night\\\\".into(), separator: "\\".into(), ..FilesPage::default() };
        assert_eq!(windows.path_of("crates/argusd/main.rs"), "C:\\src\\night\\crates\\argusd\\main.rs");
        let unix = FilesPage { root: "/home/me/night/".into(), separator: "/".into(), ..FilesPage::default() };
        assert_eq!(unix.path_of("README.md"), "/home/me/night/README.md");
    }

    #[test]
    fn an_entry_is_found_by_its_path() {
        assert_eq!(find(&loaded(), "crates/argusd").map(|e| e.dir), Some(true));
        assert_eq!(find(&loaded(), "README.md").map(|e| e.dir), Some(false));
        assert!(find(&loaded(), "crates/gone").is_none());
    }
}
