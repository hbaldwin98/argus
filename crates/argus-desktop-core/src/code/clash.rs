//! Whether a disk change and the person's touch the same text, and the
//! split of a set into what clears the other and what does not. The rule
//! is the page's (`sync.ts`): nothing is guessed where both wrote.

use super::change::{Change, ChangeSet};

/// A run of the buffer the disk changed too, and what the disk put there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clash {
    pub from: usize,
    pub to: usize,
    pub theirs: String,
}

/// Whether two runs of the same text touch the same characters. Runs that
/// only meet end to end do not: one line typed, the next rewritten. Two
/// insertions at one place do, since which goes first is a choice.
pub fn clash((af, at): (usize, usize), (bf, bt): (usize, usize)) -> bool {
    if af < bt && bf < at {
        return true;
    }
    if af == at && bf == bt {
        return af == bf;
    }
    if af == at {
        return bf < af && af < bt;
    }
    if bf == bt {
        return af < bf && bf < at;
    }
    false
}

/// `changes` split into the ones that clash with none of `against` and the
/// ones that clash with one, each still a set on the same text.
pub fn split(changes: &ChangeSet, against: &ChangeSet) -> (ChangeSet, ChangeSet) {
    let theirs: Vec<(usize, usize)> = against.changes().iter().map(|c| (c.from, c.to)).collect();
    let (mut clear, mut clashing): (Vec<Change>, Vec<Change>) = (Vec::new(), Vec::new());
    for c in changes.changes() {
        let hit = theirs.iter().any(|&span| clash((c.from, c.to), span));
        (if hit { &mut clashing } else { &mut clear }).push(c);
    }
    let len = changes.old_len();
    (
        ChangeSet::of(&clear, len).expect("a subset of a set is a set"),
        ChangeSet::of(&clashing, len).expect("a subset of a set is a set"),
    )
}

#[cfg(test)]
mod tests;
