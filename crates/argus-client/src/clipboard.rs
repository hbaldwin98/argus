//! The clipboard, read directly.
//!
//! Windows gives a terminal application no way to tell a paste from fast
//! typing: the console delivers pasted text as ordinary key records, with
//! none of the bracketing a Unix terminal supplies (see [`crate::paste`],
//! which infers it from timing). Reading the clipboard ourselves sidesteps
//! the guess entirely — an explicit paste key is never wrong about what it
//! is.

/// The clipboard's text, or `None` when there is no clipboard to read or
/// nothing text-shaped on it.
pub fn read() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
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
        assert_eq!(normalize("one\r\ntwo\rthree\nfour"), "one\ntwo\nthree\nfour");
    }

    #[test]
    fn text_without_line_endings_is_left_alone() {
        assert_eq!(normalize("just text"), "just text");
    }
}
