//! What may be typed into an agent's terminal: one line with no control
//! character in it, since a newline there sends a message early and an
//! escape does worse.

/// `text` with every control character made a space.
pub fn plain(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newline_or_an_escape_becomes_a_space() {
        assert_eq!(plain("Fix\nthe \x1b[2Jtitle\t"), "Fix the  [2Jtitle ");
    }
}
