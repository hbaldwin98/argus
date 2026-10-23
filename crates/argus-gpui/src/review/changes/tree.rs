//! The Changes pane's Files tab: every file of the checkout's working tree by folder, each changed
//! file marked by its status and each folder by how many changes are under it, narrowed to the
//! changes by the Changed filter, and flat while the find field holds words (the prototype's
//! `filesList` and `treeRows`).
//!
//! The list and the file read whole are the session's (`ReviewTree`); which folders are open, the
//! filter and the find are the window's own. A changed file opens as its change does
//! (`OpenChange`, on the side it is changed on, unstaged first), and a file with no change is read
//! whole in place of the diff (`ReviewRead`).

use std::collections::{BTreeMap, HashMap, HashSet};

use argus_desktop_core::connection::page::{Call, ReviewPage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, SharedString};

use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::review::changes::lists::{badge, status};
use crate::review::changes::side::{Files, Opened};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The most files a find lists: past this, more words are what is wanted, not a longer list.
const HITS: usize = 300;

/// How a file changed, and the list it opens from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub kind: &'static str,
    pub staged: bool,
}

/// One row of the tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Folder { path: String, name: String, depth: usize, open: bool, changed: usize },
    File { path: String, name: String, depth: usize, change: Option<Change> },
    /// A file the find matched, by its whole path, with where the words start in it.
    Hit { path: String, at: usize, change: Option<Change> },
}

/// Each changed file, by path, from the review's lists: its unstaged change over its staged one,
/// since the unstaged is what is still to look at.
pub fn changes(page: &ReviewPage) -> HashMap<String, Change> {
    let mut changes = HashMap::new();
    for section in &page.sections {
        let staged = section.side == "staged";
        for file in &section.files {
            let change = Change { kind: file.file.kind, staged };
            changes.entry(file.file.path.clone()).and_modify(|was: &mut Change| if was.staged && !staged { *was = change }).or_insert(change);
        }
    }
    changes
}

#[derive(Default)]
struct Folder<'a> {
    folders: BTreeMap<&'a str, Folder<'a>>,
    files: Vec<&'a str>,
}

/// The rows drawn: by folder, the open folders' insides under them, or while `query` (lowercased)
/// holds words the files whose path holds it. `changed_only` leaves out what holds no change.
pub fn rows(files: &[String], changes: &HashMap<String, Change>, open: &HashSet<String>, changed_only: bool, query: &str) -> Vec<Row> {
    if !query.is_empty() {
        return files
            .iter()
            .filter_map(|path| {
                let at = path.to_lowercase().find(query)?;
                let change = changes.get(path).copied();
                (!changed_only || change.is_some()).then(|| Row::Hit { path: path.clone(), at, change })
            })
            .take(HITS)
            .collect();
    }
    let mut root = Folder::default();
    for path in files {
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        let mut folder = &mut root;
        for part in parts {
            folder = folder.folders.entry(part).or_default();
        }
        folder.files.push(path);
    }
    let mut out = Vec::new();
    walk(&root, "", 0, changes, open, changed_only, &mut out);
    out
}

fn walk(folder: &Folder, at: &str, depth: usize, changes: &HashMap<String, Change>, open: &HashSet<String>, changed_only: bool, out: &mut Vec<Row>) {
    for (name, inside) in &folder.folders {
        let path = if at.is_empty() { (*name).to_owned() } else { format!("{at}/{name}") };
        let under = format!("{path}/");
        let changed = changes.keys().filter(|changed| changed.starts_with(&under)).count();
        if changed_only && changed == 0 {
            continue;
        }
        let is_open = open.contains(&path);
        out.push(Row::Folder { path: path.clone(), name: (*name).to_owned(), depth, open: is_open, changed });
        if is_open {
            walk(inside, &path, depth + 1, changes, open, changed_only, out);
        }
    }
    let mut files = folder.files.clone();
    files.sort_unstable();
    for path in files {
        let change = changes.get(path).copied();
        if changed_only && change.is_none() {
            continue;
        }
        let name = path.rsplit('/').next().unwrap_or(path).to_owned();
        out.push(Row::File { path: path.to_owned(), name, depth, change });
    }
}

/// The file lit in the tab: the one read whole, else the change on screen.
pub fn lit(page: &ReviewPage) -> Option<String> {
    let reading = page.tree.as_ref().and_then(|tree| tree.reading.as_ref()).map(|reading| reading.path.clone());
    reading.or_else(|| page.file.and_then(|file| page.files.get(file)).map(|file| file.path.clone()))
}

impl Files {
    /// The tab's list: loading words until the first list is in, then the rows.
    pub(super) fn checkout(&self, page: &ReviewPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let list = v_flex().id("review-tree").test_support().role(gpui_kit::Role::Tree).flex_1().min_h_0().overflow_y_scroll().pt(rem(tokens.sp_4)).pb(rem(10.0)).gap(rem(tokens.sp_2));
        let Some(tree) = page.tree.as_ref().filter(|tree| tree.listed) else {
            return list.child(said("review-tree-loading", "Reading the checkout\u{2026}", tokens));
        };
        let changes = changes(page);
        let rows = rows(&tree.files, &changes, &self.folders, self.changed_only, &self.query);
        let lit = lit(page);
        if rows.is_empty() {
            let words = if self.query.is_empty() { "No file here." } else { "No file in the checkout matches." };
            return list.child(said("review-tree-none", words, tokens));
        }
        list.children(rows.into_iter().enumerate().map(|(at, row)| self.row(at, row, lit.as_deref(), tokens, cx)))
    }

    fn row(&self, at: usize, row: Row, lit: Option<&str>, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let base = |depth: usize| {
            h_flex()
                .id(("review-tree-row", at))
                .flex_shrink_0()
                .h(rem(26.0))
                .mx(rem(tokens.sp_6))
                .pl(rem(tokens.sp_8 + depth as f32 * 14.0))
                .pr(rem(tokens.sp_8))
                .gap(rem(tokens.sp_6))
                .items_center()
                .rounded(rem(tokens.r_sm))
                .border_1()
                .typed(tokens.type_body)
                .cursor_pointer()
        };
        match row {
            Row::Folder { path, name, depth, open, changed } => base(depth)
                .test_support()
                .aria_label(SharedString::from(path.clone()))
                .aria_expanded(open)
                .border_color(gpui_kit::transparent_black())
                .text_color(tokens.fg_muted)
                .hover(|style| style.bg(tokens.fill.opacity(0.6)).text_color(tokens.fg))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.folders.remove(&path) {
                        this.folders.insert(path.clone());
                    }
                    cx.notify();
                }))
                .child(div().flex_shrink_0().text_color(tokens.fg_faint).child(if open { Glyph::Down.at(12.0) } else { Glyph::Right.at(12.0) }))
                .child(div().flex_shrink_0().child(Glyph::Folder.at(14.0)))
                .child(div().flex_1().min_w_0().truncate().child(SharedString::from(name)))
                .when(changed > 0, |row| {
                    row.child(div().flex_shrink_0().font_family(SharedString::from(tokens.font_mono)).text_size(rem(10.5)).text_color(tokens.changed).child(SharedString::from(changed.to_string())))
                })
                .into_any_element(),
            Row::File { path, name, depth, change } => {
                let shown = SharedString::from(name);
                self.file(base(depth).child(div().flex_shrink_0().w(rem(12.0))), at, path, change, lit, cx).test_support().child(shown_name(shown, change.is_some(), tokens)).when(change.is_some(), |row| row.child(dot(tokens))).into_any_element()
            }
            Row::Hit { path, at: from, change } => {
                let to = from + self.query.len();
                let (before, hit, after) = (path[..from].to_owned(), path[from..to].to_owned(), path[to..].to_owned());
                self.file(base(0), at, path, change, lit, cx)
                    .test_support()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .typed(tokens.type_small)
                            .text_color(tokens.fg_muted)
                            .child(SharedString::from(before))
                            .child(div().text_color(tokens.fg).bg(tokens.attention.opacity(0.25)).child(SharedString::from(hit)))
                            .child(div().min_w_0().truncate().child(SharedString::from(after))),
                    )
                    .into_any_element()
            }
        }
    }

    /// A file's row: lit when it is on screen, its status or a file's mark, and the press that
    /// opens its change or reads it whole.
    fn file(&self, row: gpui_kit::Stateful<gpui_kit::Div>, at: usize, path: String, change: Option<Change>, lit: Option<&str>, cx: &mut Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        let tokens = &{ *Tokens::global(cx) };
        let on = lit == Some(path.as_str());
        let (fill, selected_hover) = (tokens.fill, tokens.selected_hover);
        let (mark, label) = match change {
            Some(change) => {
                let (letter, colour, said) = status(change.kind, tokens);
                (badge(("review-tree-status", at), letter, colour, said, tokens, cx).into_any_element(), format!("{path}, {}", said.to_lowercase()))
            }
            None => (div().flex_shrink_0().text_color(tokens.fg_faint).child(Glyph::File.at(14.0)).into_any_element(), path.clone()),
        };
        let tooltip = SharedString::from(path.clone());
        row.aria_label(SharedString::from(label))
            .aria_selected(on)
            .border_color(if on { tokens.line_strong } else { gpui_kit::transparent_black() })
            .text_color(if on { tokens.fg } else { tokens.fg_muted })
            .when(on, |row| row.bg(tokens.bg_selected).hover(move |style| style.bg(selected_hover)))
            .when(!on, |row| row.hover(move |style| style.bg(fill.opacity(0.6)).text_color(tokens.fg)))
            .tooltip(tip(tooltip))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.emit(Opened);
                let call = match change {
                    Some(change) => Call::OpenChange { staged: change.staged, path: path.clone() },
                    None => Call::ReviewRead { path: path.clone() },
                };
                this.ask(call, cx);
            }))
            .child(mark)
    }
}

fn shown_name(name: SharedString, changed: bool, tokens: &Tokens) -> impl IntoElement {
    div().flex_1().min_w_0().truncate().when(changed, |name| name.text_color(tokens.fg).weighted(tokens.weight_medium)).child(name)
}

/// `.dotc`: a changed file's mark at the row's end.
fn dot(tokens: &Tokens) -> impl IntoElement {
    div().flex_shrink_0().size(rem(6.0)).rounded_full().bg(tokens.changed)
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

    fn changed(list: &[(&str, &'static str, bool)]) -> HashMap<String, Change> {
        list.iter().map(|(path, kind, staged)| ((*path).to_owned(), Change { kind, staged: *staged })).collect()
    }

    fn said(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Folder { name, depth, open, changed, .. } => format!("{}{}{name}/{}", "  ".repeat(*depth), if *open { "v " } else { "> " }, if *changed > 0 { format!(" {changed}") } else { String::new() }),
                Row::File { name, depth, change, .. } => format!("{}  {name}{}", "  ".repeat(*depth), if change.is_some() { " *" } else { "" }),
                Row::Hit { path, at, .. } => format!("{path} @{at}"),
            })
            .collect()
    }

    const FILES: [&str; 5] = ["README.md", "crates/a/src/lib.rs", "crates/a/src/main.rs", "crates/b/lib.rs", "Cargo.toml"];

    #[test]
    fn folders_come_first_shut_with_how_many_changes_are_under_them_then_the_files_sorted() {
        let changes = changed(&[("crates/a/src/lib.rs", "modified", false), ("crates/b/lib.rs", "added", true)]);
        let rows = rows(&paths(&FILES), &changes, &HashSet::new(), false, "");
        assert_eq!(said(&rows), ["> crates/ 2", "  Cargo.toml", "  README.md"]);
    }

    #[test]
    fn an_open_folder_shows_its_insides_under_it_a_level_in() {
        let changes = changed(&[("crates/a/src/lib.rs", "modified", false)]);
        let open: HashSet<String> = ["crates", "crates/a"].into_iter().map(String::from).collect();
        let rows = rows(&paths(&FILES), &changes, &open, false, "");
        assert_eq!(said(&rows), ["v crates/ 1", "  v a/ 1", "    > src/ 1", "  > b/", "  Cargo.toml", "  README.md"]);
    }

    #[test]
    fn changed_only_leaves_out_what_holds_no_change() {
        let changes = changed(&[("crates/a/src/lib.rs", "modified", false)]);
        let open: HashSet<String> = ["crates", "crates/a", "crates/a/src"].into_iter().map(String::from).collect();
        let rows = rows(&paths(&FILES), &changes, &open, true, "");
        assert_eq!(said(&rows), ["v crates/ 1", "  v a/ 1", "    v src/ 1", "        lib.rs *"]);
    }

    #[test]
    fn a_find_lists_every_file_whose_path_holds_the_words_flat_with_where_they_start() {
        let changes = changed(&[("crates/b/lib.rs", "added", false)]);
        let rows = rows(&paths(&FILES), &changes, &HashSet::new(), false, "lib");
        assert_eq!(said(&rows), ["crates/a/src/lib.rs @13", "crates/b/lib.rs @9"]);
        assert_eq!(said(&super::rows(&paths(&FILES), &changes, &HashSet::new(), true, "lib")), ["crates/b/lib.rs @9"], "the filter holds while finding");
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
        assert_eq!(changes["a.rs"], Change { kind: "modified", staged: false });
        assert_eq!(changes["b.rs"], Change { kind: "modified", staged: true });
    }
}
