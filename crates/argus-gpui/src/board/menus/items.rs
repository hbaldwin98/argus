//! What the Move menu and the Comes-after menu list, as plain data: a label, a rule, or a choice,
//! each choice with what it does (`moveMenu` and `afterMenu` of `feature_stage.ts`).
//!
//! The moves are the row's own (`FeatureRow::moves`), so which are offered and why one is not is
//! Rust's. This only arranges them: the agent's pane first, then the moves, then `Comes after...`
//! when the menu was opened from the peek, which alone knows what a feature may come after.

use argus_desktop_core::board::index_words::move_key;
use argus_desktop_core::connection::page::{AfterChoice, FeatureRow};

/// What choosing an item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Does {
    GoToPane(u64),
    /// One of a row's moves, by its `to`.
    Move(&'static str),
    /// Opens the Comes-after menu where this one was.
    ComesAfter,
    /// Says the feature comes after `on`, or with `waits` that it already does and this takes it back.
    Wait { on: String, waits: bool },
}

/// What stands in an item's first column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lead {
    None,
    /// The move already here.
    Here,
    /// A feature the open one may come after: ticked when it does.
    Tick(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub text: String,
    pub lead: Lead,
    /// A feature's number, in the mono face before its title.
    pub number: Option<i64>,
    /// Why it cannot be chosen, or `take back`: quiet words at the right edge.
    pub note: Option<String>,
    /// The key that does the same.
    pub key: Option<&'static str>,
    pub disabled: bool,
    pub does: Does,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Label(String),
    Rule,
    Item(Item),
}

fn item(text: impl Into<String>, does: Does) -> Item {
    Item { text: text.into(), lead: Lead::None, number: None, note: None, key: None, disabled: false, does }
}

/// The Move menu of `row`; `after` is the peek's list when the menu is opened from the peek.
pub fn moves(row: &FeatureRow, after: &[AfterChoice]) -> Vec<Entry> {
    let mut entries = vec![Entry::Label(format!("#{} {}", row.number, row.title))];
    if let Some(agent) = &row.agent {
        entries.push(Entry::Item(Item { key: Some("G"), ..item("Go to its pane", Does::GoToPane(agent.pane)) }));
        entries.push(Entry::Rule);
    }
    entries.push(Entry::Label("Move to".into()));
    for place in &row.moves {
        entries.push(Entry::Item(Item {
            lead: if place.disabled.as_deref() == Some("here now") { Lead::Here } else { Lead::None },
            note: place.disabled.clone(),
            key: if place.disabled.is_some() { None } else { move_key(place.to) },
            disabled: place.disabled.is_some(),
            ..item(place.label.clone(), Does::Move(place.to))
        }));
    }
    if !after.is_empty() {
        entries.push(Entry::Rule);
        entries.push(Entry::Item(item("Comes after\u{2026}", Does::ComesAfter)));
    }
    entries
}

/// The Comes-after menu of `row`.
pub fn comes_after(row: &FeatureRow, after: &[AfterChoice]) -> Vec<Entry> {
    std::iter::once(Entry::Label(format!("#{} comes after\u{2026}", row.number)))
        .chain(after.iter().map(|choice| {
            Entry::Item(Item {
                lead: Lead::Tick(choice.waits),
                number: Some(choice.number),
                note: choice.waits.then(|| "take back".to_owned()),
                ..item(choice.title.clone(), Does::Wait { on: choice.slug.clone(), waits: choice.waits })
            })
        }))
        .collect()
}

/// The positions of the items that can be chosen.
pub fn enabled(entries: &[Entry]) -> Vec<usize> {
    entries.iter().enumerate().filter(|(_, entry)| matches!(entry, Entry::Item(item) if !item.disabled)).map(|(at, _)| at).collect()
}

/// Where the highlight goes `by` choices on from `at`, wrapping at either end; from nowhere the first
/// choice, or the last going back.
pub fn stepped(enabled: &[usize], at: Option<usize>, by: isize) -> Option<usize> {
    let last = enabled.len().checked_sub(1)?;
    let here = at.and_then(|at| enabled.iter().position(|&one| one == at));
    let next = match here {
        Some(here) => (here as isize + by).rem_euclid(last as isize + 1) as usize,
        None if by < 0 => last,
        None => 0,
    };
    Some(enabled[next])
}
