//! What the person typed, found from the buffer before and after. The editor reports its text
//! changed and not how, so the change is read off the two ropes by what they share at each end
//! (`ts:code/sync.ts` `between`, on a rope, so a key in a long file does not copy the file).

use ropey::Rope;

use super::change::{Change, ChangeSet};

/// The one change that turns `old` into `new`, or `None` when they are the same text. Whole
/// characters only, so it never splits a character, and so never a surrogate pair in the UTF-16
/// the wire counts.
pub fn found(old: &Rope, new: &Rope) -> Option<Change> {
    let (olen, nlen) = (old.len(), new.len());
    let shared = olen.min(nlen);
    let mut start = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    if start == olen && start == nlen {
        return None;
    }
    // The bytes before `start` agree, so a boundary in one is a boundary in the other.
    start = old.floor_char_boundary(start);
    let mut end = old.bytes_at(olen).reversed().zip(new.bytes_at(nlen).reversed()).take(shared - start).take_while(|(a, b)| a == b).count();
    // A shared tail starts at a boundary of both, or it is cut back to one.
    while end > 0 && !(old.is_char_boundary(olen - end) && new.is_char_boundary(nlen - end)) {
        end -= 1;
    }
    Some(Change { from: start, to: olen - end, insert: new.slice(start..nlen - end).to_string() })
}

/// The set that turns `old` into `new`, made on `old`.
pub fn set(old: &Rope, new: &Rope) -> Option<ChangeSet> {
    let change = found(old, new)?;
    Some(ChangeSet::of(&[change], old.len()).expect("one change inside the text it was found in"))
}

#[cfg(test)]
mod tests;
