//! What a row of the outline says in marks and words (BOARD-WORK-3 to -7): the glyph and tone of its
//! state, the words of its tooltip, what stands at its end, how its text is set, and when a changed dot
//! is worth drawing.
//!
//! Pure, so each rule is held without a window; the row draws what these say. The Record lens's rows
//! use the same marks.

use argus_desktop_core::connection::page::{FeatureView, WorkRow, WorkAt};
use gpui_kit::Hsla;

use crate::theme::Tokens;

/// How loudly a mark says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Faint,
    Attention,
    Ok,
    Ref,
}

impl Tone {
    pub fn colour(self, tokens: &Tokens) -> Hsla {
        match self {
            Self::Faint => tokens.fg_faint,
            Self::Attention => tokens.attention,
            Self::Ok => tokens.ok,
            Self::Ref => tokens.ref_,
        }
    }
}

/// The glyph of a state and its tone: to do a ring, doing a half moon, done a tick, an open question a
/// mark, a decision made a diamond, anything else a quiet dot.
pub fn state_mark(state: &str) -> (&'static str, Tone) {
    match state {
        "todo" => ("\u{25cb}", Tone::Faint),
        "doing" => ("\u{25d0}", Tone::Attention),
        "done" => ("\u{2713}", Tone::Ok),
        "open" => ("?", Tone::Attention),
        "accepted" => ("\u{25c6}", Tone::Ref),
        "rejected" => ("\u{2715}", Tone::Faint),
        "withdrawn" | "superseded" => ("\u{2298}", Tone::Faint),
        _ => ("\u{b7}", Tone::Faint),
    }
}

/// The states the legend lists, with the words each says.
pub const MARK_WORDS: [(&str, &str); 6] =
    [("todo", "to do"), ("doing", "in progress"), ("done", "done"), ("accepted", "decided"), ("open", "open"), ("rejected", "rejected")];

/// The words a glyph stands for, as its tooltip says them: a state the legend does not list says itself.
pub fn mark_word(state: &str) -> &str {
    MARK_WORDS.iter().find(|(known, _)| *known == state).map_or(state, |(_, word)| word)
}

/// What stands at a row's end, the first of these that applies (BOARD-WORK-5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aside {
    /// An open decision waits on a person.
    Waiting,
    /// A task held up by an open decision.
    WaitsOn(String),
    /// A decision that is out: the state word.
    State(String),
    /// A task with subtasks: how many of them are done.
    Subtasks(usize, usize),
    Nothing,
}

pub fn aside(row: &WorkRow) -> Aside {
    if row.state == "open" {
        Aside::Waiting
    } else if let Some(waits) = &row.waits_on {
        Aside::WaitsOn(waits.clone())
    } else if row.state == "withdrawn" || row.state == "rejected" {
        Aside::State(row.state.to_owned())
    } else if let Some((done, total)) = row.subtasks {
        Aside::Subtasks(done, total)
    } else {
        Aside::Nothing
    }
}

/// How a row's text is set (BOARD-WORK-6): strong when it is the cursor's, muted when it is done or
/// out, and a rejected one struck through and faint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text {
    pub strong: bool,
    pub muted: bool,
    pub struck: bool,
}

pub fn text(state: &str, on: bool) -> Text {
    let struck = state == "rejected";
    Text { strong: on && !struck, muted: matches!(state, "done" | "withdrawn" | "superseded") && !struck, struck }
}

/// How a row's label is coloured: an open decision's in attention, another decision's in the link
/// colour (faint when it is rejected), a task's in the faint of a locator.
pub fn label_tone(row: &WorkRow) -> Tone {
    let decision = matches!(row.at, WorkAt::Decision { .. });
    match (decision, row.state) {
        (_, "open") => Tone::Attention,
        (true, "rejected") => Tone::Faint,
        (true, _) => Tone::Ref,
        (false, _) => Tone::Faint,
    }
}

/// A dot on every row says nothing: it is drawn only where some rows changed and some did not, and the
/// header says how many changed since (BOARD-WORK-7).
pub fn mixed(view: &FeatureView) -> bool {
    let changed = view.rows.iter().filter(|row| row.changed).count();
    changed > 0 && changed < view.rows.len()
}

#[cfg(test)]
mod tests;
