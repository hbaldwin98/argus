//! A change's size as the Git stage says it, `+N −N`, wherever it is said: the strip's total, a
//! file's row and the head over its diff.
//!
//! A count of nothing is said in the faint text rather than in its tone: a new file's `−0` in red
//! beside every row is emphasis spent on nothing, and the column of counts reads as noise.

use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Div, Hsla, SharedString};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The colour one count is said in: its tone, or the faint text when it is nothing.
pub fn tone(n: usize, tone: Hsla, tokens: &Tokens) -> Hsla {
    if n == 0 {
        tokens.fg_faint
    } else {
        tone
    }
}

/// `+added −removed`, the two counts 4px apart on one line in the mono face, so a column of them
/// lines up figure by figure.
pub fn counts(added: usize, removed: usize, tokens: &Tokens) -> Div {
    h_flex()
        .flex_shrink_0()
        .gap(rem(tokens.sp_4))
        .font_family(SharedString::from(tokens.font_mono))
        .whitespace_nowrap()
        .child(div().text_color(tone(added, tokens.ok, tokens)).child(SharedString::from(format!("+{added}"))))
        .child(div().text_color(tone(removed, tokens.danger, tokens)).child(SharedString::from(format!("\u{2212}{removed}"))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_of_nothing_is_faint_and_any_other_wears_its_tone() {
        let tokens = crate::theme::themes::ledger::tokens();
        assert_eq!(tone(0, tokens.danger, &tokens), tokens.fg_faint);
        assert_eq!(tone(0, tokens.ok, &tokens), tokens.fg_faint);
        assert_eq!(tone(3, tokens.danger, &tokens), tokens.danger);
        assert_eq!(tone(1, tokens.ok, &tokens), tokens.ok);
    }
}
