//! The text of a Markdown document in the order a person reads it, one leaf for each paragraph,
//! heading, list item, code block and table cell: what a selection runs across and what it copies.
//!
//! A window draws each leaf as one wrapped run of text and numbers them in the order [`leaves`]
//! does, so a place in the document is a leaf and a byte of its text, and the text between two
//! places is read here and not by whichever window drew it.

use super::model::{Block, Inline, List, Table};
use super::runs::Styled;

/// A place in the document: a leaf, and a byte of its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub leaf: usize,
    pub at: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf {
    pub styled: Styled,
    /// What copying puts before this leaf's text when something precedes it: a tab between the cells
    /// of a row, a line end anywhere else.
    pub sep: &'static str,
}

/// The leaves of `blocks`, in reading order.
pub fn leaves(blocks: &[Block]) -> Vec<Leaf> {
    let mut out = Vec::new();
    blocks_into(blocks, &mut out);
    out
}

fn push(out: &mut Vec<Leaf>, inlines: &[Inline], sep: &'static str) {
    out.push(Leaf { styled: Styled::of(inlines), sep });
}

fn blocks_into(blocks: &[Block], out: &mut Vec<Leaf>) {
    for block in blocks {
        match block {
            Block::Heading { text, .. } => push(out, text, "\n"),
            Block::Paragraph(text) => push(out, text, "\n"),
            Block::List(list) => list_into(list, out),
            Block::Quote(inner) => blocks_into(inner, out),
            Block::Code(text) => push(out, &[Inline::Text(text.clone())], "\n"),
            Block::Rule => {}
            Block::Table(table) => table_into(table, out),
        }
    }
}

fn list_into(list: &List, out: &mut Vec<Leaf>) {
    for item in &list.items {
        push(out, &item.text, "\n");
        if let Some(sub) = &item.sub {
            list_into(sub, out);
        }
    }
}

fn table_into(table: &Table, out: &mut Vec<Leaf>) {
    for row in std::iter::once(&table.head).chain(&table.rows) {
        for (k, cell) in row.iter().enumerate() {
            push(out, cell, if k == 0 { "\n" } else { "\t" });
        }
    }
}

/// The text from `from` to `to`, `from` first, leaf after leaf; none when they are the same place.
pub fn text_between(leaves: &[Leaf], from: Pos, to: Pos) -> Option<String> {
    if from == to {
        return None;
    }
    let mut out = String::new();
    for leaf in from.leaf..=to.leaf {
        let Some(found) = leaves.get(leaf) else { break };
        let text = &found.styled.text;
        let start = if leaf == from.leaf { from.at.min(text.len()) } else { 0 };
        let end = if leaf == to.leaf { to.at.min(text.len()) } else { text.len() };
        if leaf != from.leaf {
            out.push_str(found.sep);
        }
        out.push_str(text.get(start..end).unwrap_or(""));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::markdown::blocks::parse;

    fn texts(doc: &str) -> Vec<String> {
        leaves(&parse(doc)).into_iter().map(|leaf| leaf.styled.text).collect()
    }

    #[test]
    fn leaves_come_in_reading_order_through_lists_quotes_and_tables() {
        let doc = "# Head\n\npara **one**\n\n- a\n  - sub\n- b\n\n> quoted\n\n```\ncode\n```\n\n| x | y |\n|---|---|\n| 1 | 2 |\n";
        assert_eq!(texts(doc), ["Head", "para one", "a", "sub", "b", "quoted", "code", "x", "y", "1", "2"]);
    }

    #[test]
    fn a_rule_is_no_leaf() {
        assert_eq!(texts("a\n\n---\n\nb"), ["a", "b"]);
    }

    #[test]
    fn text_between_joins_leaves_with_a_line_end_and_cells_with_a_tab() {
        let all = leaves(&parse("one\n\n| x | y |\n|---|---|\n| 1 | 2 |\n"));
        let end = Pos { leaf: all.len() - 1, at: 1 };
        assert_eq!(text_between(&all, Pos { leaf: 0, at: 1 }, end).as_deref(), Some("ne\nx\ty\n1\t2"));
    }

    #[test]
    fn text_between_inside_one_leaf_is_that_stretch_and_the_same_place_is_nothing() {
        let all = leaves(&parse("hello world"));
        assert_eq!(text_between(&all, Pos { leaf: 0, at: 6 }, Pos { leaf: 0, at: 11 }).as_deref(), Some("world"));
        assert_eq!(text_between(&all, Pos { leaf: 0, at: 3 }, Pos { leaf: 0, at: 3 }), None);
    }

    #[test]
    fn a_place_past_the_text_is_held_to_its_end() {
        let all = leaves(&parse("héllo"));
        assert_eq!(text_between(&all, Pos { leaf: 0, at: 0 }, Pos { leaf: 0, at: 99 }).as_deref(), Some("héllo"));
    }
}
