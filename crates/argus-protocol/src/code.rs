//! A checkout's code as the Code stage reads it: which version of its
//! files, a file's lines and the ones that version changed, what the file
//! defines, and where text occurs across the checkout. Read-only; changing
//! code is the editor's.

use serde::{Deserialize, Serialize};

use crate::ids::CheckoutId;
use crate::review::HighlightSpan;

/// Most matches one search returns: past this, a narrower query is what
/// the person wants, not a longer list.
pub const MAX_CODE_MATCHES: usize = 500;

/// Which of a checkout's versions is read. Each is shown against a base,
/// and the lines it changed against that base are marked: the working tree
/// and the index against `HEAD`, a commit against its first parent. `HEAD`
/// itself has no base, so nothing in it is marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeVersion {
    WorkingTree,
    Index,
    Head,
    /// A commit, by its oid.
    Commit(String),
}

/// One line of a file as read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLine {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<HighlightSpan>,
    /// Added or changed by this version against its base.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub added: bool,
}

/// Lines this version removed, which have nowhere of their own to be drawn:
/// `count` of them sat just before line `before`, numbered from one, or
/// after the last line when `before` is one past it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Removed {
    pub before: u32,
    pub count: u32,
}

/// Something a file defines, as tree-sitter's tag queries find it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeSymbol {
    pub name: String,
    /// The grammar's own word for it — `function`, `class`, `method`,
    /// `interface`, `module` — which the client shows rather than maps.
    pub kind: String,
    /// Numbered from one.
    pub line: u32,
}

/// One file at one version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeFile {
    pub request_id: u64,
    pub checkout: CheckoutId,
    pub path: String,
    pub version: CodeVersion,
    pub lines: Vec<CodeLine>,
    pub removed: Vec<Removed>,
    pub symbols: Vec<CodeSymbol>,
    /// Why `lines` is empty though the file is there: binary, or too large.
    pub note: Option<String>,
}

/// How much one file changed against its version's base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeChange {
    pub path: String,
    pub added: u32,
    pub removed: u32,
}

/// Every file of one version, and the ones it changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeTree {
    pub request_id: u64,
    pub checkout: CheckoutId,
    pub version: CodeVersion,
    /// Repo-relative, forward-slashed, sorted.
    pub files: Vec<String>,
    pub changes: Vec<CodeChange>,
}

/// A line text was found on. `start..end` is the first match on it, in
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeMatch {
    pub path: String,
    pub line: u32,
    pub text: String,
    pub start: u32,
    pub end: u32,
}

/// Where a search's text occurs in a checkout's working tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeMatches {
    pub request_id: u64,
    pub checkout: CheckoutId,
    pub query: String,
    pub matches: Vec<CodeMatch>,
    /// More matched than [`MAX_CODE_MATCHES`].
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unchanged_plain_line_is_sent_as_its_text_alone() {
        // Most lines of most files are neither changed nor highlighted, and
        // a file is sent whole.
        let line = CodeLine { text: "x".into(), spans: Vec::new(), added: false };
        let bytes = rmp_serde::to_vec_named(&line).unwrap();
        let back: CodeLine = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(back, line);
        assert!(bytes.len() < 12, "{} bytes", bytes.len());
    }
}
