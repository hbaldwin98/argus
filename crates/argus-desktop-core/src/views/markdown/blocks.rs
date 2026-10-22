//! Markdown read into blocks: the one entry the windows call, [`parse`].
//!
//! Lines are read in priority order: a blank line separates; a fence runs to a closing fence of the
//! same character and at least the same length, or to the end; a heading; a rule; a quote, to
//! `MAX_DEPTH` and then plain text; a list; a pipe table; else a paragraph, which runs to a blank
//! line or a line that starts another block and keeps its newlines as breaks. Anything the grammar
//! does not name is a paragraph, so any text at all reads as something.

use super::inline;
use super::line;
use super::list;
use super::model::{Block, MAX_DEPTH};
use super::table;

/// `text` as blocks. Line ends are made `\n` and a tab four spaces first.
pub fn parse(text: &str) -> Vec<Block> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "    ");
    let lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    parse_blocks(&lines, 0)
}

fn parse_blocks(lines: &[String], depth: usize) -> Vec<Block> {
    let mut out = Vec::new();
    let n = lines.len();
    let mut i = 0;
    while i < n {
        let current = &lines[i];
        if line::is_blank(current) {
            i += 1;
            continue;
        }

        if let Some((mark, len)) = line::fence(current) {
            let mut body = Vec::new();
            i += 1;
            while i < n && !line::closes_fence(&lines[i], mark, len) {
                body.push(lines[i].as_str());
                i += 1;
            }
            i += 1;
            out.push(Block::Code(body.join("\n")));
            continue;
        }

        if let Some((hashes, text)) = line::heading(current) {
            // A view is not a document: no heading is larger than a section's.
            out.push(Block::Heading { level: if hashes <= 3 { 3 } else { 4 }, text: inline::parse(text) });
            i += 1;
            continue;
        }

        if line::is_rule(current) {
            out.push(Block::Rule);
            i += 1;
            continue;
        }

        if depth < MAX_DEPTH && line::quote(current).is_some() {
            let mut inner = Vec::new();
            while i < n {
                let Some(rest) = line::quote(&lines[i]) else { break };
                inner.push(rest.to_owned());
                i += 1;
            }
            out.push(Block::Quote(parse_blocks(&inner, depth + 1)));
            continue;
        }

        if line::item(current).is_some() {
            let (list, next) = list::read(lines, i);
            out.push(Block::List(list));
            i = next;
            continue;
        }

        if table::begins(lines, i) {
            let (table, next) = table::read(lines, i);
            out.push(Block::Table(table));
            i = next;
            continue;
        }

        let mut paragraph = vec![current.as_str()];
        i += 1;
        while i < n && !line::is_blank(&lines[i]) && !line::starts_block(&lines[i]) {
            paragraph.push(lines[i].as_str());
            i += 1;
        }
        out.push(Block::Paragraph(inline::parse(&paragraph.join("\n"))));
    }
    out
}

#[cfg(test)]
mod tests;
