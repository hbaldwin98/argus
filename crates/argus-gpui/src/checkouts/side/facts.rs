//! The facts list of the side column: Upstream, Changes, Feature and Last commit, a label in the
//! faint colour and what it says in the muted one (`.cs-facts`). The strings are built in Rust.

use argus_desktop_core::checkouts::side::{Fact, Value};
use argus_desktop_core::checkouts::table::LastCommit;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, font, prelude::*, FontWeight, Hsla, SharedString, StyledText, TestSupportExt as _, TextRun};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// One stretch of a value's text and the face and colour it is set in.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub text: String,
    pub mono: bool,
    pub colour: Hsla,
}

/// A commit as a line says it: the short hash in faint mono, the summary, and how long ago.
fn commit_pieces(commit: &LastCommit, tokens: &Tokens) -> Vec<Piece> {
    vec![
        Piece { text: commit.short.clone(), mono: true, colour: tokens.fg_faint },
        Piece { text: format!(" {}", commit.summary), mono: false, colour: tokens.fg_muted },
        Piece { text: format!(" \u{b7} {}", commit.ago), mono: false, colour: tokens.fg_faint },
    ]
}

/// `#42` in the accent while the Feature stage has it under its cursor, then the title.
fn feature_pieces(number: i64, viewed: bool, title: Option<&str>, tokens: &Tokens) -> Vec<Piece> {
    let mut pieces = vec![Piece { text: format!("#{number}"), mono: true, colour: if viewed { tokens.accent } else { tokens.fg_muted } }];
    if let Some(title) = title {
        pieces.push(Piece { text: format!(" {title}"), mono: false, colour: tokens.fg_muted });
    }
    pieces
}

/// The pieces as one wrapped run of text, so its words flow and break as the page's inline text does
/// (`.cs-facts dd`), not as boxes with a gap between them.
fn run(pieces: &[Piece], tokens: &Tokens) -> StyledText {
    let text: String = pieces.iter().map(|piece| piece.text.as_str()).collect();
    let runs = pieces
        .iter()
        .map(|piece| {
            let mut face = font(SharedString::from(if piece.mono { tokens.font_mono } else { tokens.font_ui }));
            face.weight = FontWeight(400.0);
            TextRun { len: piece.text.len(), font: face, color: piece.colour, background_color: None, underline: None, strikethrough: None }
        })
        .collect();
    StyledText::new(SharedString::from(text)).with_runs(runs)
}

/// A commit as a line says it.
pub fn commit(commit: &LastCommit, tokens: &Tokens) -> impl IntoElement {
    run(&commit_pieces(commit, tokens), tokens)
}

fn value(value: &Value, tokens: &Tokens) -> gpui_kit::AnyElement {
    match value {
        Value::Text(text) => div().child(SharedString::from(text.clone())).into_any_element(),
        Value::Feature { number, viewed, title } => run(&feature_pieces(*number, *viewed, title.as_deref(), tokens), tokens).into_any_element(),
        Value::Commit(last) => commit(last, tokens).into_any_element(),
    }
}

pub fn facts(facts: &[Fact], tokens: &Tokens) -> impl IntoElement {
    v_flex().id("side-facts").test_support().gap(rem(tokens.sp_12)).text_size(rem(tokens.fs_sm)).children(facts.iter().map(|fact| {
        h_flex()
            .id(fact.label)
            .test_support()
            .aria_label(SharedString::from(fact.label))
            .items_start()
            .gap(rem(tokens.sp_12))
            .child(div().w(rem(tokens.ck_facts_label_w)).flex_shrink_0().text_color(tokens.fg_faint).child(fact.label))
            .child(div().flex_1().min_w_0().text_color(tokens.fg_muted).child(value(&fact.value, tokens)))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::themes::ledger;

    #[test]
    fn a_feature_reads_as_one_run_with_its_number_in_mono_and_lit_when_viewed() {
        let tokens = ledger::tokens();
        let lit = feature_pieces(42, true, Some("Screenshot harness"), &tokens);
        assert_eq!(lit.iter().map(|piece| piece.text.as_str()).collect::<String>(), "#42 Screenshot harness");
        assert!(lit[0].mono && lit[0].colour == tokens.accent && !lit[1].mono);
        assert_eq!(feature_pieces(42, false, None, &tokens), vec![Piece { text: "#42".into(), mono: true, colour: tokens.fg_muted }]);
    }

    #[test]
    fn a_commit_reads_as_hash_summary_and_age_in_one_run() {
        let tokens = ledger::tokens();
        let last = LastCommit { short: "abc1234".into(), summary: "Fix the thing".into(), ago: "3h".into() };
        let pieces = commit_pieces(&last, &tokens);
        assert_eq!(pieces.iter().map(|piece| piece.text.as_str()).collect::<String>(), "abc1234 Fix the thing \u{b7} 3h");
        assert!(pieces[0].mono && pieces[0].colour == tokens.fg_faint && pieces[2].colour == tokens.fg_faint);
    }
}
