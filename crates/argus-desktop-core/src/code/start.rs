//! What the Code stage's file area says while no file is read: what changed here, the files opened
//! lately, the hint, and the card that offers the match the search's cursor is on (`code/stage.ts`,
//! `start`, `remember` and `selectedHit`). The files opened lately are the window's to keep, not
//! the daemon's and not saved, so [`Opened`] is what a window holds beside the page it hears.

use std::collections::HashMap;

use super::stage::{CodeChangedFile, CodePage};
use crate::chrome::strip::Part;

/// How many changed files the start panel lists before it says how many more there are.
pub const CHANGED_SHOWN: usize = 10;

/// How many files opened lately a checkout remembers.
pub const OPENED_KEPT: usize = 8;

/// The hint under the lists, with the key the field takes between its two halves.
pub const HINT: (&str, &str, &str) = ("Type in the field to go to a file by name; start with ", "#", " to search the checkout's text instead.");

/// The files read in each checkout while this window has been open, newest first.
#[derive(Debug, Default, Clone)]
pub struct Opened(HashMap<u64, Vec<String>>);

impl Opened {
    /// Notes the file `page` is reading. Whether the list changed.
    pub fn remember(&mut self, page: &CodePage) -> bool {
        let Some(path) = &page.path else { return false };
        let was = self.0.entry(page.checkout).or_default();
        if was.first() == Some(path) {
            return false;
        }
        was.retain(|held| held != path);
        was.insert(0, path.clone());
        was.truncate(OPENED_KEPT);
        true
    }

    /// The files opened in `checkout`, newest first.
    pub fn lately(&self, checkout: u64) -> &[String] {
        self.0.get(&checkout).map_or(&[], Vec::as_slice)
    }
}

/// A file in a list: its name, then its folder, and for a changed file how much.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLine {
    pub path: String,
    pub name: String,
    /// Its folder, or the phrase for the root.
    pub dir: String,
    pub counts: Option<(u32, u32)>,
}

fn root_phrase(dir: &str) -> String {
    if dir.is_empty() { "repository root".to_owned() } else { dir.to_owned() }
}

fn changed_line(file: &CodeChangedFile) -> FileLine {
    FileLine { path: file.path.clone(), name: file.name.clone(), dir: root_phrase(&file.dir), counts: Some((file.added, file.removed)) }
}

fn lately_line(path: &str) -> FileLine {
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    FileLine { path: path.to_owned(), name: name.to_owned(), dir: root_phrase(dir), counts: None }
}

/// The start panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    /// `Changed on main`.
    pub heading: String,
    pub changed: Vec<FileLine>,
    /// `and 3 more, under Changed in the tree.`, when the list was cut.
    pub more: Option<String>,
    /// What stands in for the list when nothing changed: `Nothing since 9d41c07.` or `No changes.`
    pub nothing: Vec<Part>,
    pub lately: Vec<FileLine>,
}

pub fn start(page: &CodePage, opened: &Opened) -> Start {
    let heading = format!("Changed on {}", if page.branch.is_empty() { &page.name } else { &page.branch });
    let changed: Vec<FileLine> = page.changed_files.iter().take(CHANGED_SHOWN).map(changed_line).collect();
    let cut = page.changed_files.len().saturating_sub(CHANGED_SHOWN);
    let nothing = match (&page.since, changed.is_empty()) {
        (_, false) => Vec::new(),
        (Some(since), true) => vec![Part::Text("Nothing since ".into()), Part::Mono(since.clone()), Part::Text(".".into())],
        (None, true) => vec![Part::Text("No changes.".into())],
    };
    Start {
        heading,
        changed,
        more: (cut > 0).then(|| format!("and {cut} more, under Changed in the tree.")),
        nothing,
        lately: opened.lately(page.checkout).iter().map(|path| lately_line(path)).collect(),
    }
}

/// The search's match under the cursor, which the file area offers to open when no file is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// `name:line`.
    pub place: String,
    /// Which match it is, in the order the search returned them.
    pub index: usize,
}

/// The selected match and the file it is in, while the results are the ones for what the field holds.
pub fn selected_match(page: &CodePage) -> Option<Selected> {
    let search = page.search.as_ref().filter(|search| search.current)?;
    search.files.iter().find_map(|file| file.hits.iter().find(|hit| hit.on).map(|hit| Selected { place: format!("{}:{}", file.name, hit.line), index: hit.index }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::stage::{CodeHit, CodeHitFile, CodeSearch};

    fn page(changed: usize) -> CodePage {
        CodePage {
            open: true,
            checkout: 7,
            name: "main".into(),
            branch: "feature".into(),
            changed_files: (0..changed)
                .map(|n| CodeChangedFile { path: format!("src/f{n}.rs"), name: format!("f{n}.rs"), dir: "src".into(), added: n as u32, removed: 1 })
                .collect(),
            ..CodePage::default()
        }
    }

    #[test]
    fn the_changed_list_is_cut_at_ten_and_says_how_many_more() {
        let said = start(&page(13), &Opened::default());
        assert_eq!(said.heading, "Changed on feature");
        assert_eq!(said.changed.len(), 10);
        assert_eq!(said.more.as_deref(), Some("and 3 more, under Changed in the tree."));
        assert_eq!(start(&page(10), &Opened::default()).more, None, "ten fit");
    }

    #[test]
    fn nothing_changed_says_since_when_it_is_known() {
        let mut quiet = page(0);
        assert_eq!(start(&quiet, &Opened::default()).nothing, [Part::Text("No changes.".into())]);
        quiet.since = Some("9d41c07".into());
        assert_eq!(start(&quiet, &Opened::default()).nothing[1], Part::Mono("9d41c07".into()));
        assert!(start(&page(1), &Opened::default()).nothing.is_empty());
    }

    #[test]
    fn a_checkout_with_no_branch_is_named_by_its_name_and_the_root_by_a_phrase() {
        let mut page = page(1);
        page.branch.clear();
        page.changed_files[0].dir = String::new();
        let said = start(&page, &Opened::default());
        assert_eq!(said.heading, "Changed on main");
        assert_eq!(said.changed[0].dir, "repository root");
        assert_eq!(said.changed[0].counts, Some((0, 1)));
    }

    #[test]
    fn the_files_opened_lately_are_newest_first_without_repeats_and_eight_at_most() {
        let mut opened = Opened::default();
        let mut reading = page(0);
        for path in ["a.rs", "src/b.rs", "a.rs", "a.rs"] {
            reading.path = Some(path.into());
            opened.remember(&reading);
        }
        assert_eq!(opened.lately(7), ["a.rs", "src/b.rs"]);
        for n in 0..12 {
            reading.path = Some(format!("f{n}.rs"));
            opened.remember(&reading);
        }
        assert_eq!(opened.lately(7).len(), OPENED_KEPT);
        assert_eq!(opened.lately(7)[0], "f11.rs");
        assert!(opened.lately(8).is_empty(), "another checkout has its own");
    }

    #[test]
    fn remembering_the_same_file_again_changes_nothing_and_no_file_remembers_nothing() {
        let mut opened = Opened::default();
        let mut reading = page(0);
        assert!(!opened.remember(&reading));
        reading.path = Some("a.rs".into());
        assert!(opened.remember(&reading));
        assert!(!opened.remember(&reading));
    }

    #[test]
    fn a_lately_row_is_named_by_its_file_and_placed_by_its_folder() {
        let mut opened = Opened::default();
        let mut reading = page(0);
        reading.path = Some("crates/argusd/src/main.rs".into());
        opened.remember(&reading);
        reading.path = Some("README.md".into());
        opened.remember(&reading);
        let said = start(&reading, &opened);
        assert_eq!((said.lately[0].name.as_str(), said.lately[0].dir.as_str()), ("README.md", "repository root"));
        assert_eq!((said.lately[1].name.as_str(), said.lately[1].dir.as_str()), ("main.rs", "crates/argusd/src"));
    }

    fn searching(current: bool) -> CodePage {
        let hit = |index, line, on| CodeHit { index, line, parts: vec![("x".into(), true)], on };
        CodePage {
            search: Some(CodeSearch {
                current,
                files: vec![CodeHitFile { path: "src/a.rs".into(), dir: "src/".into(), name: "a.rs".into(), count: 2, open: true, hits: vec![hit(0, 3, false), hit(1, 9, true)] }],
                ..CodeSearch::default()
            }),
            ..CodePage::default()
        }
    }

    #[test]
    fn the_card_offers_the_lit_match_only_while_the_results_are_current() {
        assert_eq!(selected_match(&searching(true)), Some(Selected { place: "a.rs:9".into(), index: 1 }));
        assert_eq!(selected_match(&searching(false)), None);
        assert_eq!(selected_match(&CodePage::default()), None);
    }
}
