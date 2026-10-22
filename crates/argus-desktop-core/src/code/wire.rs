//! The edge where the rope's bytes meet the protocol's UTF-16 units: a
//! change set leaving as the daemon counts it, and arriving the same way.
//! Nothing inside the editor counts in UTF-16, so a position is converted
//! once, here, against the text it is positioned in.

use argus_protocol::TextChange;
use ropey::Rope;

use super::change::{Change, ChangeSet, Misfit};

/// Why a set from the daemon does not fit the text it was said to be for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// A position past the end of the text.
    Past(u32),
    /// A position between the halves of a surrogate pair.
    Splits(u32),
    /// Out of order, overlapping or backwards.
    Misfit(Misfit),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::Past(at) => write!(f, "{at} is past the end of the text"),
            Refused::Splits(at) => write!(f, "{at} is between the halves of a surrogate pair"),
            Refused::Misfit(m) => m.fmt(f),
        }
    }
}

/// `set` as the protocol carries it, positioned in `doc`, the text before it.
pub fn to_wire(set: &ChangeSet, doc: &Rope) -> Vec<TextChange> {
    set.changes()
        .into_iter()
        .map(|c| TextChange {
            from: doc.byte_to_utf16_idx(c.from) as u32,
            to: doc.byte_to_utf16_idx(c.to) as u32,
            insert: c.insert,
        })
        .collect()
}

fn byte_of(doc: &Rope, unit: u32) -> Result<usize, Refused> {
    let at = unit as usize;
    if at > doc.len_utf16() {
        return Err(Refused::Past(unit));
    }
    let byte = doc.utf16_to_byte_idx(at);
    // The rope answers a position inside a pair with the pair's start.
    if doc.byte_to_utf16_idx(byte) != at {
        return Err(Refused::Splits(unit));
    }
    Ok(byte)
}

/// The set the protocol's `changes` say, made on `doc`.
pub fn from_wire(changes: &[TextChange], doc: &Rope) -> Result<ChangeSet, Refused> {
    let mut out = Vec::with_capacity(changes.len());
    for c in changes {
        out.push(Change { from: byte_of(doc, c.from)?, to: byte_of(doc, c.to)?, insert: c.insert.clone() });
    }
    ChangeSet::of(&out, doc.len()).map_err(Refused::Misfit)
}

#[cfg(test)]
mod tests;
