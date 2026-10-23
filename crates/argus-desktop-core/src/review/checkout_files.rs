//! The checkout's files as the Changes pane's Files tab reads them: every file of the working tree,
//! listed each time the tab is shown, and one file read whole. A changed file opens as its change
//! does, so only a file the review has no diff of is read here; opening a change puts it away.
//!
//! The list and the file are the Code stage's asks (`ListCode`, `ReadCode`), answered by request
//! id, so an answer to an older ask is dropped and the list on screen stays until the next is in.

use argus_protocol::{CheckoutId, ClientMsg, CodeFile, CodeTree, CodeVersion};

use crate::connection::page::{DiffRow, DiffSide, ReviewReading, ReviewTree};
use crate::review::rows::parts;

#[derive(Debug, Default)]
pub struct CheckoutFiles {
    /// The last list in; none before the first.
    files: Option<Vec<String>>,
    list_asked: Option<u64>,
    reading: Option<Reading>,
}

#[derive(Debug)]
struct Reading {
    path: String,
    asked: Option<u64>,
    file: Option<Box<CodeFile>>,
    refused: Option<String>,
}

impl CheckoutFiles {
    /// The tab shown: the list asked for again, unless an ask is out.
    pub fn list(&mut self, checkout: CheckoutId, request_id: u64) -> Option<ClientMsg> {
        if self.list_asked.is_some() {
            return None;
        }
        self.list_asked = Some(request_id);
        Some(ClientMsg::ListCode { request_id, checkout, version: CodeVersion::WorkingTree })
    }

    /// `path` read whole, in place of the diff.
    pub fn read(&mut self, checkout: CheckoutId, path: String, request_id: u64) -> ClientMsg {
        self.reading = Some(Reading { path: path.clone(), asked: Some(request_id), file: None, refused: None });
        ClientMsg::ReadCode { request_id, checkout, path, version: CodeVersion::WorkingTree }
    }

    /// A change opened: the file read gives way to its diff.
    pub fn put_away(&mut self) {
        self.reading = None;
    }

    /// The file read whole, while one is.
    pub fn reading(&self) -> Option<&str> {
        self.reading.as_ref().map(|r| r.path.as_str())
    }

    /// The list, if it was this tab's ask.
    pub fn receive_tree(&mut self, tree: CodeTree) -> Result<(), CodeTree> {
        if self.list_asked != Some(tree.request_id) {
            return Err(tree);
        }
        self.list_asked = None;
        self.files = Some(tree.files);
        Ok(())
    }

    /// The file, if it is the one this tab is reading.
    pub fn receive_file(&mut self, file: Box<CodeFile>) -> Result<(), Box<CodeFile>> {
        match &mut self.reading {
            Some(reading) if reading.asked == Some(file.request_id) => {
                reading.asked = None;
                reading.file = Some(file);
                Ok(())
            }
            _ => Err(file),
        }
    }

    /// Whether a refused ask was this tab's: a list that will not come stops loading, and a file says
    /// why in place of its lines.
    pub fn refused(&mut self, request_id: u64, reason: &str) -> bool {
        if self.list_asked == Some(request_id) {
            self.list_asked = None;
            return true;
        }
        match &mut self.reading {
            Some(reading) if reading.asked == Some(request_id) => {
                reading.asked = None;
                reading.refused = Some(reason.to_owned());
                true
            }
            _ => false,
        }
    }

    pub fn page(&self) -> ReviewTree {
        ReviewTree {
            files: self.files.clone().unwrap_or_default(),
            listed: self.files.is_some(),
            loading: self.list_asked.is_some() && self.files.is_none(),
            reading: self.reading.as_ref().map(|reading| ReviewReading {
                path: reading.path.clone(),
                rows: reading.file.as_deref().map(rows).unwrap_or_default(),
                loading: reading.asked.is_some(),
                note: reading.refused.clone().or_else(|| reading.file.as_ref().and_then(|f| f.note.clone())),
            }),
        }
    }
}

/// The file's lines as the diff draws a file read whole: each a line on its own, numbered.
fn rows(file: &CodeFile) -> Vec<DiffRow> {
    file.lines
        .iter()
        .enumerate()
        .map(|(at, line)| DiffRow {
            row: at + 1,
            kind: "line",
            text: String::new(),
            hidden: None,
            step: None,
            left: Some(DiffSide {
                old: None,
                new: Some(at as u32 + 1),
                marker: if line.added { "+" } else { "" },
                kind: if line.added { "added" } else { "context" },
                parts: parts(&line.text, &line.spans),
            }),
            right: None,
            cursor: false,
            picked: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::CodeLine;

    const CO: CheckoutId = CheckoutId(100);

    fn tree(request_id: u64, files: &[&str]) -> CodeTree {
        CodeTree { request_id, checkout: CO, version: CodeVersion::WorkingTree, files: files.iter().map(|f| (*f).to_owned()).collect(), changes: Vec::new() }
    }

    fn file(request_id: u64, path: &str, lines: &[&str]) -> CodeFile {
        CodeFile {
            request_id,
            checkout: CO,
            path: path.into(),
            version: CodeVersion::WorkingTree,
            lines: lines.iter().map(|text| CodeLine { text: (*text).into(), spans: Vec::new(), added: false }).collect(),
            removed: Vec::new(),
            symbols: Vec::new(),
            note: None,
        }
    }

    #[test]
    fn the_tab_unshown_lists_nothing_and_shown_asks_once_while_an_ask_is_out() {
        let mut files = CheckoutFiles::default();
        assert!(!files.page().listed && !files.page().loading);
        assert!(matches!(files.list(CO, 1), Some(ClientMsg::ListCode { request_id: 1, version: CodeVersion::WorkingTree, .. })));
        assert!(files.list(CO, 2).is_none(), "one ask out at a time");
        assert!(files.page().loading);
        assert!(files.receive_tree(tree(1, &["a.rs", "src/b.rs"])).is_ok());
        assert_eq!(files.page().files, ["a.rs", "src/b.rs"]);
        assert!(files.list(CO, 3).is_some(), "shown again, listed again");
        let page = files.page();
        assert!(page.listed && !page.loading && page.files.len() == 2, "the last list stays until the next is in");
    }

    #[test]
    fn an_answer_to_another_ask_is_given_back() {
        let mut files = CheckoutFiles::default();
        files.list(CO, 1);
        assert!(files.receive_tree(tree(9, &["x"])).is_err());
        assert!(files.receive_file(Box::new(file(9, "x", &[]))).is_err(), "nothing is being read");
        files.read(CO, "a.rs".into(), 2);
        assert!(files.receive_file(Box::new(file(3, "a.rs", &[]))).is_err());
    }

    #[test]
    fn a_file_read_whole_is_its_numbered_lines_until_a_change_opens() {
        let mut files = CheckoutFiles::default();
        files.list(CO, 1);
        assert!(matches!(files.read(CO, "a.rs".into(), 2), ClientMsg::ReadCode { request_id: 2, ref path, .. } if path == "a.rs"));
        assert!(files.page().reading.is_some_and(|r| r.loading && r.rows.is_empty()));
        assert!(files.receive_file(Box::new(file(2, "a.rs", &["one", "two"]))).is_ok());
        let reading = files.page().reading.expect("reading");
        let numbers: Vec<Option<u32>> = reading.rows.iter().map(|row| row.left.as_ref().and_then(|side| side.new)).collect();
        assert_eq!(numbers, [Some(1), Some(2)]);
        assert!(!reading.loading);
        files.put_away();
        assert_eq!(files.reading(), None);
    }

    #[test]
    fn a_refused_read_says_why_in_place_of_its_lines_and_a_refused_list_stops_loading() {
        let mut files = CheckoutFiles::default();
        files.list(CO, 1);
        assert!(files.refused(1, "no such checkout"));
        assert!(!files.page().loading);
        files.read(CO, "a.rs".into(), 2);
        assert!(!files.refused(7, "not mine"));
        assert!(files.refused(2, "a.rs is not there"));
        assert_eq!(files.page().reading.and_then(|r| r.note), Some("a.rs is not there".into()));
    }
}
