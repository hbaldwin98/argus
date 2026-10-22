//! What is drawn of a clash while the sync holds one: the person's clashing text marked in the
//! danger colour, with a wavy underline, wherever typing has carried it (`.cm-clash`), and a strip
//! under the bar that says what the agent wrote at each (`.cm-theirs`, EDITOR-CLASH-3).
//!
//! The page puts what the agent wrote inline after each clash. The kit's editor has no inline
//! widget, and text put in the buffer would be the person's to type over, so the words stand in a
//! strip of their own, one line for each clash and the line it is on.

use gpui_kit::component::input::{RopeExt as _, TextDecoration};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, px, AnyElement, Context, HighlightStyle, SharedString, TestSupportExt as _, UnderlineStyle};

use super::session::Key;
use super::CodeEditor;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How much of the danger colour a clash is filled with (`--danger` at 16%) and its underline
/// drawn in (70%).
const FILL: f32 = 0.16;
const UNDERLINE: f32 = 0.70;
/// How much of the blue what the agent wrote is washed with (`--blue` at 18%).
const WASH: f32 = 0.18;

impl CodeEditor {
    /// Marks the clashes the sync holds for `key`, and takes the marks away once none is held.
    pub(super) fn clash(&mut self, key: &Key, cx: &mut Context<Self>) {
        let tokens = *Tokens::global(cx);
        let clashes = self.lives.get(key.0, &key.1).and_then(|live| live.sync()).map(|sync| sync.clashes()).unwrap_or_default();
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(state) = session.state.clone() else { return };
        let marks = session.clashes.get_or_insert_with(|| state.update(cx, |state, cx| state.create_decorations_collection(Vec::new(), cx))).clone();
        let text = state.read(cx).text().clone();
        let style = HighlightStyle {
            background_color: Some(tokens.danger.opacity(FILL)),
            underline: Some(UnderlineStyle { thickness: px(1.), color: Some(tokens.danger.opacity(UNDERLINE)), wavy: true }),
            ..HighlightStyle::default()
        };
        session.theirs = clashes.iter().map(|clash| (text.offset_to_point(clash.from.min(text.len())).row + 1, clash.theirs.clone())).collect();
        marks.set(clashes.iter().map(|clash| TextDecoration::new(clash.from..clash.to, style)).collect(), cx);
        cx.notify();
    }

    /// The strip of what the agent wrote at each clash, or nothing while none is held.
    pub(super) fn theirs(&self, tokens: &Tokens) -> Option<AnyElement> {
        let theirs = &self.shown.as_ref().and_then(|key| self.sessions.get(key))?.theirs;
        if theirs.is_empty() {
            return None;
        }
        Some(
            v_flex()
                .id("editor-theirs")
                .test_support()
                .flex_shrink_0()
                .w_full()
                .px(rem(tokens.sp_12))
                .py(rem(tokens.sp_4))
                .gap(rem(tokens.sp_2))
                .border_b_1()
                .border_color(tokens.line)
                .typed(tokens.type_small)
                .text_size(rem(tokens.fs_xs))
                .children(theirs.iter().take(MOST).enumerate().map(|(at, (line, text))| {
                    let said: SharedString = if text.is_empty() { "removed".into() } else { first_line(text).into() };
                    h_flex()
                        .id(("theirs", at))
                        .test_support()
                        .aria_label(SharedString::from(format!("Line {line}, the agent wrote {said}")))
                        .gap(rem(tokens.sp_8))
                        .child(div().text_color(tokens.fg_faint).child(SharedString::from(format!("Line {line}, the agent wrote"))))
                        .child(div().italic().bg(tokens.blue.opacity(WASH)).px(rem(tokens.sp_4)).rounded(rem(tokens.r_sm)).text_color(tokens.fg).child(said))
                }))
                .into_any_element(),
        )
    }
}

/// The most clashes the strip lists; a file with more has them marked in the text.
const MOST: usize = 4;

/// What the agent wrote, cut to its first line and a length a strip holds.
fn first_line(text: &str) -> String {
    const LONGEST: usize = 120;
    let line = text.lines().next().unwrap_or("");
    let more = text.lines().nth(1).is_some() || line.chars().count() > LONGEST;
    let mut said: String = line.chars().take(LONGEST).collect();
    if more {
        said.push('\u{2026}');
    }
    said
}
