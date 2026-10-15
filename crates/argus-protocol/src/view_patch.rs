//! How a patch replaces one block of a view the daemon holds: reading the
//! path an agent names the block by, in the grammar a refusal names blocks
//! by, and the view with that block swapped.

use crate::agent_view::{AgentView, Block};

/// One step from a block down to a list of blocks inside it, and which of
/// that list's blocks the path goes on to.
enum Step {
    /// `.blocks[i]`: a section, card or details.
    Blocks(usize),
    /// `.columns[c][i]`
    Column(usize, usize),
    /// `.tabs[t].blocks[i]`
    Tab(usize, usize),
    /// `.items[i]`
    Item(usize),
    /// `.details[r][i]`: the blocks a table row opens to.
    Detail(usize, usize),
}

impl AgentView {
    /// The view with the block at `path` replaced by `block`, or what is
    /// wrong with the path. The result is checked again by the caller, as a
    /// shown view is.
    pub fn patched(mut self, path: &str, block: Block) -> Result<AgentView, String> {
        let (top, steps) = parse(path)?;
        let mut list = &mut self.blocks;
        let mut at = top;
        let mut walked = format!("blocks[{top}]");
        for step in &steps {
            let len = list.len();
            let Some(here) = list.get_mut(at) else {
                return Err(format!("{path}: {walked} is out of range, there are {len} blocks"));
            };
            let (inner, next) = descend(here, step, &walked, path)?;
            walked.push_str(&describe(step));
            list = inner;
            at = next;
        }
        let len = list.len();
        match list.get_mut(at) {
            Some(slot) => *slot = block,
            None => return Err(format!("{path}: {walked} is out of range, there are {len} blocks")),
        }
        Ok(self)
    }
}

fn describe(step: &Step) -> String {
    match step {
        Step::Blocks(i) => format!(".blocks[{i}]"),
        Step::Column(c, i) => format!(".columns[{c}][{i}]"),
        Step::Tab(t, i) => format!(".tabs[{t}].blocks[{i}]"),
        Step::Item(i) => format!(".items[{i}]"),
        Step::Detail(r, i) => format!(".details[{r}][{i}]"),
    }
}

/// The list inside `block` that `step` goes into, and the index in it the
/// step names. `walked` is the path to `block`, for the refusal.
fn descend<'a>(
    block: &'a mut Block,
    step: &Step,
    walked: &str,
    path: &str,
) -> Result<(&'a mut Vec<Block>, usize), String> {
    match (step, block) {
        (Step::Blocks(i), Block::Section { blocks, .. })
        | (Step::Blocks(i), Block::Card { blocks, .. })
        | (Step::Blocks(i), Block::Details { blocks, .. }) => Ok((blocks, *i)),
        (Step::Column(c, i), Block::Columns { columns }) => {
            let count = columns.len();
            match columns.get_mut(*c) {
                Some(column) => Ok((column, *i)),
                None => Err(format!("{path}: {walked}.columns[{c}] is out of range, there are {count} columns")),
            }
        }
        (Step::Tab(t, i), Block::Tabs { tabs }) => {
            let count = tabs.len();
            match tabs.get_mut(*t) {
                Some(tab) => Ok((&mut tab.blocks, *i)),
                None => Err(format!("{path}: {walked}.tabs[{t}] is out of range, there are {count} tabs")),
            }
        }
        (Step::Item(i), Block::Grid { items, .. }) => Ok((items, *i)),
        (Step::Detail(r, i), Block::Table { details, .. }) => {
            let count = details.len();
            match details.get_mut(*r) {
                Some(Some(inner)) => Ok((inner, *i)),
                Some(None) => Err(format!("{path}: {walked}.details[{r}] is empty, the row opens to nothing")),
                None => Err(format!(
                    "{path}: {walked}.details[{r}] is out of range, there are {count} rows with details"
                )),
            }
        }
        (_, block) => Err(format!("{path}: {walked} is {}, which has no {}", kind(block), describe(step))),
    }
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

/// The index and what follows it, from text that starts `[3]`.
fn index<'p>(rest: &'p str, path: &str) -> Result<(usize, &'p str), String> {
    let inner = rest
        .strip_prefix('[')
        .ok_or_else(|| format!("{path}: expected `[` at {rest:?}"))?;
    let (digits, after) = inner
        .split_once(']')
        .ok_or_else(|| format!("{path}: a `[` is never closed"))?;
    let n = digits
        .parse::<usize>()
        .map_err(|_| format!("{path}: {digits:?} is not an index"))?;
    Ok((n, after))
}

/// The top-level index and the steps below it.
fn parse(path: &str) -> Result<(usize, Vec<Step>), String> {
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
                (Step::Blocks(i), after)
            }
            "columns" => {
                let (c, after) = index(after, path)?;
                let (i, after) = index(after, path)?;
                (Step::Column(c, i), after)
            }
            "tabs" => {
                let (t, after) = index(after, path)?;
                let after = after
                    .strip_prefix(".blocks")
                    .ok_or_else(|| format!("{path}: a tab's blocks are `.tabs[t].blocks[i]`"))?;
                let (i, after) = index(after, path)?;
                (Step::Tab(t, i), after)
            }
            "items" => {
                let (i, after) = index(after, path)?;
                (Step::Item(i), after)
            }
            "details" => {
                let (r, after) = index(after, path)?;
                let (i, after) = index(after, path)?;
                (Step::Detail(r, i), after)
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
    use crate::agent_view::Tab;

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
                    },
                ],
            },
            Block::Columns { columns: vec![vec![md("a0")], vec![md("b0"), md("b1")]] },
            Block::Tabs { tabs: vec![Tab { title: "one".into(), blocks: vec![md("t0")] }] },
            Block::Grid { min: Default::default(), items: vec![md("g0"), md("g1")] },
            table(vec![None, Some(vec![md("d0")])]),
            Block::Details { summary: "d".into(), chip: None, open: false, blocks: vec![md("e0")] },
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
            "blocks[1].blocks[0]x", "[1]",
        ] {
            let err = patched(bad).unwrap_err();
            assert!(err.starts_with(&format!("{bad}:")), "{bad}: {err}");
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
