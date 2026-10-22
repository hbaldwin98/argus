//! The small pieces a feature's row, its card and its peek share: the tasks cell, the questions
//! cell, the agent's pill and the numbers a feature comes after (`tasks`, `questions`, `agentLine`
//! and `after` of `feature_stage.ts`).
//!
//! What each cell says is `index_words`'; this gives it the page's look in the token space.

use argus_desktop_core::board::index_words::{self, Tasks, Tone};
use argus_desktop_core::connection::page::{Agent, FeatureRow};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, Hsla, SharedString};

use crate::kit::pill::{status_pill, status_word};
use crate::kit::progress::{bar, percent};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The colour a word of `tone` takes: attention for what is open, ok for what is settled.
pub fn tone_colour(tone: Tone, plain: Hsla, tokens: &Tokens) -> Hsla {
    match tone {
        Tone::Plain => plain,
        Tone::Attention => tokens.attention,
        Tone::Ok => tokens.ok,
    }
}

/// `.meta`: the small muted type of the page's secondary words.
pub fn meta(words: impl Into<SharedString>, tokens: &Tokens) -> gpui_kit::Div {
    div().typed(tokens.type_small).text_color(tokens.fg_muted).whitespace_nowrap().child(words.into())
}

/// A progress bar and its `done/total`, as a task count is said in a row and on a card.
pub fn progress(slug: &str, done: usize, total: usize, tokens: &Tokens) -> impl IntoElement {
    let share = percent(done as u32, total as u32).unwrap_or(0);
    h_flex()
        .items_center()
        .gap(rem(tokens.sp_4))
        .child(bar(SharedString::from(format!("progress-{slug}")), share, format!("{done} of {total} tasks"), tokens))
        .child(meta(format!("{done}/{total}"), tokens))
}

/// The tasks column of a list row: `new`, `no tasks`, or the bar; the first two faint.
pub fn tasks_cell(row: &FeatureRow, tokens: &Tokens) -> AnyElement {
    match index_words::tasks(row) {
        Tasks::New => meta("new", tokens).text_color(tokens.fg_faint).into_any_element(),
        Tasks::None => meta("no tasks", tokens).text_color(tokens.fg_faint).into_any_element(),
        Tasks::Progress { done, total } => progress(&row.slug, done, total, tokens).into_any_element(),
    }
}

/// The questions column: `N open` or `N decided`, in plain muted type. The page's spans carry
/// `fg-attention`/`fg-ok`, but `.meta` follows in style.css at the same specificity, so what the
/// page draws is muted; this copies what is drawn, not what the TS meant.
pub fn questions_cell(row: &FeatureRow, tokens: &Tokens) -> Option<AnyElement> {
    let (words, _) = index_words::questions(row)?;
    Some(meta(words, tokens).into_any_element())
}

/// The agent's pill with its status word, as the rail says it.
pub fn agent_pill(slug: &str, agent: &Agent, tokens: &Tokens) -> impl IntoElement {
    status_pill(SharedString::from(format!("agent-{slug}")), status_word(agent.status).to_owned(), agent.status, tokens)
}

/// `#3, #5` in the mono face.
pub fn after(row: &FeatureRow, tokens: &Tokens) -> gpui_kit::Div {
    div().font_family(SharedString::from(tokens.font_mono)).child(index_words::after(row))
}
