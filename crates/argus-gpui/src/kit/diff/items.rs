//! What the list holds, in order: every row, and the comment box among them under the row it is
//! about.

use argus_desktop_core::connection::page::DiffRow;

/// What the list holds at an index: a row, or the comment box among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Row(usize),
    Comment,
}

/// Where the comment box goes: under the last picked row, or the cursor's when
/// none is picked.
pub fn comment_under(rows: &[DiffRow]) -> Option<usize> {
    let mut under = None;
    for (i, row) in rows.iter().enumerate() {
        if row.picked || (row.cursor && under.is_none()) {
            under = Some(i);
        }
    }
    under
}

/// The list's items: every row, with the comment box after `under` when it is open.
pub fn items(rows: &[DiffRow], comment: bool) -> Vec<Item> {
    let under = comment.then(|| comment_under(rows)).flatten();
    let mut items = Vec::with_capacity(rows.len() + 1);
    for i in 0..rows.len() {
        items.push(Item::Row(i));
        if under == Some(i) {
            items.push(Item::Comment);
        }
    }
    // A box with no row to sit under (the lines it was on are gone) still shows
    // at the end, so its words are not hidden from the person typing them.
    if comment && under.is_none() {
        items.push(Item::Comment);
    }
    items
}

/// The index in `items` of the cursor's row, to bring into view.
pub fn cursor_item(rows: &[DiffRow], items: &[Item]) -> Option<usize> {
    let cursor = rows.iter().position(|row| row.cursor)?;
    items.iter().position(|item| *item == Item::Row(cursor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::diff::tests::row;

    #[test]
    fn the_comment_sits_under_the_last_picked_row_else_the_cursors() {
        let mut rows = vec![row("line"), row("line"), row("line"), row("line")];
        assert_eq!(comment_under(&rows), None);
        rows[1].cursor = true;
        assert_eq!(comment_under(&rows), Some(1));
        rows[2].picked = true;
        rows[3].picked = true;
        assert_eq!(comment_under(&rows), Some(3), "the last picked row, whatever the cursor");
    }

    #[test]
    fn an_open_comment_is_one_item_after_its_row_and_a_closed_one_is_none() {
        let mut rows = vec![row("line"), row("line"), row("line")];
        rows[1].cursor = true;
        assert_eq!(items(&rows, false), [Item::Row(0), Item::Row(1), Item::Row(2)]);
        assert_eq!(items(&rows, true), [Item::Row(0), Item::Row(1), Item::Comment, Item::Row(2)]);
        let none: Vec<DiffRow> = vec![row("line")];
        assert_eq!(items(&none, true), [Item::Row(0), Item::Comment], "a box with no row to sit under still shows");
    }

    #[test]
    fn the_cursors_item_counts_the_comment_above_it() {
        let mut rows = vec![row("line"), row("line"), row("line")];
        rows[0].picked = true;
        rows[2].cursor = true;
        let list = items(&rows, true);
        assert_eq!(cursor_item(&rows, &list), Some(3), "the box is under the picked row, above the cursor's");
    }
}
