//! What changed in an agent's view since the person last had it in front of
//! them, named by the paths the page draws the changed blocks at.
//!
//! A view shown again is diffed against the one it replaces. What changed
//! flashes once if the person is looking at that view as it does, and waits,
//! marked unseen, if they are not; which view is in front of them is
//! `agent_views`'s to say, so it is told, not asked.

use std::collections::{BTreeSet, HashMap};

use argus_protocol::{Block, PaneId};

type Key = (PaneId, String);

#[derive(Default)]
pub struct ViewChanges {
    /// The blocks, by path, that changed in a view since the person last
    /// had it in front of them. Kept until they open the view.
    unseen: HashMap<Key, BTreeSet<String>>,
    /// What changed in a view that was on screen as it did, or that has
    /// just been opened onto: sent so the page can flash it once. Kept
    /// until the next thing happens to a view, not until it is sent, so a
    /// page redrawn a moment later does not lose the flash before it ran.
    flashing: HashMap<Key, BTreeSet<String>>,
}

impl ViewChanges {
    /// A new connection replays every view, so nothing here describes one.
    pub fn reset(&mut self) {
        self.unseen.clear();
        self.flashing.clear();
    }

    /// The next thing that happens to a view ends every flash.
    pub fn end_flashes(&mut self) {
        self.flashing.clear();
    }

    /// A view was shown again as `after`, having been `before`. The paths
    /// that differ flash when `seen` and wait to be opened when not.
    pub fn record(&mut self, key: Key, before: Option<&[Block]>, after: &[Block], seen: bool) {
        let changed = before.map(|before| changed_blocks(before, after)).unwrap_or_default();
        if changed.is_empty() {
            return;
        }
        let marks = if seen { &mut self.flashing } else { &mut self.unseen };
        marks.entry(key).or_default().extend(changed);
    }

    /// The view closed: what was unseen in it is moot.
    pub fn forget(&mut self, key: &Key) {
        self.unseen.remove(key);
        self.flashing.clear();
    }

    /// The person has the view in front of them: what was unseen in it
    /// flashes once instead.
    pub fn reveal(&mut self, key: &Key) {
        if let Some(marks) = self.unseen.remove(key) {
            self.flashing.entry(key.clone()).or_default().extend(marks);
        }
    }

    /// The paths to mark in a view, and whether any of them wait unseen.
    pub fn marks(&self, key: &Key) -> (Vec<String>, bool) {
        let unseen = self.unseen.get(key);
        let marked = unseen.into_iter().chain(self.flashing.get(key)).flatten();
        let paths = marked.cloned().collect::<BTreeSet<_>>().into_iter().collect();
        (paths, unseen.is_some_and(|marks| !marks.is_empty()))
    }
}

/// The paths, as the page names a block under its view (`2`, `1.0.3`), of
/// the blocks in `after` that are not as they were in `before`.
///
/// Only the innermost block that differs is named, so a section with one
/// changed line in it marks the line and not everything beside it. A
/// block whose kind changed, or whose container has a different shape (a
/// tab or column more or fewer), is named whole; so is one that was added.
/// A path is built as the page builds it, from `Slot::page_step`.
pub fn changed_blocks(before: &[Block], after: &[Block]) -> Vec<String> {
    let mut found = Vec::new();
    diff_list(before, after, "", &mut found);
    found
}

fn diff_list(before: &[Block], after: &[Block], prefix: &str, found: &mut Vec<String>) {
    for (i, new) in after.iter().enumerate() {
        let path = if prefix.is_empty() { i.to_string() } else { format!("{prefix}.{i}") };
        match before.get(i) {
            Some(old) => diff_block(old, new, &path, found),
            None => found.push(path),
        }
    }
}

fn diff_block(old: &Block, new: &Block, path: &str, found: &mut Vec<String>) {
    if old == new {
        return;
    }
    let (inside_old, inside_new) = (old.children(), new.children());
    let same_shape = inside_old.len() == inside_new.len()
        && inside_old.iter().zip(&inside_new).all(|(a, b)| a.0 == b.0)
        && shell(old) == shell(new);
    if !same_shape {
        found.push(path.to_string());
        return;
    }
    for ((_, a), (slot, b)) in inside_old.iter().zip(&inside_new) {
        let at = match slot.page_step() {
            Some(step) => format!("{path}.{step}"),
            None => path.to_string(),
        };
        diff_list(a, b, &at, found);
    }
}

/// The block's own content, with the blocks inside it set aside: each is
/// replaced by a divider, and a list of dividers is then emptied, so a
/// container that gained or lost a block still has the shell it had.
/// `Block::children_mut` hands out slices, which cannot be shortened, so
/// the lengths are masked in the block's JSON rather than cleared in it.
fn shell(block: &Block) -> serde_json::Value {
    let mut bare = block.clone();
    for (_, list) in bare.children_mut() {
        list.fill(Block::Divider);
    }
    let mut json = serde_json::to_value(&bare).unwrap_or_default();
    mask(&mut json);
    json
}

fn mask(json: &mut serde_json::Value) {
    use serde_json::Value;
    match json {
        Value::Array(items) => {
            let divider = |v: &Value| v.get("kind").and_then(Value::as_str) == Some("divider");
            if !items.is_empty() && items.iter().all(divider) {
                items.clear();
            } else {
                items.iter_mut().for_each(mask);
            }
        }
        Value::Object(map) => map.values_mut().for_each(mask),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(text: &str) -> Block {
        Block::Markdown { text: text.into() }
    }

    fn section(blocks: Vec<Block>) -> Block {
        Block::Section { heading: "s".into(), blocks }
    }

    fn changed(before: Vec<Block>, after: Vec<Block>) -> Vec<String> {
        changed_blocks(&before, &after)
    }

    #[test]
    fn a_change_is_named_by_the_path_the_page_draws_it_at() {
        let grid = |b| Block::Grid { min: argus_protocol::GridMin::Md, items: vec![md("a"), b] };
        assert_eq!(changed(vec![grid(md("b"))], vec![grid(md("B"))]), ["0.1.0"]);
        let table = |b| Block::Table {
            columns: vec![argus_protocol::Column { title: "c".into(), numeric: false, optional: false }],
            rows: vec![vec![argus_protocol::Value::Empty], vec![argus_protocol::Value::Empty]],
            sorted_by: None,
            total: None,
            row_keys: Vec::new(),
            select: argus_protocol::RowSelect::None,
            actions: Vec::new(),
            details: vec![None, Some(vec![b])],
        };
        assert_eq!(changed(vec![table(md("x"))], vec![table(md("y"))]), ["0.d1.0"]);
    }

    #[test]
    fn only_the_blocks_whose_content_differs_are_named() {
        assert_eq!(changed(vec![md("a"), md("b")], vec![md("a"), md("B")]), ["1"]);
        assert!(changed(vec![md("a")], vec![md("a")]).is_empty());
    }

    #[test]
    fn a_nested_change_names_the_inner_block_and_not_its_containers() {
        let before = vec![md("x"), section(vec![md("a"), Block::Divider, md("c")])];
        let after = vec![md("x"), section(vec![md("a"), Block::Divider, md("C")])];
        assert_eq!(changed(before, after), ["1.2"]);
    }

    #[test]
    fn a_columns_and_a_tabs_paths_add_the_column_or_tab_first() {
        let columns = |b: &str| Block::Columns { columns: vec![vec![md("a")], vec![md("a"), md(b)]] };
        assert_eq!(changed(vec![columns("b")], vec![columns("B")]), ["0.1.1"]);
        let tabs = |b: &str| Block::Tabs {
            tabs: vec![
                argus_protocol::Tab { title: "one".into(), blocks: vec![md("a")] },
                argus_protocol::Tab { title: "two".into(), blocks: vec![md(b)] },
            ],
        };
        assert_eq!(changed(vec![md("x"), tabs("b")], vec![md("x"), tabs("B")]), ["1.1.0"]);
    }

    #[test]
    fn a_containers_own_change_names_the_container() {
        let with_heading = |h: &str| Block::Section { heading: h.into(), blocks: vec![md("a")] };
        assert_eq!(changed(vec![with_heading("old")], vec![with_heading("new")]), ["0"]);
    }

    #[test]
    fn a_block_added_or_of_another_kind_is_named_whole() {
        assert_eq!(changed(vec![md("a")], vec![md("a"), md("b")]), ["1"]);
        assert_eq!(changed(vec![md("a")], vec![Block::Divider]), ["0"]);
        assert_eq!(changed(vec![section(vec![md("a")])], vec![section(vec![md("a"), md("b")])]), ["0.1"]);
        let columns = |n: usize| Block::Columns { columns: vec![vec![md("a")]; n] };
        assert_eq!(changed(vec![columns(1)], vec![columns(2)]), ["0"]);
    }

    #[test]
    fn a_blocks_removed_are_not_named() {
        assert!(changed(vec![md("a"), md("b")], vec![md("a")]).is_empty());
    }

    #[test]
    fn a_containers_heading_changing_while_a_block_is_added_names_the_container() {
        let sec = |h: &str, n: usize| Block::Section { heading: h.into(), blocks: vec![md("a"); n] };
        assert_eq!(changed(vec![sec("old", 1)], vec![sec("new", 2)]), ["0"]);
    }
}
