//! What the bar over the editor says of the file, and in what colour: one word for each status
//! (`ts:code/editor.ts` `SAID`, `bar` and the `.ed-state` rules; EDITOR-STATE).

use super::live::Status;

/// How loudly a status is said: the bar's colour for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Faint,
    Muted,
    Danger,
    /// What needs the person without being a fault: the connection is gone.
    Attention,
}

/// The words the bar reads.
pub fn said(status: &Status) -> String {
    match status {
        Status::Opening => "Opening\u{2026}".into(),
        Status::Saved => "Saved".into(),
        Status::Unsaved => "Unsaved".into(),
        Status::Saving => "Saving\u{2026}".into(),
        Status::Clash => "An agent changed what you were editing".into(),
        Status::Failed(reason) => format!("Not saved: {reason}"),
        Status::Closed(reason) => format!("Closed: {reason}"),
        Status::Offline => "Disconnected; kept here until the daemon is back".into(),
    }
}

pub fn tone(status: &Status) -> Tone {
    match status {
        Status::Opening | Status::Saved => Tone::Faint,
        Status::Unsaved | Status::Saving => Tone::Muted,
        Status::Clash | Status::Failed(_) | Status::Closed(_) => Tone::Danger,
        Status::Offline => Tone::Attention,
    }
}

#[cfg(test)]
mod tests;
