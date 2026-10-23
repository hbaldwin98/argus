//! The Changes pane's Files tab: the checkout's working tree as the one file tree every surface
//! draws (`kit::tree`, rows ordered and marked by core's `code::tree` and `checkouts::marks`, as the
//! Code stage's are), narrowed to the changes by the Changed filter and to the paths holding the
//! find's words, with the folders that hold them open.
//!
//! The list and the file read whole are the session's (`ReviewTree`); which folders are open, the
//! filter and the find are the window's own. `j` and `k` step through the files shown, skipping the
//! folders, and open each as a click does (the prototype's `stepFile`). A changed file opens as its
//! change does (`OpenChange`, on the side it is changed on, unstaged first), and a file with no
//! change is read whole in place of the diff (`ReviewRead`).

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use argus_desktop_core::checkouts::marks;
use argus_desktop_core::code::stage::CodeTreeRow;
use argus_desktop_core::code::tree::{self as code_tree, folders_of};
use argus_desktop_core::connection::page::{Call, ReviewPage};
use argus_protocol::CodeChange;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context};

use crate::kit::ghosts::ghosts;
use crate::kit::tree::{self, Row};
use crate::review::changes::side::{Files, Opened};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How a file changed, and the list it opens from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub staged: bool,
    pub added: usize,
    pub removed: usize,
}

/// Each changed file, by path, from the review's lists: its unstaged change over its staged one,
/// since the unstaged is what is still to look at.
pub fn changes(page: &ReviewPage) -> HashMap<String, Change> {
    let mut changes = HashMap::new();
    for section in &page.sections {
        let staged = section.side == "staged";
        for file in &section.files {
            let change = Change { staged, added: file.file.added, removed: file.file.removed };
            changes.entry(file.file.path.clone()).and_modify(|was: &mut Change| if was.staged && !staged { *was = change }).or_insert(change);
        }
    }
    changes
}

/// The rows drawn: the `open` folders' insides under them, or while `query` (lowercased) holds
/// words only the files whose path holds it, under their folders, open. `changed_only` leaves out
/// what holds no change. `lit` is the file on screen.
pub fn rows(files: &[String], changes: &HashMap<String, Change>, open: &HashSet<String>, changed_only: bool, query: &str, lit: Option<&str>) -> Vec<CodeTreeRow> {
    let found: Vec<String>;
    let (files, open) = if query.is_empty() {
        (files, Cow::Borrowed(open))
    } else {
        found = files.iter().filter(|path| path.to_lowercase().contains(query)).cloned().collect();
        let holding = found.iter().flat_map(|path| folders_of(path)).map(str::to_owned).collect();
        (found.as_slice(), Cow::Owned(holding))
    };
    let changed: Vec<CodeChange> = changes.iter().map(|(path, change)| CodeChange { path: path.clone(), added: change.added as u32, removed: change.removed as u32 }).collect();
    code_tree::rows(files, &changed, &open, changed_only)
        .into_iter()
        .map(|row| {
            let on = !row.folder && Some(row.path.as_str()) == lit;
            CodeTreeRow::of(row, on)
        })
        .collect()
}

/// The file lit in the tab: the one read whole, else the change on screen.
pub fn lit(page: &ReviewPage) -> Option<String> {
    let reading = page.tree.as_ref().and_then(|tree| tree.reading.as_ref()).map(|reading| reading.path.clone());
    reading.or_else(|| page.file.and_then(|file| page.files.get(file)).map(|file| file.path.clone()))
}

impl Files {
    /// The tab's list: loading bars until the first list is in, then the rows.
    pub(super) fn checkout(&self, page: &ReviewPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let list = v_flex()
            .id("review-tree")
            .test_support()
            .role(gpui_kit::Role::Tree)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(rem(tokens.sp_6))
            .pt(rem(tokens.sp_4))
            .pb(rem(10.0))
            .gap(rem(tokens.hairline));
        let Some(tree) = page.tree.as_ref().filter(|tree| tree.listed) else {
            return list.child(ghosts("review-tree-loading", &[0.6, 0.8, 0.5, 0.7], tokens));
        };
        let changes = changes(page);
        let lit = lit(page);
        let rows = rows(&tree.files, &changes, &self.folders, self.changed_only, &self.query, lit.as_deref());
        if rows.is_empty() {
            let words = if self.query.is_empty() { "No file here." } else { "No file in the checkout matches." };
            return list.child(said("review-tree-none", words, tokens));
        }
        let under = marks::changed_under(changes.keys().map(String::as_str));
        list.children(rows.iter().enumerate().map(|(at, row)| {
            let entry = Row { path: &row.path, name: &row.name, depth: row.depth, dir: row.folder, open: row.open, on: row.on, ignored: false, ringed: false, mark: marks::of_code(row, &under) };
            let (path, folder, change) = (row.path.clone(), row.folder, changes.get(&row.path).copied());
            tree::row(("review-tree-row", at), &entry, None, tokens)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !folder {
                        this.open(&path, change, cx);
                    } else if !this.folders.remove(&path) {
                        this.folders.insert(path.clone());
                    }
                    cx.notify();
                }))
                .into_any_element()
        }))
    }

    /// A file opened from the tab: its change, or the file read whole.
    fn open(&self, path: &str, change: Option<Change>, cx: &mut Context<Self>) {
        cx.emit(Opened);
        let call = match change {
            Some(change) => Call::OpenChange { staged: change.staged, path: path.to_owned() },
            None => Call::ReviewRead { path: path.to_owned() },
        };
        self.ask(call, cx);
    }

    /// `j` or `k` while the tab is shown: the next or previous file of the rows shown opened, from
    /// the first when none is lit. Whether the tab took the key.
    pub fn step(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let Some(page) = self.heard.read(cx).review.clone() else { return false };
        let Some(tree) = page.tree.as_ref().filter(|_| self.checkout && page.jump.is_none()) else { return false };
        let changes = changes(&page);
        let lit = lit(&page);
        let files: Vec<String> = rows(&tree.files, &changes, &self.folders, self.changed_only, &self.query, lit.as_deref()).into_iter().filter(|row| !row.folder).map(|row| row.path).collect();
        let at = files.iter().position(|path| Some(path) == lit.as_ref());
        let next = match at {
            Some(at) if forward => (at + 1).min(files.len().saturating_sub(1)),
            Some(at) => at.saturating_sub(1),
            None => 0,
        };
        if let Some(path) = files.get(next).filter(|_| Some(next) != at) {
            self.open(path, changes.get(path).copied(), cx);
        }
        true
    }
}

fn said(id: &'static str, words: &'static str, tokens: &Tokens) -> impl IntoElement {
    div().id(id).test_support().aria_label(words).px(rem(tokens.sp_12)).py(rem(tokens.sp_8)).typed(tokens.type_small).text_color(tokens.fg_muted).child(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| (*p).to_owned()).collect()
    }

    fn changed(list: &[(&str, bool)]) -> HashMap<String, Change> {
        list.iter().map(|(path, staged)| ((*path).to_owned(), Change { staged: *staged, added: 1, removed: 0 })).collect()
    }

    fn said(rows: &[CodeTreeRow]) -> Vec<String> {
        rows.iter().map(|row| format!("{}{}{}", "  ".repeat(row.depth), row.name, if row.folder { "/" } else { "" })).collect()
    }

    const FILES: [&str; 5] = ["README.md", "crates/a/src/lib.rs", "crates/a/src/main.rs", "crates/b/lib.rs", "Cargo.toml"];

    #[test]
    fn a_find_lists_the_files_whose_path_holds_the_words_under_their_folders_open() {
        let changes = changed(&[("crates/b/lib.rs", false)]);
        let rows = rows(&paths(&FILES), &changes, &HashSet::new(), false, "lib", None);
        assert_eq!(said(&rows), ["crates/", "  a/", "    src/", "      lib.rs", "  b/", "    lib.rs"]);
        let only = super::rows(&paths(&FILES), &changes, &HashSet::new(), true, "lib", None);
        assert_eq!(said(&only), ["crates/", "  b/", "    lib.rs"], "the filter holds while finding");
    }

    #[test]
    fn the_file_on_screen_is_the_row_lit() {
        let rows = rows(&paths(&FILES), &HashMap::new(), &HashSet::new(), false, "", Some("README.md"));
        let on: Vec<&str> = rows.iter().filter(|row| row.on).map(|row| row.path.as_str()).collect();
        assert_eq!(on, ["README.md"]);
    }

    #[test]
    fn a_change_opens_from_its_unstaged_side_over_its_staged_one() {
        use argus_desktop_core::connection::page::ReviewFile;
        use argus_desktop_core::review::staging::{ReviewSection, SectionFile};
        let file = |path: &str| SectionFile { file: ReviewFile { path: path.into(), dir: String::new(), name: path.into(), added: 1, removed: 0, kind: "modified" }, partly: true, on: false };
        let page = ReviewPage {
            sections: vec![ReviewSection { side: "staged", files: vec![file("a.rs"), file("b.rs")] }, ReviewSection { side: "unstaged", files: vec![file("a.rs")] }],
            ..ReviewPage::default()
        };
        let changes = changes(&page);
        assert!(!changes["a.rs"].staged);
        assert!(changes["b.rs"].staged);
    }
}
