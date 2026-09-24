//! Text crossing between Argus and the clipboard.
//!
//! Windows gives a terminal application no way to tell a paste from fast
//! typing: the console delivers pasted text as ordinary key records, with
//! none of the bracketing a Unix terminal supplies (see [`crate::paste`],
//! which infers it from timing). Reading the clipboard ourselves sidesteps
//! the guess entirely — an explicit paste key is never wrong about what it
//! is.
//!
//! A desktop clipboard only exists where Argus runs on the desk. Over SSH,
//! or inside a multiplexer on a remote host, the one clipboard the user
//! means is the terminal's, and only OSC 52 reaches it.

use std::io::{self, Write};

use base64::Engine;

/// The clipboard's text, or `None` when there is no clipboard to read or
/// nothing text-shaped on it.
pub fn read() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Put text on the clipboard, reporting whether it could have arrived.
///
/// Both routes are always taken, since neither knows whether it is the one
/// that matters. The desktop clipboard can fail loudly but is out of reach
/// over SSH; OSC 52 reaches the user's terminal through SSH and herdr
/// (tmux forwards it with `set-clipboard on`), but nothing answers it, so
/// once written it counts as delivered.
pub fn write(text: &str) -> bool {
    let desktop = arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .is_ok();
    let terminal = write_to_terminal(text).is_ok();
    desktop || terminal
}

/// Goes straight to stdout, not through the frame buffer: text is only
/// copied while handling input, between frames, when that buffer has
/// already been flushed, so the sequence cannot land inside a frame.
fn write_to_terminal(text: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(osc52(text).as_bytes())?;
    stdout.flush()
}

/// BEL rather than ST as the terminator: every terminal that implements
/// OSC 52 accepts BEL, and some older ones accept nothing else.
fn osc52(text: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    format!("\x1b]52;c;{encoded}\x07")
}

/// Line endings as a pty wants them.
///
/// Windows puts `\r\n` on the clipboard. A bare `\r` written to a pty is
/// Enter — the very thing an explicit paste exists to avoid — so every
/// flavour of line ending becomes a plain newline, which inside a
/// bracketed paste is literal text.
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_line_endings_do_not_arrive_as_enter() {
        assert_eq!(
            normalize("one\r\ntwo\rthree\nfour"),
            "one\ntwo\nthree\nfour"
        );
    }

    #[test]
    fn copied_text_reaches_the_terminal_as_base64_osc_52() {
        assert_eq!(osc52("hi\n"), "\x1b]52;c;aGkK\x07");
    }

    #[test]
    fn text_without_line_endings_is_left_alone() {
        assert_eq!(normalize("just text"), "just text");
    }
}
