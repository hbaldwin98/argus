//! What a client sends as a paste: text with its line endings as a pty
//! wants them, and how large a file may be to cross as one.

/// The most a pasted file may be. Under the frame limit with room for the
/// message around it; a screenshot is a few hundred kilobytes.
pub const PASTE_FILE_MAX: usize = 32 * 1024 * 1024;

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
