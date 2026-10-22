//! What the Markdown of an agent's view is read into: blocks, and inline marks inside them.
//!
//! The text is an agent's, read from somewhere nobody here controls, so the model has no place for
//! anything a window could be tricked into acting on: no raw HTML, no image, and a link holds only
//! an http(s) URL that [`super::link::safe_url`] passed.

/// Nesting is capped so no input can ask for a deep recursion, in blocks (a quote in a quote) and
/// in marks (emphasis in a link in emphasis).
pub const MAX_DEPTH: usize = 5;
/// The longest link label, in characters.
pub const MAX_LABEL: usize = 300;
/// The longest link URL, in characters.
pub const MAX_URL: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `level` is 3 or 4: a view is not a document, so no heading is larger than a section's.
    Heading { level: u8, text: Vec<Inline> },
    Paragraph(Vec<Inline>),
    List(List),
    Quote(Vec<Block>),
    /// A fence's lines, not highlighted.
    Code(String),
    Rule,
    Table(Table),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub ordered: bool,
    /// The number an ordered list begins at; 1 for any other.
    pub start: u32,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub text: Vec<Inline>,
    /// One nested level only: the items under this one are never nested further.
    pub sub: Option<List>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub head: Vec<Vec<Inline>>,
    /// Every row is as wide as the head.
    pub rows: Vec<Vec<Vec<Inline>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Em(Vec<Inline>),
    Del(Vec<Inline>),
    /// `url` is http(s) and normalised.
    Link { url: String, children: Vec<Inline> },
    /// A newline in a paragraph is a hard break.
    Break,
}
