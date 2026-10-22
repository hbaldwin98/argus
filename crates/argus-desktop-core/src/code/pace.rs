//! When what is typed is written, when the file is read again, and how long a write that failed
//! waits: the editor's timing as plain numbers and a function of "now", so the policy is tested
//! without a clock (`ts:code/editor.ts` `QUIET_MS` and kin).

/// How long typing may pause before what was typed is written: short enough that an agent reads
/// what the person sees, long enough that a word is one write and not one per letter.
pub const QUIET_MS: u64 = 200;
/// The longest typing goes unwritten while it never pauses.
pub const MOST_MS: u64 = 1000;
/// How long an agent's change stays lit.
pub const LIT_MS: u64 = 1600;
/// How long after a write settles the file is read again for its change marks: a burst of writes
/// is read once.
pub const REREAD_MS: u64 = 400;
/// How long after a failed write it is tried again.
pub const RETRY_MS: u64 = 1000;

/// What a key typed asks of the timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// Typing has gone on for `MOST_MS`: write now.
    Write,
    /// Write if nothing more is typed in this many milliseconds.
    After(u64),
}

/// A burst of typing: when it began.
#[derive(Debug, Default, Clone, Copy)]
pub struct Typing {
    since: Option<u64>,
}

impl Typing {
    /// A key was typed at `now` (milliseconds on any clock that only goes forward). The limit is
    /// checked only when a key arrives, so a forced write is made at the first key past it, not
    /// at the limit (EDITOR-SAVE-10).
    pub fn typed(&mut self, now: u64) -> Then {
        let since = *self.since.get_or_insert(now);
        if now.saturating_sub(since) >= MOST_MS {
            Then::Write
        } else {
            Then::After(QUIET_MS)
        }
    }

    /// The burst was written, or the write was asked for by hand: the next key begins another.
    pub fn written(&mut self) {
        self.since = None;
    }
}

#[cfg(test)]
mod tests;
