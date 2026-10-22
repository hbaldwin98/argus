//! A sentence with parts that are not text in it (a key's cap, a mono file name), wrapped as the
//! page's inline text wraps: GPUI has no inline elements, so each word is a flex item that
//! carries its own trailing space and the line breaks between words, never inside one.

use gpui_kit::{div, prelude::*, AnyElement, Div, SharedString};

/// A piece of a flowing sentence.
pub enum Piece {
    /// Plain words, in the surrounding type.
    Text(String),
    /// Text kept whole: it measures as one run, which lines up with the page where per-word items drift wider, and breaks only between runs.
    Run(String),
    /// One unbreakable part, such as a cap or a name; the spaces around it are the text's.
    Part(AnyElement),
}

/// The words of a text, each with the space that follows it, so that nothing between words is
/// lost when they wrap.
pub fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    for ch in text.chars() {
        word.push(ch);
        if ch == ' ' {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The pieces as one wrapping row; the caller sets its width, alignment and type.
pub fn flow(pieces: Vec<Piece>) -> Div {
    let mut row = div().flex().flex_wrap().items_center();
    for piece in pieces {
        match piece {
            Piece::Text(text) => {
                for word in words(&text) {
                    row = row.child(div().flex_shrink_0().whitespace_nowrap().child(SharedString::from(word)));
                }
            }
            Piece::Run(text) => row = row.child(div().flex_shrink_0().whitespace_nowrap().child(SharedString::from(text))),
            Piece::Part(part) => row = row.child(div().flex_shrink_0().child(part)),
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words keep the space after them, so joining them gives the text back.
    #[test]
    fn words_keep_the_spaces_between_them() {
        let text = " to search the checkout's text instead.";
        assert_eq!(words(text).concat(), text);
        assert_eq!(words(text).first().map(String::as_str), Some(" "));
        assert_eq!(words("one two").len(), 2);
        assert!(words("").is_empty());
    }
}
