//! A progress bar: how much of something is done, as a fill in a small track (`.pbar`).
//!
//! The same bar is a task count in the feature list and on a card, the tasks of a header, and a
//! checklist's tally; its thin form is a hairline that sits under a line of text and takes the
//! width it is given. What the percentage is, is decided here once: a whole number, rounded, and
//! held to the track.

use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, relative, ElementId, SharedString};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `round(done / total * 100)`, held to 0..=100; none when there is nothing to be done (a total
/// of zero has no share, not a full one).
pub fn percent(done: u32, total: u32) -> Option<u8> {
    (total > 0).then(|| ((f64::from(done) / f64::from(total)) * 100.0).round().clamp(0.0, 100.0) as u8)
}

/// `.pbar`: a `--meter-w` track `--meter-h` tall on `--fill`, filled to `percent` in `--ok`.
/// `label` is what a reader that cannot see it is told (`3 of 7 tasks`).
pub fn bar(id: impl Into<ElementId>, percent: u8, label: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    track(id, percent, label, tokens.ok, tokens).w(rem(tokens.meter_w)).h(rem(tokens.meter_h)).flex_shrink_0()
}

/// The thin bar: `--meter-h-thin` tall, as wide as its parent, filled in the faint colour, for a
/// share that is said quietly under its words (a download under the model it fetches).
pub fn thin(id: impl Into<ElementId>, percent: u8, label: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    track(id, percent, label, tokens.fg_faint, tokens).w_full().h(rem(tokens.meter_h_thin))
}

fn track(id: impl Into<ElementId>, percent: u8, label: impl Into<SharedString>, filled: gpui_kit::Hsla, tokens: &Tokens) -> crate::kit::icon_button::Node {
    div()
        .id(id)
        .test_support()
        .aria_label(label)
        .overflow_hidden()
        .rounded(rem(tokens.r_pill))
        .bg(tokens.fill)
        .child(div().h_full().w(relative(f32::from(percent.min(100)) / 100.0)).bg(filled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_is_rounded_to_a_whole_percent() {
        assert_eq!(percent(1, 3), Some(33));
        assert_eq!(percent(2, 3), Some(67));
        assert_eq!(percent(1, 8), Some(13), "12.5 rounds up, as the page's Math.round does");
        assert_eq!(percent(7, 7), Some(100));
        assert_eq!(percent(0, 7), Some(0));
    }

    #[test]
    fn nothing_to_do_has_no_share_and_more_done_than_there_is_stops_at_all() {
        assert_eq!(percent(0, 0), None);
        assert_eq!(percent(5, 0), None);
        assert_eq!(percent(9, 7), Some(100));
    }

    #[test]
    fn the_thin_bar_is_a_hairline_under_the_full_one() {
        // The proposal's 2px bar, for the model cards' download under their words.
        let tokens = crate::theme::themes::ledger::tokens();
        assert_eq!(tokens.meter_h_thin, 2.0);
        assert!(tokens.meter_h_thin < tokens.meter_h);
    }
}
