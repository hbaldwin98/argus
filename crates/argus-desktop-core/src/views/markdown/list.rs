//! A list: items by `-`, `*`, `+` or a number, one nested level deep.
//!
//! An item indented two or more past the list's base is a sub-item of the one before it, its own
//! marker deciding what kind of list the sub-items are; a change of marker kind at the top ends the
//! list; a line that starts no block continues the item before it; a blank line continues the list
//! only when an item at least at the base indent follows and is not a rule.

use super::inline::parse;
use super::line;
use super::model::{Inline, Item, List};

struct Raw {
    text: Vec<String>,
    sub: Vec<Raw>,
    /// The marker of the first sub-item, which says what kind of list they are.
    sub_marker: String,
}

/// The list that begins at line `from`, and the line after it.
pub fn read(lines: &[String], from: usize) -> (List, usize) {
    let first = line::item(&lines[from]).expect("a list begins at an item");
    let base = first.indent;
    let top_ordered = first.ordered();
    let first_marker = first.marker.to_owned();
    let mut items: Vec<Raw> = Vec::new();
    // The item the next continuation joins: the last top item, or its last sub-item.
    let mut last: Option<(usize, Option<usize>)> = None;
    let mut i = from;
    while i < lines.len() {
        let text = &lines[i];
        if line::is_blank(text) {
            let mut j = i;
            while j < lines.len() && line::is_blank(&lines[j]) {
                j += 1;
            }
            let next = lines.get(j).and_then(|l| line::item(l));
            match next {
                Some(next) if !line::is_rule(&lines[j]) && next.indent >= base => {
                    i = j;
                    continue;
                }
                _ => break,
            }
        }
        let found = if line::is_rule(text) { None } else { line::item(text) };
        if let Some(found) = found {
            if found.indent < base {
                break;
            }
            let raw = Raw { text: vec![found.text.to_owned()], sub: Vec::new(), sub_marker: found.marker.to_owned() };
            if found.indent - base >= 2 && !items.is_empty() {
                let top = items.len() - 1;
                let parent = &mut items[top];
                if parent.sub.is_empty() {
                    parent.sub_marker = found.marker.to_owned();
                }
                parent.sub.push(raw);
                last = Some((top, Some(parent.sub.len() - 1)));
            } else {
                if found.ordered() != top_ordered {
                    break;
                }
                items.push(raw);
                last = Some((items.len() - 1, None));
            }
            i += 1;
            continue;
        }
        let Some((top, sub)) = last else { break };
        if line::starts_block(text) {
            break;
        }
        let target = match sub {
            Some(s) => &mut items[top].sub[s],
            None => &mut items[top],
        };
        target.text.push(text.trim().to_owned());
        i += 1;
    }
    (build(&items, top_ordered, &first_marker), i)
}

fn build(items: &[Raw], ordered: bool, marker: &str) -> List {
    let start = if ordered { marker.trim_end_matches(['.', ')']).parse().unwrap_or(1) } else { 1 };
    let items = items
        .iter()
        .map(|raw| {
            let text: Vec<Inline> = parse(&raw.text.join("\n"));
            let sub = (!raw.sub.is_empty()).then(|| {
                let sub_ordered = raw.sub_marker.starts_with(|c: char| c.is_ascii_digit());
                build(&raw.sub, sub_ordered, &raw.sub_marker)
            });
            Item { text, sub }
        })
        .collect();
    List { ordered, start, items }
}
