//! What the editor's bar says of Vim: the mode, the keys of a command begun, the line being typed
//! after `:`, `/` or `?`, and the line the last key left (a recording, an error). The library the
//! page runs says these in its own panel; a window with a bar of its own draws them there.

use super::key::notation;
use super::{Mode, Shared, Vim};

/// The words to show, each empty when there is nothing to say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Said {
    pub mode: &'static str,
    /// The keys of a command not finished, as Vim writes them (`2d`, `"a`, `<C-w>`).
    pub pending: String,
    /// The `:` or `/` line as typed, or what the last key left, or `recording @q`.
    pub line: String,
}

pub fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Replace => "REPLACE",
        Mode::Visual => "VISUAL",
        Mode::VisualLine => "VISUAL LINE",
        Mode::VisualBlock => "VISUAL BLOCK",
        Mode::Command => "COMMAND",
        Mode::Search => "SEARCH",
        Mode::Confirm => "CONFIRM",
    }
}

pub fn said(vim: &Vim, sh: &Shared) -> Said {
    let pending = vim.pending().iter().copied().map(notation).collect();
    let line = if let Some((prefix, text, _)) = vim.prompt() {
        format!("{prefix}{text}")
    } else if let Some(message) = vim.message() {
        message.to_owned()
    } else if let Some((register, _)) = &sh.macros.recording {
        format!("recording @{register}")
    } else {
        String::new()
    };
    Said { mode: mode_name(vim.mode()), pending, line }
}

#[cfg(test)]
mod tests;
