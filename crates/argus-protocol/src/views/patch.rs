//! How a patch replaces one block of a view the daemon holds: reading the
//! path an agent names the block by, in the grammar a refusal names blocks
//! by, and the view with that block swapped.

use crate::views::agent_view::{AgentView, Block};
use crate::views::tree::Slot;

impl AgentView {
    /// The view with the block at `path` replaced by `block`, or what is
    /// wrong with the path. The result is checked again by the caller, as a
    /// shown view is.
    pub fn patched(mut self, path: &str, block: Block) -> Result<AgentView, String> {
        let (top, steps) = parse(path)?;
        let mut list: &mut [Block] = &mut self.blocks;
        let mut at = top;
        let mut walked = format!("blocks[{top}]");
        for (slot, next) in &steps {
            let len = list.len();
            let Some(here) = list.get_mut(at) else {
                return Err(format!("{path}: {walked} is out of range, there are {len} blocks"));
            };
            list = descend(here, slot, *next, &walked, path)?;
            at = *next;
            walked.push_str(&slot.patch_path(*next));
        }
        let len = list.len();
        match list.get_mut(at) {
            Some(held) => *held = block,
            None => return Err(format!("{path}: {walked} is out of range, there are {len} blocks")),
        }
        Ok(self)
    }
}

/// The list inside `block` that `slot` names. `next` is the index the path
/// goes on to in it and `walked` the path to `block`, for the refusal.
fn descend<'a>(
    block: &'a mut Block,
    slot: &Slot,
    next: usize,
    walked: &str,
    path: &str,
) -> Result<&'a mut [Block], String> {
    if let Some(at) = block.children().iter().position(|(held, _)| held == slot) {
        return Ok(block.children_mut().swap_remove(at).1);
    }
    Err(match (slot, &*block) {
        (Slot::Column(c), Block::Columns { columns }) => {
            format!("{path}: {walked}.columns[{c}] is out of range, there are {} columns", columns.len())
        }
        (Slot::Tab(t), Block::Tabs { tabs }) => {
            format!("{path}: {walked}.tabs[{t}] is out of range, there are {} tabs", tabs.len())
        }
        (Slot::Item(i), Block::Grid { items, .. }) => {
            format!("{path}: {walked}.items[{i}] is out of range, there are {} items", items.len())
        }
        (Slot::Details(r), Block::Table { details, .. }) => match details.get(*r) {
            Some(None) => format!("{path}: {walked}.details[{r}] is empty, the row opens to nothing"),
            _ => format!(
                "{path}: {walked}.details[{r}] is out of range, there are {} rows with details",
                details.len()
            ),
        },
        (_, block) => format!("{path}: {walked} is {}, which has no {}", kind(block), slot.patch_path(next)),
    })
}

fn kind(block: &Block) -> &'static str {
    match block {
        Block::Section { .. } => "a section",
        Block::Columns { .. } => "columns",
        Block::Tabs { .. } => "tabs",
        Block::Table { .. } => "a table",
        Block::Card { .. } => "a card",
        Block::Grid { .. } => "a grid",
        Block::Details { .. } => "details",
        _ => "a block",
    }
}

/// The index and what follows it, from text that starts `[3]`. Digits and
/// nothing else, so `[+3]` and `[ 3]` are not another way to write it.
fn index<'p>(rest: &'p str, path: &str) -> Result<(usize, &'p str), String> {
    let inner = rest
        .strip_prefix('[')
        .ok_or_else(|| format!("{path}: expected `[` at {rest:?}"))?;
    let (digits, after) = inner
        .split_once(']')
        .ok_or_else(|| format!("{path}: a `[` is never closed"))?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("{path}: {digits:?} is not an index"));
    }
    let n = digits
        .parse::<usize>()
        .map_err(|_| format!("{path}: {digits:?} is not an index"))?;
    Ok((n, after))
}

/// The top-level index and the steps below it, each a slot and the index in
/// its list that the path goes on to (always 0 for a grid item, which is
/// the block itself).
fn parse(path: &str) -> Result<(usize, Vec<(Slot, usize)>), String> {
    let rest = path
        .strip_prefix("blocks")
        .ok_or_else(|| format!("{path}: a path starts with `blocks[i]`"))?;
    let (top, mut rest) = index(rest, path)?;
    let mut steps = Vec::new();
    while !rest.is_empty() {
        let after = rest
            .strip_prefix('.')
            .ok_or_else(|| format!("{path}: expected `.` at {rest:?}"))?;
        let name_end = after.find('[').unwrap_or(after.len());
        let (name, after) = after.split_at(name_end);
        let (step, after) = match name {
            "blocks" => {
                let (i, after) = index(after, path)?;
                ((Slot::Blocks, i), after)
            }
            "columns" => {
                let (c, after) = index(after, path)?;
                let (i, after) = index(after, path)?;
                ((Slot::Column(c), i), after)
            }
            "tabs" => {
                let (t, after) = index(after, path)?;
                let after = after
                    .strip_prefix(".blocks")
                    .ok_or_else(|| format!("{path}: a tab's blocks are `.tabs[t].blocks[i]`"))?;
                let (i, after) = index(after, path)?;
                ((Slot::Tab(t), i), after)
            }
            "items" => {
                let (i, after) = index(after, path)?;
                ((Slot::Item(i), 0), after)
            }
            "details" => {
                let (r, after) = index(after, path)?;
                let (i, after) = index(after, path)?;
                ((Slot::Details(r), i), after)
            }
            other => {
                return Err(format!(
                    "{path}: {other:?} does not lead to a block; use blocks, columns, tabs, items or details"
                ))
            }
        };
        steps.push(step);
        rest = after;
    }
    Ok((top, steps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::agent_view::Tab;

    fn md(text: &str) -> Block {
        Block::Markdown { text: text.into() }
    }

    fn view(blocks: Vec<Block>) -> AgentView {
        AgentView { id: "v".into(), title: "t".into(), blocks, ..Default::default() }
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

    /// One of everything a path can go through.
    fn nested() -> AgentView {
        view(vec![
            md("top0"),
            Block::Section {
                heading: "s".into(),
                blocks: vec![
                    md("s0"),
                    Block::Card {
                        title: None,
                        chip: None,
                        tone: Default::default(),
                        blocks: vec![md("c0")],
                        actions: Vec::new(),
                        opens: None,
                    },
                ],
                opens: None,
            },
            Block::Columns { columns: vec![vec![md("a0")], vec![md("b0"), md("b1")]] },
            Block::Tabs { tabs: vec![Tab { title: "one".into(), blocks: vec![md("t0")] }] },
            Block::Grid { min: Default::default(), items: vec![md("g0"), md("g1")] },
            table(vec![None, Some(vec![md("d0")])]),
            Block::Details { summary: "d".into(), chip: None, open: false, blocks: vec![md("e0")], opens: None },
        ])
    }

    fn patched(path: &str) -> Result<AgentView, String> {
        nested().patched(path, md("NEW"))
    }

    fn text(block: &Block) -> &str {
        match block {
            Block::Markdown { text } => text,
            _ => "?",
        }
    }

    #[test]
    fn replaces_a_top_level_block() {
        let v = patched("blocks[0]").unwrap();
        assert_eq!(text(&v.blocks[0]), "NEW");
        assert_eq!(v.blocks.len(), 7);
    }

    #[test]
    fn replaces_a_block_in_a_section_and_in_a_card_in_it() {
        let v = patched("blocks[1].blocks[0]").unwrap();
        let Block::Section { blocks, .. } = &v.blocks[1] else { panic!() };
        assert_eq!(text(&blocks[0]), "NEW");
        let v = patched("blocks[1].blocks[1].blocks[0]").unwrap();
        let Block::Section { blocks, .. } = &v.blocks[1] else { panic!() };
        let Block::Card { blocks, .. } = &blocks[1] else { panic!() };
        assert_eq!(text(&blocks[0]), "NEW");
    }

    #[test]
    fn replaces_a_block_in_a_column() {
        let v = patched("blocks[2].columns[1][1]").unwrap();
        let Block::Columns { columns } = &v.blocks[2] else { panic!() };
        assert_eq!(text(&columns[1][0]), "b0");
        assert_eq!(text(&columns[1][1]), "NEW");
        assert_eq!(text(&columns[0][0]), "a0");
    }

    #[test]
    fn replaces_a_block_in_a_tab() {
        let v = patched("blocks[3].tabs[0].blocks[0]").unwrap();
        let Block::Tabs { tabs } = &v.blocks[3] else { panic!() };
        assert_eq!(text(&tabs[0].blocks[0]), "NEW");
    }

    #[test]
    fn replaces_an_item_of_a_grid() {
        let v = patched("blocks[4].items[1]").unwrap();
        let Block::Grid { items, .. } = &v.blocks[4] else { panic!() };
        assert_eq!((text(&items[0]), text(&items[1])), ("g0", "NEW"));
    }

    #[test]
    fn replaces_a_block_a_table_row_opens_to() {
        let v = patched("blocks[5].details[1][0]").unwrap();
        let Block::Table { details, .. } = &v.blocks[5] else { panic!() };
        assert_eq!(text(&details[1].as_ref().unwrap()[0]), "NEW");
    }

    #[test]
    fn replaces_a_block_in_details() {
        let v = patched("blocks[6].blocks[0]").unwrap();
        let Block::Details { blocks, .. } = &v.blocks[6] else { panic!() };
        assert_eq!(text(&blocks[0]), "NEW");
    }

    #[test]
    fn replaces_a_container_whole() {
        let v = patched("blocks[2]").unwrap();
        assert_eq!(text(&v.blocks[2]), "NEW");
    }

    #[test]
    fn refuses_an_index_out_of_range_naming_the_path() {
        let err = patched("blocks[7]").unwrap_err();
        assert!(err.starts_with("blocks[7]:") && err.contains("there are 7 blocks"), "{err}");
        let err = patched("blocks[2].columns[1][2]").unwrap_err();
        assert!(err.starts_with("blocks[2].columns[1][2]:") && err.contains("there are 2 blocks"), "{err}");
        let err = patched("blocks[2].columns[2][0]").unwrap_err();
        assert!(err.contains("columns[2] is out of range") && err.contains("2 columns"), "{err}");
        let err = patched("blocks[3].tabs[1].blocks[0]").unwrap_err();
        assert!(err.contains("tabs[1] is out of range"), "{err}");
        let err = patched("blocks[5].details[2][0]").unwrap_err();
        assert!(err.contains("details[2] is out of range"), "{err}");
        let err = patched("blocks[9].blocks[0]").unwrap_err();
        assert!(err.contains("blocks[9] is out of range"), "{err}");
    }

    #[test]
    fn refuses_a_row_without_details() {
        let err = patched("blocks[5].details[0][0]").unwrap_err();
        assert!(err.contains("opens to nothing"), "{err}");
    }

    #[test]
    fn refuses_a_step_the_block_has_no_such_thing_as() {
        let err = patched("blocks[0].blocks[0]").unwrap_err();
        assert!(err.contains("blocks[0] is a block, which has no .blocks[0]"), "{err}");
        let err = patched("blocks[2].items[0]").unwrap_err();
        assert!(err.contains("blocks[2] is columns"), "{err}");
        let err = patched("blocks[1].columns[0][0]").unwrap_err();
        assert!(err.contains("blocks[1] is a section"), "{err}");
    }

    #[test]
    fn refuses_a_path_that_is_not_the_grammar() {
        for bad in [
            "", "blocks", "blocks[]", "blocks[x]", "blocks[-1]", "blocks[1", "block[1]", "blocks[1]blocks[0]",
            "blocks[1].", "blocks[1].rows[0]", "blocks[1].columns[0]", "blocks[3].tabs[0][0]",
            "blocks[1].blocks[0]x", "[1]", "blocks[+1]", "blocks[ 1]", "blocks[1].blocks[+0]",
        ] {
            let err = patched(bad).unwrap_err();
            assert!(err.starts_with(&format!("{bad}:")), "{bad}: {err}");
        }
    }

    fn walk(block: &Block, path: &str, found: &mut Vec<String>) {
        found.push(path.to_string());
        for (slot, list) in block.children() {
            for (i, inner) in list.iter().enumerate() {
                walk(inner, &format!("{path}{}", slot.patch_path(i)), found);
            }
        }
    }

    fn paths(view: &AgentView) -> Vec<String> {
        let mut found = Vec::new();
        for (i, block) in view.blocks.iter().enumerate() {
            walk(block, &format!("blocks[{i}]"), &mut found);
        }
        found
    }

    fn news(blocks: &[Block]) -> usize {
        let own = blocks.iter().filter(|b| text(b) == "NEW").count();
        own + blocks.iter().flat_map(|b| b.nested()).map(news).sum::<usize>()
    }

    #[test]
    fn every_path_the_walk_prints_is_one_a_patch_reads_back() {
        let found = paths(&nested());
        assert!(found.contains(&"blocks[4].items[1]".to_string()));
        assert!(found.contains(&"blocks[5].details[1][0]".to_string()));
        assert!(found.contains(&"blocks[3].tabs[0].blocks[0]".to_string()));
        for path in found {
            let v = patched(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(news(&v.blocks), 1, "{path}");
        }
    }

    #[test]
    fn leaves_the_rest_of_the_view_as_it_was() {
        let v = patched("blocks[2].columns[0][0]").unwrap();
        let mut expected = nested();
        let Block::Columns { columns } = &mut expected.blocks[2] else { panic!() };
        columns[0][0] = md("NEW");
        assert_eq!(v, expected);
    }
}
