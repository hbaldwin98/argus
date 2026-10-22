//! What the shortcuts sheet lists and which section it opens on: the keymap's groups and the
//! extensions' headings dealt into one section per place, the pairs of bindings that are one
//! thought written once, the search over the rows, and the section for where the person is (the
//! page's `keys/sections.ts`).
//!
//! The sheet is generated from the keymap, so a binding added there is on it with nothing else
//! changed. A window draws the sections; what it does with keys in the sheet is its own.

use super::chords::{leader, press, presses_of, KeyChord};
use super::keymap::{Action, Group, Place};
use crate::connection::page::Stage;
use crate::extensions::keys::ExtGroup;

/// What a section is made of: a keymap group or an extension's heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The place's name, or `ext/<title>` for an extension.
    pub place: String,
    pub title: String,
    pub bindings: Vec<SourceBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBinding {
    pub action: Option<Action>,
    pub chords: Vec<KeyChord>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    /// The alternatives, each the presses it takes in turn.
    pub keys: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The place's name, the id [`section_for`] answers with.
    pub id: String,
    pub title: String,
    /// The keys that follow the leader, written without it.
    pub rows: Vec<Row>,
    /// The pane's keys that are not chorded: pasting, paging, the leader sent on.
    pub without: Vec<Row>,
    /// Whether `rows` follow the leader, which the sheet then says once.
    pub chorded: bool,
}

/// The name a place goes by in the sheet, as the page's bindings call it.
pub fn place_id(place: Place) -> String {
    match serde_json::to_value(place) {
        Ok(serde_json::Value::String(name)) => name,
        _ => format!("{place:?}").to_lowercase(),
    }
}

/// Every source the sheet has: the keymap's groups, then one per extension.
pub fn sources(keymap: &[Group], extensions: &[ExtGroup]) -> Vec<Source> {
    let groups = keymap.iter().map(|group| Source {
        place: place_id(group.place),
        title: group.title.to_owned(),
        bindings: group
            .bindings
            .iter()
            .map(|binding| SourceBinding {
                action: Some(binding.action),
                chords: binding.chords.iter().map(KeyChord::from).collect(),
                label: binding.label.to_owned(),
            })
            .collect(),
    });
    let extensions = extensions.iter().map(|group| Source {
        place: format!("ext/{}", group.title),
        title: group.title.clone(),
        bindings: group
            .bindings
            .iter()
            .map(|binding| SourceBinding { action: None, chords: binding.chords.iter().map(KeyChord::from).collect(), label: binding.label.clone() })
            .collect(),
    });
    groups.chain(extensions).collect()
}

/// Bindings that are one row. The caps come from the chords themselves, so a rebound key reads
/// true; a set whose chords disagree stays as it was.
const MERGES: [(&[Action], &str, bool); 3] = [
    (&[Action::OtherSide, Action::PreviousSide], "the keys to the next / previous pane on screen", false),
    (&[Action::FocusLeft, Action::FocusRight, Action::FocusUp, Action::FocusDown], "the keys to the pane that way", true),
    (&[Action::MovePaneLeft, Action::MovePaneRight, Action::MovePaneUp, Action::MovePaneDown], "move the pane that way, trading places", true),
];

const ARROWS: [char; 4] = ['\u{2190}', '\u{2192}', '\u{2191}', '\u{2193}'];

/// `\u{2190} \u{2192} \u{2191} \u{2193}`, or `Shift + arrow` when the four hold a modifier; `None`
/// when they do not share one.
fn arrow_cap(chords: &[KeyChord]) -> Option<Vec<String>> {
    let parts: Option<Vec<(String, char)>> = chords
        .iter()
        .map(|chord| {
            let written = press(&KeyChord { leader: false, ..chord.clone() });
            let glyph = written.chars().last().filter(|c| ARROWS.contains(c))?;
            let prefix = written[..written.len() - glyph.len_utf8()].trim_end().to_owned();
            Some((prefix, glyph))
        })
        .collect();
    let parts = parts?;
    let prefix = parts.first()?.0.clone();
    if parts.iter().any(|(other, _)| *other != prefix) {
        return None;
    }
    Some(vec![if prefix.is_empty() {
        parts.iter().map(|(_, glyph)| glyph.to_string()).collect::<Vec<_>>().join(" ")
    } else {
        format!("{prefix} + arrow")
    }])
}

fn keys_of(chords: &[KeyChord], leader: &str, bare: bool) -> Vec<Vec<String>> {
    chords
        .iter()
        .map(|chord| {
            if chord.leader && bare {
                vec![press(&KeyChord { leader: false, ..chord.clone() })]
            } else {
                presses_of(chord, leader)
            }
        })
        .collect()
}

/// The rows of one source: a merged set is one row where its first member stood. `bare` writes a
/// chorded key without its leader.
fn rows_of(bindings: &[&SourceBinding], leader: &str, bare: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut done = vec![false; bindings.len()];
    for (i, binding) in bindings.iter().enumerate() {
        if done[i] {
            continue;
        }
        let merge = MERGES.iter().find(|(actions, _, _)| binding.action.is_some() && binding.action == actions.first().copied());
        if let Some((actions, label, arrows)) = merge {
            let at: Vec<Option<usize>> = actions.iter().map(|action| bindings.iter().position(|b| b.action == Some(*action))).collect();
            if at.iter().all(Option::is_some) {
                let at: Vec<usize> = at.into_iter().flatten().collect();
                let chords: Vec<KeyChord> = at.iter().flat_map(|&j| bindings[j].chords.iter().cloned()).collect();
                let cap = if *arrows { Some(arrow_cap(&chords)) } else { None };
                // A merge of arrows whose chords do not share a modifier is not made.
                if cap != Some(None) {
                    for &j in &at {
                        done[j] = true;
                    }
                    let keys = match cap {
                        Some(Some(cap)) => vec![cap],
                        _ => keys_of(&chords, leader, bare),
                    };
                    rows.push(Row { label: (*label).to_owned(), keys });
                    continue;
                }
            }
        }
        rows.push(Row { label: binding.label.clone(), keys: keys_of(&binding.chords, leader, bare) });
    }
    rows
}

/// A pane key that follows the leader: all but the leader sent on.
fn keyed(binding: &SourceBinding) -> bool {
    binding.action != Some(Action::Leader) && binding.chords.iter().any(|chord| chord.leader)
}

/// One section per source, in the order given. The pane's chorded keys lose their leader and the
/// rest wait under "without"; the leader sent on keeps both its presses, it being the one chord
/// that is the leader twice.
pub fn sections(sources: &[Source], leader: &str) -> Vec<Section> {
    sources
        .iter()
        .map(|source| {
            let all: Vec<&SourceBinding> = source.bindings.iter().collect();
            let chorded = source.bindings.iter().any(|binding| binding.chords.iter().any(|chord| chord.leader));
            if !chorded {
                return Section { id: source.place.clone(), title: source.title.clone(), rows: rows_of(&all, leader, false), without: Vec::new(), chorded };
            }
            let (with, without): (Vec<&SourceBinding>, Vec<&SourceBinding>) = all.into_iter().partition(|binding| keyed(binding));
            Section {
                id: source.place.clone(),
                title: source.title.clone(),
                rows: rows_of(&with, leader, true),
                without: rows_of(&without, leader, false),
                chorded,
            }
        })
        .collect()
}

/// Every section the sheet has for what the session sent.
pub fn all(keymap: &[Group], extensions: &[ExtGroup]) -> Vec<Section> {
    sections(&sources(keymap, extensions), &leader(keymap))
}

fn text(row: &Row) -> String {
    let keys: Vec<String> = row.keys.iter().map(|alternative| alternative.join(" ")).collect();
    format!("{} {}", row.label, keys.join(" ")).to_lowercase()
}

/// The sections with a row that has every word of `query` in its action or its key, narrowed to
/// those rows. An empty query is every section.
pub fn matching(all: &[Section], query: &str) -> Vec<Section> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_owned).collect();
    if words.is_empty() {
        return all.to_vec();
    }
    let keep = |rows: &[Row]| -> Vec<Row> { rows.iter().filter(|row| words.iter().all(|word| text(row).contains(word.as_str()))).cloned().collect() };
    all.iter()
        .map(|section| Section { rows: keep(&section.rows), without: keep(&section.without), ..section.clone() })
        .filter(|section| !section.rows.is_empty() || !section.without.is_empty())
        .collect()
}

/// A dialog with keys of its own that is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    Review,
    History,
    Sessions,
}

/// What the sheet is opened from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Where {
    /// The keys are in a pane's text area.
    pub pane: bool,
    pub dialog: Option<Dialog>,
    /// A sidebar row holds the keys.
    pub rail: bool,
    /// A feature is open, and its diagram expanded.
    pub outline: bool,
    pub diagram: bool,
    /// The stage up: `feature`, `checkouts`, `code` or another's name.
    pub stage: String,
}

/// The stage's name as `Where::stage` says it, which is the page's.
pub fn stage_id(stage: Stage) -> String {
    match serde_json::to_value(stage) {
        Ok(serde_json::Value::String(name)) => name,
        _ => String::new(),
    }
}

/// The section for where the person is: the layer with the keys, as the window asks them, then the
/// stage. Anywhere when nothing is more exact or the keymap has no such section.
pub fn section_for(at: &Where, ids: &[String]) -> String {
    let dialog = at.dialog.map(|dialog| match dialog {
        Dialog::Review => "review",
        Dialog::History => "history",
        Dialog::Sessions => "sessions",
    });
    let stage = ["feature", "checkouts", "code"].into_iter().find(|name| *name == at.stage);
    [dialog, at.pane.then_some("pane"), at.rail.then_some("rail"), at.diagram.then_some("diagram"), at.outline.then_some("outline"), stage]
        .into_iter()
        .flatten()
        .find(|id| ids.iter().any(|known| known == id))
        .unwrap_or("anywhere")
        .to_owned()
}

/// The section a Down or Up reaches from `current`, clamped at the ends (no wrap).
pub fn step(ids: &[String], current: &str, down: bool) -> Option<String> {
    let at = ids.iter().position(|id| id == current).unwrap_or(0);
    let next = if down { (at + 1).min(ids.len().checked_sub(1)?) } else { at.saturating_sub(1) };
    ids.get(next).cloned()
}

#[cfg(test)]
mod tests;
