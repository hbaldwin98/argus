//! What the note reads as (BOARD-CAPTURE-11): the verdict, then in a cell the title and brief, the
//! tasks and the decisions as the daemon's own grammar read them, and a line it could not place named.
//!
//! The reading is Rust's (`CapturePage`), sent whole on every change; this only draws it, so the
//! preview is what Create stores.

use argus_desktop_core::board::capture_note::{CapturePage, NoteDecision, NoteRow};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, SharedString, TestSupportExt as _};

use crate::kit::caption::{aside, label, meta};
use crate::kit::chips::{cell, ktag};
use crate::kit::pill::status_pill;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the label's far end says: that it is being made, why it cannot be, or that it can be.
pub fn verdict(page: &CapturePage, tokens: &Tokens) -> impl IntoElement {
    let (words, tone) = if page.creating {
        ("creating\u{2026}".to_owned(), None)
    } else if let Some(blocked) = &page.blocked {
        (blocked.clone(), Some(tokens.attention))
    } else {
        ("ready to create".to_owned(), Some(tokens.ok))
    };
    div().id("capture-verdict").test_support().aria_label(SharedString::from(words.clone())).child(aside(words, tone, tokens))
}

/// `Line 3 reads as nothing.` or `Lines 3, 5 read as nothing.`, and how a line starts.
pub fn unread_words(unread: &[usize]) -> Option<String> {
    let first = unread.first()?;
    let said = if unread.len() == 1 {
        format!("Line {first} reads as nothing.")
    } else {
        format!("Lines {} read as nothing.", unread.iter().map(usize::to_string).collect::<Vec<_>>().join(", "))
    };
    Some(format!("{said} Start a task with -, a question with ?, a decision made with !."))
}

/// `.nrow`: a mark, a label, the words and what ends the row, 6px apart on a row `--row-h-sm` tall.
fn nrow(tokens: &Tokens) -> gpui_kit::Div {
    h_flex().min_h(rem(tokens.row_h_sm)).items_center().gap(rem(tokens.sp_6))
}

/// `.n`: a row's locator, in mono and faint, a column of its own.
fn locator(words: impl Into<SharedString>, width: f32, colour: gpui_kit::Hsla, tokens: &Tokens) -> gpui_kit::Div {
    div().flex_shrink_0().w(rem(width)).font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_sm)).text_color(colour).child(words.into())
}

fn task(row: &NoteRow, tokens: &Tokens) -> AnyElement {
    nrow(tokens)
        .pl(rem(row.depth as f32 * tokens.row_indent))
        .child(div().flex_shrink_0().w(rem(tokens.nrow_mark_w)).text_color(tokens.fg_faint).child("\u{25cb}"))
        .child(locator(row.label.clone(), tokens.nrow_label_w, tokens.fg_faint, tokens))
        .child(div().flex_1().min_w_0().when(row.depth > 0, |text| text.text_color(tokens.fg_muted)).child(SharedString::from(row.text.clone())))
        .into_any_element()
}

fn decision(at: usize, decision: &NoteDecision, tokens: &Tokens) -> AnyElement {
    let (mark, tone) = if decision.open { ("?", tokens.attention) } else { ("\u{2713}", tokens.ok) };
    v_flex()
        .id(("capture-decision", at))
        .test_support()
        .aria_label(SharedString::from(decision.label.clone()))
        .child(
            nrow(tokens)
                .child(div().flex_shrink_0().w(rem(tokens.nrow_mark_w)).text_color(tone).child(mark))
                // The page classes it `n fg-attention` or `n fg-ref`, but `.n` is written after both, so
                // its faint grey wins and the label is no more coloured than a task's.
                .child(locator(decision.label.clone(), tokens.nrow_label_w, tokens.fg_faint, tokens))
                .child(div().flex_1().min_w_0().child(SharedString::from(decision.text.clone())))
                .child(if decision.open {
                    status_pill(("capture-state", at), "open", "waiting", tokens).into_any_element()
                } else {
                    status_pill(("capture-state", at), "made", "done", tokens).into_any_element()
                }),
        )
        .when(!decision.options.is_empty(), |rows| {
            rows.child(h_flex().flex_wrap().gap(rem(tokens.sp_4)).pl(rem(tokens.sp_32)).children(decision.options.iter().map(|option| ktag(SharedString::from(option.clone()), tokens))))
        })
        .when_some(decision.because.clone(), |rows, because| rows.child(meta(tokens).pl(rem(tokens.sp_32)).child(SharedString::from(format!("because {because}")))))
        .into_any_element()
}

/// `.capture-reads`: the section named by its label, the verdict at its end, and the cell under it.
pub fn reads(page: &CapturePage, tokens: &Tokens) -> impl IntoElement {
    let open = page.decisions.iter().filter(|d| d.open).count();
    let decided = page.decisions.len() - open;
    let headed = |id: &'static str, words: &'static str, end: AnyElement| h_flex().id(id).test_support().aria_label(words).items_center().gap(rem(tokens.sp_8)).child(label(words, tokens)).child(div().flex_1()).child(end);
    let tasks = v_flex()
        .gap(rem(tokens.sp_2))
        .child(headed("capture-tasks", "Tasks", aside(format!("0/{}", page.tasks.len()), None, tokens).into_any_element()))
        .when(page.tasks.is_empty(), |rows| rows.child(meta(tokens).child("A line starting with - is a task.")))
        .children(page.tasks.iter().map(|row| task(row, tokens)));
    let decisions = (!page.decisions.is_empty()).then(|| {
        let tally = if open > 0 { aside(format!("{open} open"), Some(tokens.attention), tokens) } else { aside(format!("{decided} decided"), Some(tokens.ok), tokens) };
        v_flex()
            .gap(rem(tokens.sp_2))
            .child(headed("capture-decisions", "Decisions", tally.into_any_element()))
            .children(page.decisions.iter().enumerate().map(|(at, d)| decision(at, d, tokens)))
    });
    v_flex()
        .id("capture-reads")
        .test_support()
        .aria_label("What the note reads as")
        .flex_shrink_0()
        .w(gpui_kit::relative(tokens.capture_reads_share))
        .min_w(rem(tokens.capture_reads_min))
        .max_w(rem(tokens.capture_reads_max))
        .h_full()
        .overflow_y_scroll()
        .gap(rem(tokens.sp_12))
        .child(
            h_flex()
                .items_center()
                .gap(rem(tokens.sp_8))
                .child(label("Reads as", tokens))
                .child(div().flex_1())
                .child(verdict(page, tokens)),
        )
        .child(
            cell(tokens)
                .flex()
                .flex_col()
                .gap(rem(tokens.sp_12))
                .children(unread_words(&page.unread).map(|words| div().id("capture-unread").test_support().aria_label(SharedString::from(words.clone())).text_color(tokens.danger).child(SharedString::from(words))))
                .child(
                    v_flex()
                        .gap(rem(tokens.sp_4))
                        .child(match page.title.is_empty() {
                            false => div().typed(tokens.type_heading).child(SharedString::from(page.title.clone())).into_any_element(),
                            true => div().text_color(tokens.fg_faint).child("The first line is the title.").into_any_element(),
                        })
                        .children(page.brief.clone().map(|brief| div().text_color(tokens.fg_muted).max_w(rem(tokens.prose_max_w)).child(SharedString::from(brief)))),
                )
                .child(tasks)
                .children(decisions),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_is_named_in_the_singular_and_several_in_a_list() {
        assert_eq!(unread_words(&[]), None);
        assert_eq!(unread_words(&[4]).as_deref(), Some("Line 4 reads as nothing. Start a task with -, a question with ?, a decision made with !."));
        assert_eq!(unread_words(&[3, 5]).as_deref(), Some("Lines 3, 5 read as nothing. Start a task with -, a question with ?, a decision made with !."));
    }
}
