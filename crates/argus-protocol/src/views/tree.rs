//! What each kind of block holds: the lists of blocks inside a container,
//! and the links, values and actions a block holds itself.
//!
//! The checks, the links list, the patch, the press and the page all walk a
//! view through [`Block::children`] and [`Block::leaves`]. Both name every
//! kind with no arm for the rest, so a kind added to the type does not
//! compile until it says here what it holds, and is then reached by every
//! one of them.

use crate::views::agent_view::{Block, FormField, RowItem, TreeNode, Value};
use crate::views::press::ViewAction;

/// Which list of blocks inside a container, named as the container holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// A section's, card's or details' own blocks.
    Blocks,
    /// One column of a columns block.
    Column(usize),
    /// One tab of a tabs block.
    Tab(usize),
    /// One item of a grid, as a list of one: the item is the block.
    Item(usize),
    /// The blocks one row of a table opens to.
    Details(usize),
}

impl Slot {
    /// What follows the container's path to reach this list, in the grammar
    /// a refusal and a patch name blocks by: `.blocks`, `.columns[1]`,
    /// `.tabs[0].blocks`, `.items[2]`, `.details[3]`. A grid item is the
    /// block itself, so its step is already complete.
    pub fn patch_step(&self) -> String {
        match self {
            Slot::Blocks => ".blocks".to_string(),
            Slot::Column(c) => format!(".columns[{c}]"),
            Slot::Tab(t) => format!(".tabs[{t}].blocks"),
            Slot::Item(i) => format!(".items[{i}]"),
            Slot::Details(r) => format!(".details[{r}]"),
        }
    }

    /// The step to the `index`th block of this list: the list's step and
    /// `[index]`, except for a grid item, whose one block is named by the
    /// item alone. What [`crate::AgentView::patched`] reads back.
    pub fn patch_path(&self, index: usize) -> String {
        match self {
            Slot::Item(_) => self.patch_step(),
            _ => format!("{}[{index}]", self.patch_step()),
        }
    }

    /// The segment the page adds to a block's path before the index of a
    /// block in this list: none for a section's, card's or details' own
    /// blocks, the column's or tab's index, a grid item's index (its block
    /// is then `0`), and `d` and the row for a table's details.
    pub fn page_step(&self) -> Option<String> {
        match self {
            Slot::Blocks => None,
            Slot::Column(i) | Slot::Tab(i) | Slot::Item(i) => Some(i.to_string()),
            Slot::Details(r) => Some(format!("d{r}")),
        }
    }
}

/// Something a block holds itself that is not a block: what the check, the
/// links list and a press each look for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Leaf<'a> {
    /// A saved view, by its id, that an element opens or an embed draws.
    SavedView(&'a str),
    Value(&'a Value),
    Action(&'a ViewAction, ActionPlace<'a>),
}

/// Where an action sits, which decides what a press of it may carry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionPlace<'a> {
    /// A header, card or row button.
    Button,
    /// On a table's selected rows, keyed as the table keys them.
    Table { row_keys: &'a [String] },
    /// A form's submit.
    Form { id: &'a str, fields: &'a [FormField] },
}

/// Where a leaf sits in its block: the step a refusal adds to the block's
/// own path. A patch addresses blocks only, so this grammar is the
/// refusal's, extending the block path a patch reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeafAt {
    /// A field of the block itself: `.opens`, `.view`, `.submit`.
    Own(&'static str),
    /// An element of a list the block holds, or a field of it:
    /// `.actions[1]`, `.items[2].opens`, `.fields[0].value`.
    Element { list: &'static str, index: usize, field: Option<&'static str> },
    /// `.rows[r][c]`
    Cell { row: usize, column: usize },
    /// `.columns[c].cards[k].<field>`
    Card { column: usize, card: usize, field: &'static str },
    /// `.nodes[i].children[j]….<field>`, by the index at each level from the top.
    Node { indices: Vec<usize>, field: &'static str },
}

impl LeafAt {
    pub fn patch_step(&self) -> String {
        match self {
            LeafAt::Own(field) => format!(".{field}"),
            LeafAt::Element { list, index, field: None } => format!(".{list}[{index}]"),
            LeafAt::Element { list, index, field: Some(f) } => format!(".{list}[{index}].{f}"),
            LeafAt::Cell { row, column } => format!(".rows[{row}][{column}]"),
            LeafAt::Card { column, card, field } => format!(".columns[{column}].cards[{card}].{field}"),
            LeafAt::Node { indices, field } => {
                let mut step = String::from(".nodes");
                for (depth, i) in indices.iter().enumerate() {
                    if depth > 0 {
                        step.push_str(".children");
                    }
                    step.push_str(&format!("[{i}]"));
                }
                format!("{step}.{field}")
            }
        }
    }
}

type Leaves<'a> = Vec<(LeafAt, Leaf<'a>)>;

fn element(list: &'static str, index: usize, field: &'static str) -> LeafAt {
    LeafAt::Element { list, index, field: Some(field) }
}

fn opened<'a>(out: &mut Leaves<'a>, at: LeafAt, opens: &'a Option<String>) {
    if let Some(id) = opens {
        out.push((at, Leaf::SavedView(id)));
    }
}

fn buttons<'a>(out: &mut Leaves<'a>, actions: &'a [ViewAction], place: ActionPlace<'a>) {
    for (i, action) in actions.iter().enumerate() {
        out.push((LeafAt::Element { list: "actions", index: i, field: None }, Leaf::Action(action, place)));
    }
}

/// A tree's leaves node by node from the top, each node's before its children's.
fn tree<'a>(nodes: &'a [TreeNode], indices: &mut Vec<usize>, out: &mut Leaves<'a>) {
    for (i, node) in nodes.iter().enumerate() {
        indices.push(i);
        let at = |field| LeafAt::Node { indices: indices.clone(), field };
        opened(out, at("opens"), &node.opens);
        if let Some(value) = &node.value {
            out.push((at("value"), Leaf::Value(value)));
        }
        tree(&node.children, indices, out);
        indices.pop();
    }
}

/// The kinds that hold no blocks, as one pattern for the three walks over
/// containers: a new kind goes here once, or gets an arm in each of them.
macro_rules! holds_no_blocks {
    () => {
        Block::Markdown { .. }
            | Block::Fields { .. }
            | Block::List { .. }
            | Block::Stats { .. }
            | Block::Code { .. }
            | Block::Diagram { .. }
            | Block::Callout { .. }
            | Block::Row { .. }
            | Block::Divider
            | Block::Chart { .. }
            | Block::Progress { .. }
            | Block::Timeline { .. }
            | Block::Tree { .. }
            | Block::Graph { .. }
            | Block::Diff { .. }
            | Block::Matrix { .. }
            | Block::Checklist { .. }
            | Block::Log { .. }
            | Block::Board { .. }
            | Block::Form { .. }
            | Block::Embed { .. }
            | Block::Unknown
    };
}

impl Block {
    /// What this block holds itself, not the blocks inside it, in the order
    /// a reader meets it, each with where it sits.
    pub fn leaves(&self) -> Vec<(LeafAt, Leaf<'_>)> {
        let mut out = Vec::new();
        match self {
            Block::Embed { view } => out.push((LeafAt::Own("view"), Leaf::SavedView(view))),
            Block::Section { opens, .. } | Block::Details { opens, .. } => opened(&mut out, LeafAt::Own("opens"), opens),
            Block::Card { opens, actions, .. } => {
                opened(&mut out, LeafAt::Own("opens"), opens);
                buttons(&mut out, actions, ActionPlace::Button);
            }
            Block::Fields { fields } => {
                for (i, field) in fields.iter().enumerate() {
                    out.push((element("fields", i, "value"), Leaf::Value(&field.value)));
                }
            }
            Block::Table { rows, row_keys, actions, .. } => {
                for (r, row) in rows.iter().enumerate() {
                    for (c, cell) in row.iter().enumerate() {
                        out.push((LeafAt::Cell { row: r, column: c }, Leaf::Value(cell)));
                    }
                }
                buttons(&mut out, actions, ActionPlace::Table { row_keys });
            }
            Block::List { items } => {
                for (i, item) in items.iter().enumerate() {
                    opened(&mut out, element("items", i, "opens"), &item.opens);
                }
            }
            Block::Checklist { items } => {
                for (i, item) in items.iter().enumerate() {
                    opened(&mut out, element("items", i, "opens"), &item.opens);
                }
            }
            Block::Timeline { events } => {
                for (i, event) in events.iter().enumerate() {
                    opened(&mut out, element("events", i, "opens"), &event.opens);
                }
            }
            Block::Stats { stats } => {
                for (i, stat) in stats.iter().enumerate() {
                    opened(&mut out, element("stats", i, "opens"), &stat.opens);
                }
            }
            Block::Graph { nodes, .. } => {
                for (n, node) in nodes.iter().enumerate() {
                    opened(&mut out, element("nodes", n, "opens"), &node.opens);
                }
            }
            Block::Tree { nodes } => tree(nodes, &mut Vec::new(), &mut out),
            Block::Board { columns } => {
                for (c, column) in columns.iter().enumerate() {
                    for (k, card) in column.cards.iter().enumerate() {
                        let at = |field| LeafAt::Card { column: c, card: k, field };
                        opened(&mut out, at("opens"), &card.opens);
                        if let Some(value) = &card.value {
                            out.push((at("value"), Leaf::Value(value)));
                        }
                    }
                }
            }
            Block::Row { items, .. } => {
                for (i, item) in items.iter().enumerate() {
                    out.push(match item {
                        RowItem::Value { value } => (element("items", i, "value"), Leaf::Value(value)),
                        RowItem::Action { action } => (element("items", i, "action"), Leaf::Action(action, ActionPlace::Button)),
                    });
                }
            }
            Block::Form { id, fields, submit } => {
                out.push((LeafAt::Own("submit"), Leaf::Action(submit, ActionPlace::Form { id, fields })));
            }
            Block::Columns { .. }
            | Block::Tabs { .. }
            | Block::Grid { .. }
            | Block::Markdown { .. }
            | Block::Code { .. }
            | Block::Diagram { .. }
            | Block::Callout { .. }
            | Block::Divider
            | Block::Chart { .. }
            | Block::Progress { .. }
            | Block::Matrix { .. }
            | Block::Log { .. }
            | Block::Diff { .. }
            | Block::Unknown => {}
        }
        out
    }

    /// The lists of blocks directly inside this one, each with the slot it
    /// is held in; none for a block that holds none. A table's rows with
    /// nothing to open to are left out.
    pub fn children(&self) -> Vec<(Slot, &[Block])> {
        match self {
            Block::Section { blocks, .. } | Block::Card { blocks, .. } | Block::Details { blocks, .. } => {
                vec![(Slot::Blocks, blocks.as_slice())]
            }
            Block::Columns { columns } => {
                columns.iter().enumerate().map(|(c, list)| (Slot::Column(c), list.as_slice())).collect()
            }
            Block::Tabs { tabs } => tabs.iter().enumerate().map(|(t, tab)| (Slot::Tab(t), tab.blocks.as_slice())).collect(),
            Block::Grid { items, .. } => items.chunks(1).enumerate().map(|(i, item)| (Slot::Item(i), item)).collect(),
            Block::Table { details, .. } => details
                .iter()
                .enumerate()
                .filter_map(|(r, inner)| inner.as_ref().map(|inner| (Slot::Details(r), inner.as_slice())))
                .collect(),
            holds_no_blocks!() => Vec::new(),
        }
    }

    /// [`Block::children`], to change the blocks in.
    pub fn children_mut(&mut self) -> Vec<(Slot, &mut [Block])> {
        match self {
            Block::Section { blocks, .. } | Block::Card { blocks, .. } | Block::Details { blocks, .. } => {
                vec![(Slot::Blocks, blocks.as_mut_slice())]
            }
            Block::Columns { columns } => {
                columns.iter_mut().enumerate().map(|(c, list)| (Slot::Column(c), list.as_mut_slice())).collect()
            }
            Block::Tabs { tabs } => {
                tabs.iter_mut().enumerate().map(|(t, tab)| (Slot::Tab(t), tab.blocks.as_mut_slice())).collect()
            }
            Block::Grid { items, .. } => items.chunks_mut(1).enumerate().map(|(i, item)| (Slot::Item(i), item)).collect(),
            Block::Table { details, .. } => details
                .iter_mut()
                .enumerate()
                .filter_map(|(r, inner)| inner.as_mut().map(|inner| (Slot::Details(r), inner.as_mut_slice())))
                .collect(),
            holds_no_blocks!() => Vec::new(),
        }
    }

    /// The block lists directly inside this one, for walking a view.
    pub fn nested(&self) -> Vec<&[Block]> {
        self.children().into_iter().map(|(_, blocks)| blocks).collect()
    }

    /// The block with every list of blocks inside it emptied, its own
    /// content kept: a section's heading, a column per column, a tab's
    /// title, a table's rows. What a container is, apart from what it holds.
    pub fn without_children(&self) -> Block {
        let mut bare = self.clone();
        match &mut bare {
            Block::Section { blocks, .. } | Block::Card { blocks, .. } | Block::Details { blocks, .. } => blocks.clear(),
            Block::Columns { columns } => columns.iter_mut().for_each(Vec::clear),
            Block::Tabs { tabs } => tabs.iter_mut().for_each(|tab| tab.blocks.clear()),
            Block::Grid { items, .. } => items.clear(),
            Block::Table { details, .. } => details.iter_mut().flatten().for_each(Vec::clear),
            holds_no_blocks!() => {}
        }
        debug_assert!(bare.children().iter().all(|(_, list)| list.is_empty()), "a container kind left out");
        bare
    }
}

/// Every leaf in `blocks` and the blocks inside them, in reading order: a
/// block's own before those of the blocks it holds.
pub fn leaves_within(blocks: &[Block]) -> Vec<Leaf<'_>> {
    fn walk<'a>(blocks: &'a [Block], out: &mut Vec<Leaf<'a>>) {
        for block in blocks {
            out.extend(block.leaves().into_iter().map(|(_, leaf)| leaf));
            for (_, inner) in block.children() {
                walk(inner, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(blocks, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_without_children_keeps_its_own_content() {
        let tabs = Block::Tabs {
            tabs: vec![Tab { title: "Log".into(), blocks: vec![Block::Divider] }],
        };
        let Block::Tabs { tabs: bare } = tabs.without_children() else { panic!() };
        assert_eq!((bare[0].title.as_str(), bare[0].blocks.len()), ("Log", 0));
        let grid = Block::Grid { min: Default::default(), items: vec![Block::Divider, Block::Divider] };
        assert!(grid.without_children().children().is_empty());
        assert_eq!(Block::Divider.without_children(), Block::Divider);
    }
    use crate::views::agent_view::Tab;

    fn md(text: &str) -> Block {
        Block::Markdown { text: text.into() }
    }

    fn table(details: Vec<Option<Vec<Block>>>) -> Block {
        Block::Table {
            columns: Vec::new(),
            rows: Vec::new(),
            sorted_by: None,
            total: None,
            row_keys: Vec::new(),
            select: Default::default(),
            actions: Vec::new(),
            details,
        }
    }

    #[test]
    fn every_container_names_the_lists_inside_it() {
        let section = Block::Section { heading: "s".into(), blocks: vec![md("a")], opens: None };
        assert_eq!(section.children().len(), 1);
        assert_eq!(section.children()[0].0, Slot::Blocks);

        let columns = Block::Columns { columns: vec![vec![md("a")], vec![md("b"), md("c")]] };
        let slots: Vec<_> = columns.children().iter().map(|(s, b)| (*s, b.len())).collect();
        assert_eq!(slots, [(Slot::Column(0), 1), (Slot::Column(1), 2)]);

        let tabs = Block::Tabs { tabs: vec![Tab { title: "t".into(), blocks: vec![md("a")] }] };
        assert_eq!(tabs.children()[0].0, Slot::Tab(0));

        let grid = Block::Grid { min: Default::default(), items: vec![md("a"), md("b")] };
        let slots: Vec<_> = grid.children().iter().map(|(s, b)| (*s, b.len())).collect();
        assert_eq!(slots, [(Slot::Item(0), 1), (Slot::Item(1), 1)]);

        let rows = table(vec![None, Some(vec![md("a")]), Some(Vec::new())]);
        let slots: Vec<_> = rows.children().iter().map(|(s, b)| (*s, b.len())).collect();
        assert_eq!(slots, [(Slot::Details(1), 1), (Slot::Details(2), 0)]);

        assert!(md("a").children().is_empty());
        assert!(Block::Unknown.children().is_empty());
    }

    #[test]
    fn the_mutable_walk_reaches_the_same_lists() {
        let mut block = Block::Grid { min: Default::default(), items: vec![md("a"), md("b")] };
        let slots: Vec<_> = block.children_mut().into_iter().map(|(s, b)| (s, b.len())).collect();
        assert_eq!(slots, [(Slot::Item(0), 1), (Slot::Item(1), 1)]);
        let mut rows = table(vec![None, Some(vec![md("a")])]);
        assert_eq!(rows.children_mut()[0].0, Slot::Details(1));
        let mut tabs = Block::Tabs { tabs: vec![Tab { title: "t".into(), blocks: vec![md("a")] }] };
        tabs.children_mut()[0].1[0] = md("b");
        assert_eq!(tabs.children()[0].1[0], md("b"));
    }

    #[test]
    fn a_slot_prints_the_grammar_a_patch_reads() {
        assert_eq!(Slot::Blocks.patch_step(), ".blocks");
        assert_eq!(Slot::Column(1).patch_step(), ".columns[1]");
        assert_eq!(Slot::Tab(2).patch_step(), ".tabs[2].blocks");
        assert_eq!(Slot::Item(3).patch_step(), ".items[3]");
        assert_eq!(Slot::Details(4).patch_step(), ".details[4]");
        assert_eq!(Slot::Blocks.patch_path(5), ".blocks[5]");
        assert_eq!(Slot::Column(1).patch_path(5), ".columns[1][5]");
        assert_eq!(Slot::Tab(2).patch_path(5), ".tabs[2].blocks[5]");
        assert_eq!(Slot::Item(3).patch_path(0), ".items[3]");
        assert_eq!(Slot::Details(4).patch_path(5), ".details[4][5]");
    }

    fn block(json: serde_json::Value) -> Block {
        serde_json::from_value(json).unwrap()
    }

    fn kinds(leaves: &[Leaf]) -> Vec<&'static str> {
        leaves
            .iter()
            .map(|leaf| match leaf {
                Leaf::SavedView(_) => "view",
                Leaf::Value(_) => "value",
                Leaf::Action(..) => "action",
            })
            .collect()
    }

    #[test]
    fn a_leaf_prints_the_step_a_refusal_adds() {
        assert_eq!(LeafAt::Own("opens").patch_step(), ".opens");
        assert_eq!(LeafAt::Element { list: "actions", index: 1, field: None }.patch_step(), ".actions[1]");
        assert_eq!(LeafAt::Element { list: "items", index: 2, field: Some("opens") }.patch_step(), ".items[2].opens");
        assert_eq!(LeafAt::Cell { row: 3, column: 1 }.patch_step(), ".rows[3][1]");
        assert_eq!(LeafAt::Card { column: 0, card: 4, field: "value" }.patch_step(), ".columns[0].cards[4].value");
        assert_eq!(LeafAt::Node { indices: vec![5], field: "opens" }.patch_step(), ".nodes[5].opens");
        let deep = LeafAt::Node { indices: vec![0, 2, 1], field: "value" };
        assert_eq!(deep.patch_step(), ".nodes[0].children[2].children[1].value");
    }

    #[test]
    fn each_kind_names_the_leaves_it_holds() {
        let view = serde_json::json!({ "kind": "view", "id": "v" });
        let act = serde_json::json!({ "id": "a", "label": "A" });
        let cases = [
            (serde_json::json!({ "kind": "embed", "view": "v" }), vec!["view"]),
            (serde_json::json!({ "kind": "section", "heading": "s", "blocks": [], "opens": "v" }), vec!["view"]),
            (serde_json::json!({ "kind": "details", "summary": "s", "blocks": [], "opens": "v" }), vec!["view"]),
            (serde_json::json!({ "kind": "card", "blocks": [], "opens": "v", "actions": [act, act] }), vec!["view", "action", "action"]),
            (serde_json::json!({ "kind": "fields", "fields": [{ "label": "f", "value": view }] }), vec!["value"]),
            (
                serde_json::json!({ "kind": "table", "columns": [{ "title": "a" }, { "title": "b" }],
                    "rows": [[view, view]], "row_keys": ["k"], "actions": [act] }),
                vec!["value", "value", "action"],
            ),
            (serde_json::json!({ "kind": "list", "items": [{ "opens": "v" }, {}] }), vec!["view"]),
            (serde_json::json!({ "kind": "checklist", "items": [{ "label": "c", "done": true, "opens": "v" }] }), vec!["view"]),
            (serde_json::json!({ "kind": "timeline", "events": [{ "title": "e", "opens": "v" }] }), vec!["view"]),
            (serde_json::json!({ "kind": "stats", "stats": [{ "label": "s", "value": "1", "opens": "v" }] }), vec!["view"]),
            (serde_json::json!({ "kind": "graph", "nodes": [{ "id": "n", "label": "n", "opens": "v" }], "edges": [] }), vec!["view"]),
            (serde_json::json!({ "kind": "tree", "nodes": [{ "label": "n", "opens": "v", "value": view }] }), vec!["view", "value"]),
            (
                serde_json::json!({ "kind": "board", "columns": [{ "title": "c", "cards": [{ "title": "k", "opens": "v", "value": view }] }] }),
                vec!["view", "value"],
            ),
            (
                serde_json::json!({ "kind": "row", "items": [{ "kind": "value", "value": view }, { "kind": "action", "action": act }] }),
                vec!["value", "action"],
            ),
            (
                serde_json::json!({ "kind": "form", "id": "f", "fields": [{ "name": "n", "label": "n", "input": { "kind": "toggle" } }], "submit": act }),
                vec!["action"],
            ),
        ];
        for (json, expected) in cases {
            let block = block(json);
            let leaves: Vec<Leaf> = block.leaves().into_iter().map(|(_, leaf)| leaf).collect();
            assert_eq!(kinds(&leaves), expected, "{block:?}");
        }
        let none = [
            serde_json::json!({ "kind": "columns", "columns": [[{ "kind": "embed", "view": "v" }]] }),
            serde_json::json!({ "kind": "tabs", "tabs": [{ "title": "t", "blocks": [] }] }),
            serde_json::json!({ "kind": "grid", "items": [] }),
            serde_json::json!({ "kind": "markdown", "text": "x" }),
            serde_json::json!({ "kind": "code", "text": "x" }),
            serde_json::json!({ "kind": "diagram", "source": "x" }),
            serde_json::json!({ "kind": "callout", "tone": "neutral", "text": "x" }),
            serde_json::json!({ "kind": "divider" }),
            serde_json::json!({ "kind": "chart", "x": [], "series": [] }),
            serde_json::json!({ "kind": "progress", "label": "p", "value": 1.0, "max": 2.0 }),
            serde_json::json!({ "kind": "matrix", "rows": [], "columns": [], "cells": [] }),
            serde_json::json!({ "kind": "log", "lines": [] }),
            serde_json::json!({ "kind": "diff", "patch": "" }),
            serde_json::json!({ "kind": "sparkle" }),
        ];
        for json in none {
            let block = block(json);
            assert!(block.leaves().is_empty(), "{block:?}");
        }
    }

    #[test]
    fn a_trees_leaves_come_node_by_node_from_the_top() {
        let tree = block(serde_json::json!({ "kind": "tree", "nodes": [
            { "label": "a", "opens": "a", "value": { "kind": "view", "id": "a-value" }, "children": [
                { "label": "b", "opens": "b" } ] },
            { "label": "c", "opens": "c" }
        ] }));
        let steps: Vec<String> = tree.leaves().iter().map(|(at, _)| at.patch_step()).collect();
        assert_eq!(steps, [".nodes[0].opens", ".nodes[0].value", ".nodes[0].children[0].opens", ".nodes[1].opens"]);
    }

    #[test]
    fn leaves_within_reads_a_blocks_own_before_those_inside_it() {
        let card = block(serde_json::json!({ "kind": "card", "opens": "card",
            "actions": [{ "id": "a", "label": "A" }],
            "blocks": [{ "kind": "section", "heading": "s", "blocks": [], "opens": "section" }] }));
        let card = [card];
        let leaves = leaves_within(&card);
        assert_eq!(leaves.len(), 3);
        assert_eq!(leaves[0], Leaf::SavedView("card"));
        assert!(matches!(leaves[1], Leaf::Action(action, ActionPlace::Button) if action.id == "a"));
        assert_eq!(leaves[2], Leaf::SavedView("section"));
    }

    #[test]
    fn a_slot_prints_the_segment_the_page_adds() {
        assert_eq!(Slot::Blocks.page_step(), None);
        assert_eq!(Slot::Column(1).page_step().as_deref(), Some("1"));
        assert_eq!(Slot::Tab(2).page_step().as_deref(), Some("2"));
        assert_eq!(Slot::Item(3).page_step().as_deref(), Some("3"));
        assert_eq!(Slot::Details(4).page_step().as_deref(), Some("d4"));
    }
}
