//! What the Feature index says in words, for a window that draws it natively: a row's cells and
//! note, a card's corner and work line, an empty column's two lines, the strip's counts, the key
//! bar, and what a card dropped on a column asks or is told.
//!
//! The page writes these itself (`feature_stage.ts`); the GPUI window reads them from here so the
//! sentences are Rust's, tested once. A drop is judged by [`drop_verdict`] for both the view, which
//! lights a column only when it takes the card, and the session, which acts on `Call::DropFeature`.

use serde::Deserialize;

use crate::board::features::Layout;
use crate::chrome::strip::Part;
use crate::connection::page::{Agent, FeatureRow, Features, Move};

/// A column of the board, as a card is dropped on one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum FeatureColumn {
    Queued,
    Active,
    Done,
    Held,
    Archived,
}

impl FeatureColumn {
    /// The word a row's `group` and a move's `to` say for it.
    pub fn word(self) -> &'static str {
        match self {
            FeatureColumn::Queued => "queued",
            FeatureColumn::Active => "active",
            FeatureColumn::Done => "done",
            FeatureColumn::Held => "held",
            FeatureColumn::Archived => "archived",
        }
    }

    pub fn of(group: &str) -> Option<FeatureColumn> {
        [FeatureColumn::Queued, FeatureColumn::Active, FeatureColumn::Done, FeatureColumn::Held, FeatureColumn::Archived].into_iter().find(|c| c.word() == group)
    }

    pub fn label(self) -> &'static str {
        match self {
            FeatureColumn::Queued => "Queued",
            FeatureColumn::Active => "Active",
            FeatureColumn::Done => "Done",
            FeatureColumn::Held => "Held",
            FeatureColumn::Archived => "Archived",
        }
    }

    /// What an empty column says: its title, and what lands there.
    pub fn empty(self) -> (&'static str, &'static str) {
        match self {
            FeatureColumn::Queued => ("Nothing queued", "Written-down features wait here"),
            FeatureColumn::Active => ("Nothing active", "A feature moves here when work starts"),
            FeatureColumn::Done => ("Nothing done yet", "Lands here when every task is done"),
            FeatureColumn::Held => ("Nothing held", "Drag a card here to set it aside"),
            FeatureColumn::Archived => ("Nothing archived", "Drag a card here to accept it"),
        }
    }
}

/// How loud a word is: attention for what is being worked, ok for what is settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Attention,
    Ok,
}

/// The colour a group's label takes in the list.
pub fn group_tone(column: FeatureColumn) -> Tone {
    match column {
        FeatureColumn::Active => Tone::Attention,
        FeatureColumn::Done => Tone::Ok,
        _ => Tone::Plain,
    }
}

/// Whether a group reads quietly: its titles and branches are muted.
pub fn quiet(row: &FeatureRow) -> bool {
    matches!(row.group, "done" | "held" | "archived")
}

/// The tasks column of a list row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tasks {
    New,
    None,
    Progress { done: usize, total: usize },
}

pub fn tasks(row: &FeatureRow) -> Tasks {
    if row.standing == "new" {
        Tasks::New
    } else if row.tasks_total == 0 {
        Tasks::None
    } else {
        Tasks::Progress { done: row.tasks_done, total: row.tasks_total }
    }
}

/// The questions column: how many are open, else how many were decided.
pub fn questions(row: &FeatureRow) -> Option<(String, Tone)> {
    if row.open > 0 {
        Some((format!("{} open", row.open), Tone::Attention))
    } else if row.decided > 0 {
        Some((format!("{} decided", row.decided), Tone::Ok))
    } else {
        None
    }
}

/// `#3, #5`: the numbers a feature comes after.
pub fn after(row: &FeatureRow) -> String {
    row.after.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ")
}

/// One piece of the line under a list row's title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bit {
    Text(String),
    Faint(String),
    Mono(String),
    MonoFaint(String),
    Agent(Agent),
}

/// What a row says under its title, first match winning: why it is held, what it comes after, what
/// comes next with its agent, then what its standing says.
pub fn note(row: &FeatureRow) -> Vec<Bit> {
    if let Some(held) = &row.held {
        return vec![Bit::Faint("held: ".into()), Bit::Text(format!("\u{201c}{held}\u{201d}"))];
    }
    if !row.after.is_empty() {
        return vec![Bit::Faint("after ".into()), Bit::Mono(after(row))];
    }
    if row.next.is_some() || row.agent.is_some() {
        let mut bits = Vec::new();
        if let Some(next) = &row.next {
            bits.extend([Bit::Faint("next ".into()), Bit::MonoFaint(next.label.clone()), Bit::Text(format!(" {}", next.text))]);
        }
        if row.next.is_some() && row.agent.is_some() {
            bits.push(Bit::Text(" \u{b7} ".into()));
        }
        bits.extend(row.agent.clone().map(Bit::Agent));
        return bits;
    }
    match (row.standing, row.group) {
        ("new", _) => vec![Bit::Faint("nothing written yet".into())],
        (_, "done") => vec![Bit::Faint("every task done \u{b7} archive it to accept it".into())],
        (_, "archived") => vec![Bit::Faint("archived".into())],
        _ => Vec::new(),
    }
}

/// Why the list is empty.
pub fn nothing_listed(features: &Features) -> String {
    if features.filter.is_empty() {
        "Every feature here is held or archived.".into()
    } else if features.folded == 0 {
        format!("Nothing matches \u{201c}{}\u{201d}.", features.filter)
    } else if !features.show_folded {
        format!("Only held or archived features match \u{201c}{}\u{201d}.", features.filter)
    } else {
        "Every feature here is held or archived.".into()
    }
}

/// A card's corner: its agent's status, what it comes after, or `new`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Corner {
    Agent(&'static str),
    After(String),
    New,
}

pub fn corner(row: &FeatureRow) -> Option<Corner> {
    if let Some(agent) = &row.agent {
        Some(Corner::Agent(agent.status))
    } else if !row.after.is_empty() {
        Some(Corner::After(after(row)))
    } else if row.standing == "new" {
        Some(Corner::New)
    } else {
        None
    }
}

/// A card's work line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Work {
    Words(&'static str),
    Progress { done: usize, total: usize, questions: Option<(String, Tone)> },
}

pub fn work(row: &FeatureRow) -> Work {
    if row.standing == "new" {
        Work::Words("nothing written yet")
    } else if row.tasks_total == 0 {
        Work::Words("no tasks yet")
    } else {
        Work::Progress { done: row.tasks_done, total: row.tasks_total, questions: questions(row) }
    }
}

/// A card's footer cut where its time begins: the account, and `ago` kept apart so it never wraps
/// alone. The split is made only when the text ends with `ago`.
pub fn footer(text: &str) -> (&str, Option<&str>) {
    match text.rfind(" \u{b7} ") {
        Some(at) if text.ends_with("ago") => (&text[..at], Some(&text[at + " \u{b7} ".len()..])),
        _ => (text, None),
    }
}

/// What the strip says beside the title: the counts, a count of nothing left out.
pub fn counts(features: &Features) -> Vec<Part> {
    let parts = [
        (features.active, " active", Part::Attention as fn(String) -> Part),
        (features.queued, " queued", Part::Text as fn(String) -> Part),
        (features.done, " done", Part::Ok as fn(String) -> Part),
    ];
    let mut said = Vec::new();
    for (count, word, part) in parts.into_iter().filter(|(count, ..)| *count > 0) {
        if !said.is_empty() {
            said.push(Part::Text(" \u{b7} ".into()));
        }
        said.push(part(format!("{count}{word}")));
    }
    said
}

pub const COUNTS_TIP: &str = "N new \u{b7} Enter open \u{b7} V list or board \u{b7} M move to\u{2026} \u{b7} G go to its pane";

/// The fold toggle's words and the count after them; only the list has one, and only while there is
/// something folded.
pub fn fold_toggle(features: &Features) -> Option<(&'static str, usize)> {
    (features.layout == "list" && features.folded > 0)
        .then_some((if features.show_folded { "Fold held and archived " } else { "Show held and archived " }, features.folded))
}

/// The keys under the index, with what each does; naming the layout `V` would switch to.
pub fn key_bar(layout: &str) -> [(&'static str, &'static str); 5] {
    [("N", "new"), ("Enter", "open"), ("V", if layout == "board" { "list" } else { "board" }), ("M", "move"), ("G", "go to pane")]
}

pub const BOARD_HINT: &str = "Drag a card to Held to set it aside";

/// The key a Move to item shows beside it.
pub fn move_key(to: &str) -> Option<&'static str> {
    match to {
        "archived" | "reopen" => Some("Shift A"),
        "held" | "unhold" => Some("H"),
        _ => None,
    }
}

/// What dropping a card on a column does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Landing {
    /// Its own column: nothing.
    Nothing,
    /// Held asks why first, which only a window can.
    Hold,
    Archive,
    /// A column that follows the work: the notice that says why.
    Refuse(String),
}

fn offered(row: &FeatureRow, to: FeatureColumn) -> bool {
    matches!(to, FeatureColumn::Held | FeatureColumn::Archived) && row.moves.iter().any(|m: &Move| m.to == to.word() && m.disabled.is_none())
}

pub fn drop_verdict(row: &FeatureRow, on: FeatureColumn) -> Landing {
    if on.word() == row.group {
        Landing::Nothing
    } else if offered(row, on) {
        if on == FeatureColumn::Held {
            Landing::Hold
        } else {
            Landing::Archive
        }
    } else {
        let why = row.moves.iter().find(|m| m.to == on.word()).and_then(|m| m.disabled.as_deref());
        Landing::Refuse(format!("{} follows the work{}.", on.label(), why.map(|why| format!(": {why}")).unwrap_or_default()))
    }
}

/// The column a card over `on` would be taken by: only Held and Archived light up.
pub fn target(row: &FeatureRow, on: Option<FeatureColumn>) -> Option<FeatureColumn> {
    on.filter(|column| offered(row, *column))
}

/// The banner under a taking column's head while a card is over it.
pub fn drop_banner(row: &FeatureRow, on: FeatureColumn) -> String {
    match on {
        FeatureColumn::Archived => format!("Drop to archive: you accept #{} as finished", row.number),
        _ => format!("Drop to hold: you say why #{} waits", row.number),
    }
}

/// The placeholder left where the dragged card was.
pub fn dragged(row: &FeatureRow, over: Option<FeatureColumn>) -> String {
    match target(row, over) {
        Some(column) => format!("#{} is being dragged to {}", row.number, column.label()),
        None => format!("#{} is being dragged", row.number),
    }
}

/// The index's layout from its word.
pub fn layout_of(features: &Features) -> Layout {
    Layout::from_word(features.layout).unwrap_or_default()
}

#[cfg(test)]
mod tests;
