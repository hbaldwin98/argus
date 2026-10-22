//! What the Code stage's file area says for the file read: which of its states it is in, the header's
//! name, version and facts, and the rows of the file with the places lines were removed between them
//! (`code/stage.ts`, `file`, `header`, `line`, `gone`).
//!
//! The page spelled this out where it built its elements, and a window that draws the file would
//! have spelled it again. The rows are the diff's (`DiffRow`), so one view draws a review, a
//! history, a patch block and a file read whole.

use crate::code::stage::CodePage;
use crate::connection::page::{DiffRow, DiffSide};

/// What fills the file area, the first of these that holds (CODE-35).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Area {
    /// The page's editor stands in for the lines; the window that has none draws the lines.
    Editor,
    /// The reason the file could not be read, with no lines to show instead.
    Failed(String),
    /// No file is read: the match card or the start panel.
    Start,
    /// A file is being read and nothing of it has arrived: five loading rows.
    Loading,
    /// `<path>: <note>`: binary, or too large.
    Note(String),
    Lines,
}

pub fn area(page: &CodePage) -> Area {
    if page.editing && page.path.is_some() {
        return Area::Editor;
    }
    if let Some(why) = page.failed.as_ref().filter(|_| page.lines.is_empty()) {
        return Area::Failed(why.clone());
    }
    let Some(path) = &page.path else {
        return Area::Start;
    };
    if page.file_loading && page.lines.is_empty() {
        return Area::Loading;
    }
    match &page.note {
        Some(note) => Area::Note(format!("{path}: {note}")),
        None => Area::Lines,
    }
}

/// The header is drawn while a file is open or the stage says where `B` goes back to.
pub fn has_header(page: &CodePage) -> bool {
    page.path.is_some() || page.back.is_some()
}

/// A path as the header says it: the name first, then the folder, which is nothing for a file at the
/// root.
pub fn path_parts(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(at) if at > 0 => (&path[at + 1..], &path[..at]),
        Some(at) => (&path[at + 1..], ""),
        None => (path, ""),
    }
}

/// What the header knows of the file, right-aligned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The facts joined by ` · `, with a last ` · ` when the counts follow.
    pub lead: String,
    pub added: u32,
    pub removed: u32,
}

/// `Match 2 of 3 in this file`, `Ln 52`, `Rust`, `207 lines`, then `+4 −1`. The editor's cursor is
/// its own, so with `editor` up (the window has one standing in for the lines) Rust's line is not
/// said: it would be stale beside it.
pub fn facts(page: &CodePage, editor: bool) -> Facts {
    let here = page.search.as_ref().and_then(|search| search.files.iter().find(|file| Some(&file.path) == page.path.as_ref()));
    let lit = here.and_then(|file| file.hits.iter().position(|hit| hit.on).map(|at| (file, at)));
    let said: Vec<String> = [
        lit.map(|(file, at)| format!("Match {} of {} in this file", at + 1, file.hits.len())),
        (!editor && !page.lines.is_empty()).then(|| format!("Ln {}", page.at)),
        page.language.map(str::to_owned),
        (!page.lines.is_empty()).then(|| format!("{} lines", page.lines.len())),
    ]
    .into_iter()
    .flatten()
    .collect();
    let counted = page.added > 0 || page.removed > 0;
    let mut lead = said.join(" \u{b7} ");
    if counted {
        lead.push_str(" \u{b7} ");
    }
    Facts { lead, added: page.added, removed: page.removed }
}

/// `− 3 lines removed here`.
pub fn gone(removed: u32) -> String {
    format!("\u{2212} {removed} {} removed here", if removed == 1 { "line" } else { "lines" })
}

/// Every row of the file in order: a line is a row of its number, `+` when this version added or
/// changed it, and its highlighted text; the lines removed just above one are a note before it,
/// and those after the last line one note after it. A line's `row` is its number, which is what a
/// click on it sends back; a note's is 0, and is never clicked.
pub fn rows(page: &CodePage) -> Vec<DiffRow> {
    let note = |removed: u32| DiffRow { row: 0, kind: "note", text: gone(removed), hidden: None, step: None, left: None, right: None, cursor: false, picked: false };
    let mut out = Vec::with_capacity(page.lines.len() + 1);
    for line in &page.lines {
        if line.removed_before > 0 {
            out.push(note(line.removed_before));
        }
        out.push(DiffRow {
            row: line.no as usize,
            kind: "line",
            text: String::new(),
            hidden: None,
            step: None,
            left: Some(DiffSide {
                old: None,
                new: Some(line.no),
                marker: if line.added { "+" } else { "" },
                kind: if line.added { "added" } else { "context" },
                parts: line.parts.clone(),
            }),
            right: None,
            cursor: line.cursor,
            picked: line.picked,
        });
    }
    if page.removed_at_end > 0 {
        out.push(note(page.removed_at_end));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::stage::{CodeHit, CodeHitFile, CodeRow, CodeSearch};

    fn line(no: u32, text: &str) -> CodeRow {
        CodeRow { no, parts: vec![(text.into(), "")], added: false, removed_before: 0, cursor: false, picked: false }
    }

    fn reading() -> CodePage {
        CodePage { open: true, path: Some("src/a.rs".into()), lines: vec![line(1, "a"), line(2, "b")], at: 2, ..CodePage::default() }
    }

    /// CODE-35: the first state that holds wins.
    #[test]
    fn the_area_is_the_first_state_that_holds() {
        assert_eq!(area(&reading()), Area::Lines);
        assert_eq!(area(&CodePage { path: None, ..reading() }), Area::Start);
        let failed = CodePage { failed: Some("gone".into()), ..reading() };
        assert_eq!(area(&failed), Area::Lines, "a failure with lines shown leaves them");
        assert_eq!(area(&CodePage { lines: Vec::new(), ..failed }), Area::Failed("gone".into()));
        let waiting = CodePage { lines: Vec::new(), file_loading: true, ..reading() };
        assert_eq!(area(&waiting), Area::Loading);
        let binary = CodePage { lines: Vec::new(), file_loading: true, note: Some("binary file".into()), ..reading() };
        assert_eq!(area(&binary), Area::Loading, "a read still out is loading before it is a note");
        assert_eq!(area(&CodePage { file_loading: false, ..binary }), Area::Note("src/a.rs: binary file".into()));
        assert_eq!(area(&CodePage { editing: true, ..reading() }), Area::Editor);
        assert_eq!(area(&CodePage { editing: true, path: None, ..reading() }), Area::Start);
    }

    #[test]
    fn the_header_is_drawn_for_a_file_or_a_way_back() {
        assert!(has_header(&reading()));
        assert!(!has_header(&CodePage::default()));
        assert!(has_header(&CodePage { back: Some("history".into()), ..CodePage::default() }));
    }

    #[test]
    fn a_path_is_a_name_and_the_folder_before_it() {
        assert_eq!(path_parts("crates/a/src/b.rs"), ("b.rs", "crates/a/src"));
        assert_eq!(path_parts("README.md"), ("README.md", ""));
        assert_eq!(path_parts("/x"), ("x", ""), "a leading slash is no folder");
    }

    /// CODE-40: the facts, in the page's order, joined by a middle dot, the counts after.
    #[test]
    fn the_facts_are_joined_and_the_counts_follow() {
        let page = CodePage { language: Some("Rust"), added: 4, removed: 1, ..reading() };
        assert_eq!(facts(&page, false), Facts { lead: "Ln 2 \u{b7} Rust \u{b7} 2 lines \u{b7} ".into(), added: 4, removed: 1 });
        assert_eq!(facts(&reading(), false).lead, "Ln 2 \u{b7} 2 lines");
        assert_eq!(facts(&CodePage { lines: Vec::new(), ..reading() }, false).lead, "");
        assert_eq!(facts(&reading(), true).lead, "2 lines", "the editor's cursor is its own");
        assert_eq!(facts(&CodePage { editing: true, ..reading() }, false).lead, "Ln 2 \u{b7} 2 lines", "a window with no editor reads the lines");
    }

    #[test]
    fn the_match_in_this_file_is_counted_among_the_file_s_hits() {
        let hit = |index, on| CodeHit { index, line: index as u32 + 1, parts: Vec::new(), on };
        let file = |path: &str, hits| CodeHitFile { path: path.into(), dir: String::new(), name: path.into(), count: 2, open: true, hits };
        let search = CodeSearch { files: vec![file("other.rs", vec![hit(0, true)]), file("src/a.rs", vec![hit(1, false), hit(2, true)])], ..CodeSearch::default() };
        let page = CodePage { search: Some(search), ..reading() };
        assert!(facts(&page, false).lead.starts_with("Match 2 of 2 in this file \u{b7} Ln 2"), "{}", facts(&page, false).lead);
        let none = CodePage { search: Some(CodeSearch { files: vec![file("src/a.rs", vec![hit(1, false)])], ..CodeSearch::default() }), ..reading() };
        assert!(!facts(&none, false).lead.contains("Match"), "no hit lit in this file");
    }

    /// CODE-37.
    #[test]
    fn removed_lines_are_a_note_above_the_line_after_them_and_one_after_the_last() {
        let mut page = reading();
        page.lines[1].removed_before = 1;
        page.removed_at_end = 3;
        let rows = rows(&page);
        let kinds: Vec<_> = rows.iter().map(|r| (r.kind, r.row)).collect();
        assert_eq!(kinds, [("line", 1), ("note", 0), ("line", 2), ("note", 0)]);
        assert_eq!(rows[1].text, "\u{2212} 1 line removed here");
        assert_eq!(rows[3].text, "\u{2212} 3 lines removed here");
    }

    /// CODE-36: a line has its number, a `+` when added, and the cursor and pick as the session says.
    #[test]
    fn a_line_row_carries_its_number_mark_and_state() {
        let mut page = reading();
        page.lines[0].added = true;
        page.lines[0].parts = vec![("fn ".into(), "k"), ("a".into(), "f")];
        page.lines[1].cursor = true;
        page.lines[1].picked = true;
        let rows = rows(&page);
        let first = rows[0].left.as_ref().unwrap();
        assert_eq!((first.new, first.old, first.marker, first.kind), (Some(1), None, "+", "added"));
        assert_eq!(first.parts, [("fn ".to_owned(), "k"), ("a".to_owned(), "f")]);
        let second = rows[1].left.as_ref().unwrap();
        assert_eq!((second.marker, second.kind), ("", "context"));
        assert!(rows[1].cursor && rows[1].picked && !rows[0].cursor);
    }
}
