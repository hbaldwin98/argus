//! The one status pill: a dot and a word on a tint of how loud the status is (`.spill`), and its
//! quiet form, the dot and the word alone, for a list where every row has one.
//!
//! A pane row, a feature's card, a task's detail and a session's header all say a status with
//! it, so the look is here once. The word is the daemon's, said as the rail says it unless the
//! caller has a better word for the same tone (`statusPill` in `kit/dom.ts`).

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, ElementId, Hsla, SharedString};

use super::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How a status is said, everywhere one is said: the two the daemon words as a person would not.
pub fn status_word(status: &str) -> &str {
    match status {
        "waiting" => "needs you",
        "needs-review" => "review",
        other => other,
    }
}

/// A feature's standing in the pill's vocabulary, so a feature reads like an agent wherever it is
/// named: working on it, held, done, or idle (`standingPill`).
pub fn standing_status(standing: &str) -> &'static str {
    match standing {
        "active" => "working",
        "done" => "done",
        "held" => "waiting",
        _ => "idle",
    }
}

/// What a status tints its pill with, whether it is loud (blocks on a person), and whether its dot
/// is a ring: an idle or ended one is not doing anything.
struct Tone {
    colour: Hsla,
    loud: bool,
    hollow: bool,
}

fn tone(status: &str, tokens: &Tokens) -> Tone {
    let (colour, loud, hollow) = match status {
        "waiting" | "needs-review" => (tokens.attention, true, false),
        "failed" => (tokens.danger, true, false),
        "done" => (tokens.ok, false, false),
        "working" => (tokens.accent, false, false),
        "info" => (tokens.info, false, false),
        _ => (tokens.fg_faint, false, true),
    };
    Tone { colour, loud, hollow }
}

/// `.spill`: the pill for `status` saying `word`, named `id` for a test or a screen reader.
pub fn status_pill(id: impl Into<ElementId>, word: impl Into<SharedString>, status: &str, tokens: &Tokens) -> impl IntoElement {
    let Tone { colour, loud, hollow } = tone(status, tokens);
    let (fill, edge) = if loud { (0.22, 0.60) } else { (0.12, 0.40) };
    let word = word.into();
    h_flex()
        .id(id)
        .test_support()
        .aria_label(word.clone())
        .h(rem(tokens.chip_h))
        .px(rem(tokens.sp_8))
        .gap(rem(tokens.sp_6))
        .items_center()
        .flex_shrink_0()
        .border_1()
        .border_color(colour.opacity(edge))
        .rounded(rem(tokens.r_pill))
        .bg(colour.opacity(fill))
        // Faint on faint fails 4.5:1 at 11px, so an ended or idle word is the muted one.
        .text_color(if hollow { tokens.fg_muted } else { colour })
        .text_size(rem(tokens.fs_xs))
        .weighted(tokens.weight_semibold)
        .whitespace_nowrap()
        .child(if hollow {
            Glyph::Ring6.at(tokens.dot_sm).text_color(colour).into_any_element()
        } else {
            div().size(rem(tokens.dot_sm)).flex_shrink_0().rounded_full().bg(colour).into_any_element()
        })
        .child(word)
}

/// The quiet pill: the dot and the word in the status's colour, with no tint and no edge, so a column
/// of them reads as words rather than as badges. What is not doing anything (idle, exited) is a faint
/// dot and a faint word, a step under the rest.
pub fn quiet_pill(id: impl Into<ElementId>, word: impl Into<SharedString>, status: &str, tokens: &Tokens) -> impl IntoElement {
    let Tone { colour, .. } = tone(status, tokens);
    let word = word.into();
    h_flex()
        .id(id)
        .test_support()
        .aria_label(word.clone())
        .h(rem(tokens.chip_h))
        .gap(rem(tokens.sp_6))
        .items_center()
        .flex_shrink_0()
        .text_color(colour)
        .text_size(rem(tokens.fs_xs))
        .weighted(tokens.weight_medium)
        .whitespace_nowrap()
        .child(div().size(rem(tokens.dot_sm)).flex_shrink_0().rounded_full().bg(colour))
        .child(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_statuses_the_daemon_words_for_a_machine_are_said_for_a_person() {
        assert_eq!(status_word("waiting"), "needs you");
        assert_eq!(status_word("needs-review"), "review");
        for other in ["working", "failed", "done", "idle", "exited"] {
            assert_eq!(status_word(other), other);
        }
    }

    #[test]
    fn a_feature_is_said_in_an_agents_vocabulary() {
        assert_eq!(standing_status("active"), "working");
        assert_eq!(standing_status("done"), "done");
        assert_eq!(standing_status("held"), "waiting");
        assert_eq!(standing_status("queued"), "idle", "anything else is idle");
    }

    #[test]
    fn what_waits_on_a_person_or_failed_is_louder_than_what_does_not() {
        let tokens = crate::theme::themes::ledger::tokens();
        for (status, loud) in [("waiting", true), ("needs-review", true), ("failed", true), ("working", false), ("done", false), ("idle", false)] {
            assert_eq!(tone(status, &tokens).loud, loud, "{status}");
        }
    }

    #[test]
    fn only_a_status_that_is_not_doing_anything_has_a_ring_for_a_dot() {
        let tokens = crate::theme::themes::ledger::tokens();
        for (status, hollow) in [("idle", true), ("exited", true), ("anything else", true), ("working", false), ("failed", false)] {
            assert_eq!(tone(status, &tokens).hollow, hollow, "{status}");
        }
    }

    #[test]
    fn a_quiet_pill_says_an_ended_status_in_the_faint_colour_and_a_loud_one_in_its_own() {
        let tokens = crate::theme::themes::ledger::tokens();
        assert_eq!(tone("exited", &tokens).colour, tokens.fg_faint);
        assert_eq!(tone("waiting", &tokens).colour, tokens.attention);
        assert_eq!(tone("failed", &tokens).colour, tokens.danger);
    }
}
