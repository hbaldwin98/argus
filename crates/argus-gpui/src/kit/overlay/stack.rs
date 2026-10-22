//! The overlays that are up, bottom to top, and the rules that decide what a change does to them.
//!
//! No window in it: opening says what it closes, Escape says which overlay it answers, and a
//! close says whether a pane gets the keys back. The host (`host`) does what each says.

use super::kind::Overlay;

/// One overlay up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Up {
    pub kind: Overlay,
    /// Escape and a press on the scrim close it. The stopped card is the one that is not.
    pub dismissable: bool,
}

/// What opening an overlay did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opening {
    /// It was up already: opening is idempotent, and nothing changed.
    Already,
    /// It is up now, and these were closed first.
    Opened { closed: Vec<Overlay> },
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stack(Vec<Up>);

impl Stack {
    pub fn open(&mut self, up: Up) -> Opening {
        if self.contains(up.kind) {
            return Opening::Already;
        }
        let closed: Vec<Overlay> =
            if up.kind.exclusive() { self.0.iter().map(|up| up.kind).filter(|kind| kind.exclusive()).collect() } else { Vec::new() };
        self.0.retain(|held| !closed.contains(&held.kind));
        self.0.push(up);
        Opening::Opened { closed }
    }

    /// Takes `kind` out wherever it is; where it was, counted from the bottom.
    pub fn close(&mut self, kind: Overlay) -> Option<usize> {
        let at = self.0.iter().position(|up| up.kind == kind)?;
        self.0.remove(at);
        Some(at)
    }

    pub fn contains(&self, kind: Overlay) -> bool {
        self.0.iter().any(|up| up.kind == kind)
    }

    pub fn kinds(&self) -> Vec<Overlay> {
        self.0.iter().map(|up| up.kind).collect()
    }

    pub fn top(&self) -> Option<Overlay> {
        self.0.last().map(|up| up.kind)
    }

    /// Whether `kind` is the one on top, where a close is the dialog layer's own.
    pub fn is_top(&self, kind: Overlay) -> bool {
        self.top() == Some(kind)
    }

    /// What Escape answers: the topmost overlay that can be dismissed. One that cannot is passed
    /// over, as the page's stopped card holds no keys and Escape reaches what is under it
    /// (CROSS-8, SHELL-KEYS-19).
    pub fn escape(&self) -> Option<Overlay> {
        self.0.iter().rev().find(|up| up.dismissable).map(|up| up.kind)
    }

    pub fn dismissable(&self, kind: Overlay) -> bool {
        self.0.iter().any(|up| up.kind == kind && up.dismissable)
    }

    /// Whether an overlay that took the keys from a pane is up.
    pub fn takes_from_pane(&self) -> bool {
        self.0.iter().any(|up| up.kind.takes_the_keys_from_a_pane())
    }
}

/// Whether the pane takes the keys back when an overlay closes: it had them when the first
/// overlay that takes them opened, none of those is left, and a pane is on screen to type in
/// (SHELL-SEAM-10).
pub fn pane_regains_the_keys(had_them: bool, still_taken: bool, pane_shown: bool) -> bool {
    had_them && !still_taken && pane_shown
}

#[cfg(test)]
mod tests;
