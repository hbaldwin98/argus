//! What the comment box says to the session and how it sits in the rows it is among: the one thing
//! the review's box and the Code stage's differ in (`review/comment_box.ts` is the page's one box, and
//! each stage handed it its own `ways`).
//!
//! A way is the call that sends the words or puts the box away, the call that chooses an agent, and
//! the room round the card. The box itself, its words and its keys are the same in both.

use argus_desktop_core::code::stage::{CodeClick, CodeGesture};
use argus_desktop_core::connection::page::{Call, ReviewClick};

use crate::kit::diff::gutter::{reading_gutter, GUTTER};
use crate::theme::Tokens;

/// How a stage's comment box talks to the session and sits among its rows.
pub trait Ways {
    /// The call that sends `body`, or with none puts the box away.
    fn send(&self, body: Option<String>) -> Call;

    /// The call that chooses the agent the comment goes to.
    fn recipient(&self, pane: u64) -> Call;

    /// How far the card stands in from the left and from the right, so its words begin where the
    /// rows' code does.
    fn margin(&self, tokens: &Tokens) -> (f32, f32);

    /// The widest the card goes, when it is held to one.
    fn widest(&self, tokens: &Tokens) -> Option<f32>;
}

/// The review's box (`.comment-card` in a diff): under the number columns, to the right edge.
pub struct Review;

impl Ways for Review {
    fn send(&self, body: Option<String>) -> Call {
        Call::ReviewComment { body }
    }

    fn recipient(&self, pane: u64) -> Call {
        Call::ReviewClick { target: ReviewClick::Recipient { pane } }
    }

    fn margin(&self, tokens: &Tokens) -> (f32, f32) {
        (GUTTER, tokens.sp_12)
    }

    fn widest(&self, _: &Tokens) -> Option<f32> {
        None
    }
}

/// The Code stage's box (`.code-body .comment-card`): under its one number column and marker, with
/// room at the right, and no wider than a comfortable line.
pub struct Code;

impl Ways for Code {
    fn send(&self, body: Option<String>) -> Call {
        Call::Code { gesture: CodeGesture::Comment { body } }
    }

    fn recipient(&self, pane: u64) -> Call {
        Call::Code { gesture: CodeGesture::Click { target: CodeClick::Recipient { pane } } }
    }

    fn margin(&self, tokens: &Tokens) -> (f32, f32) {
        (reading_gutter(tokens), tokens.sp_16)
    }

    fn widest(&self, tokens: &Tokens) -> Option<f32> {
        Some(tokens.code_comment_max_w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REVIEW-37 and CODE-50: each stage's box sends the call its own session answers.
    #[test]
    fn each_stage_sends_the_words_and_the_choice_as_its_own_call() {
        assert_eq!(format!("{:?}", Review.send(Some("hi".into()))), "ReviewComment { body: Some(\"hi\") }");
        assert_eq!(format!("{:?}", Review.send(None)), "ReviewComment { body: None }");
        assert_eq!(format!("{:?}", Review.recipient(7)), "ReviewClick { target: Recipient { pane: 7 } }");
        assert_eq!(format!("{:?}", Code.send(Some("hi".into()))), "Code { gesture: Comment { body: Some(\"hi\") } }");
        assert_eq!(format!("{:?}", Code.send(None)), "Code { gesture: Comment { body: None } }");
        assert_eq!(format!("{:?}", Code.recipient(7)), "Code { gesture: Click { target: Recipient { pane: 7 } } }");
    }

    /// CODE-50: the Code stage's card is in 68 px, and is held to 720.
    #[test]
    fn the_codes_card_clears_one_number_column_and_the_reviews_two() {
        let tokens = crate::theme::themes::ledger::tokens();
        assert_eq!(Code.margin(&tokens), (68.0, 16.0));
        assert_eq!(Review.margin(&tokens), (104.0, 12.0));
        assert_eq!((Code.widest(&tokens), Review.widest(&tokens)), (Some(720.0), None));
    }
}
