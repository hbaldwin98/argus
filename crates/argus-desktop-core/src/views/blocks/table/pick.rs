//! Picking rows of a table: which keys name the rows, what the select-all box shows, and what
//! ticking it does to the rows the filter hides.
//!
//! Only a table whose agent gave one key per row can be picked from, and only when it said `one` or
//! `many` (TABLE-4).

use std::collections::BTreeSet;

use argus_protocol::RowSelect;

/// What the select-all box shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    Clear,
    Mixed,
    All,
}

/// The mode a table is picked in: its own, when every row has a key, else none.
pub fn mode(select: RowSelect, keys: usize, rows: usize) -> RowSelect {
    if keys == rows {
        select
    } else {
        RowSelect::None
    }
}

/// The key a row's open state is remembered by: the agent's, else its place.
pub fn row_ids(row_keys: &[String], rows: usize) -> Vec<String> {
    let keyed = row_keys.len() == rows;
    (0..rows).map(|r| if keyed { row_keys.get(r).cloned().unwrap_or_else(|| format!("#{r}")) } else { format!("#{r}") }).collect()
}

/// The select-all box over the rows the filter shows: checked when every one is picked, mixed when
/// some are, and clear when none is or there is nothing to pick.
pub fn all_state(shown: &[&str], chosen: &BTreeSet<String>) -> Tri {
    let picked = shown.iter().filter(|key| chosen.contains(**key)).count();
    match picked {
        0 => Tri::Clear,
        n if n == shown.len() => Tri::All,
        _ => Tri::Mixed,
    }
}

/// What ticking or clearing select-all leaves picked: the shown rows are added or taken away, and a
/// row the filter hides is neither.
pub fn toggle_all(chosen: &BTreeSet<String>, shown: &[&str], on: bool) -> BTreeSet<String> {
    let mut next = chosen.clone();
    for key in shown {
        if on {
            next.insert((*key).to_owned());
        } else {
            next.remove(*key);
        }
    }
    next
}

/// What the radio of one row leaves picked: that row alone.
pub fn only(key: &str) -> BTreeSet<String> {
    BTreeSet::from([key.to_owned()])
}

/// What the bar says of the pick: `2 selected`, or nothing when the table cannot be picked from.
pub fn count_line(mode: RowSelect, picked: usize) -> String {
    if mode == RowSelect::None {
        String::new()
    } else {
        format!("{picked} selected")
    }
}

/// Whether the agent's actions are out of reach: a table that is picked from offers them on a pick.
pub fn actions_off(mode: RowSelect, picked: usize) -> bool {
    mode != RowSelect::None && picked == 0
}

#[cfg(test)]
mod tests;
