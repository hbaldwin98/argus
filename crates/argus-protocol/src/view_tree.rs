//! How the blocks of a view nest: the lists of blocks inside each
//! container, and what a patch path and the desktop page each call them.
//!
//! The checks, the patch, the press and the page all walk a view; they walk
//! it through [`Block::children`], so a container added to the type is
//! added here once and is reached by every one of them.

use crate::agent_view::Block;

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

impl Block {
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
            _ => Vec::new(),
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
            _ => Vec::new(),
        }
    }

    /// The block lists directly inside this one, for walking a view.
    pub fn nested(&self) -> Vec<&[Block]> {
        self.children().into_iter().map(|(_, blocks)| blocks).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_view::Tab;

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
        let section = Block::Section { heading: "s".into(), blocks: vec![md("a")] };
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

    #[test]
    fn a_slot_prints_the_segment_the_page_adds() {
        assert_eq!(Slot::Blocks.page_step(), None);
        assert_eq!(Slot::Column(1).page_step().as_deref(), Some("1"));
        assert_eq!(Slot::Tab(2).page_step().as_deref(), Some("2"));
        assert_eq!(Slot::Item(3).page_step().as_deref(), Some("3"));
        assert_eq!(Slot::Details(4).page_step().as_deref(), Some("d4"));
    }
}
