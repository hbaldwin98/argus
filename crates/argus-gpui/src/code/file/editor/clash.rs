//! What is drawn of a clash while the sync holds one: the person's clashing text marked in the
//! danger colour, with a wavy underline, wherever typing has carried it (`.cm-clash`), and after
//! each clash what the agent wrote there, in the line itself (`.cm-theirs`, EDITOR-CLASH-3).
//!
//! The page puts the agent's words in the line as a widget that moves the text after it along. The
//! kit's editor has no inline widget, and text put in the buffer would be the person's to type
//! over, so the words are a box laid over the editor at the end of the clash, on the place the
//! editor last drew that offset. It is opaque, so it covers what follows the clash on its line
//! rather than pushing it right. The window is redrawn whenever the editor's state changes, so
//! scrolling and typing move it with no watch of its own (`the_agents_words_follow_the_editor_when_it_scrolls`).

use gpui_kit::component::input::{RopeExt as _, TextDecoration};
use gpui_kit::{anchored, deferred, div, point, prelude::*, px, AnyElement, Context, HighlightStyle, SharedString, TestSupportExt as _, UnderlineStyle, Window};

use super::session::Key;
use super::CodeEditor;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// How much of the danger colour a clash is filled with (`--danger` at 16%) and its underline
/// drawn in (70%).
const FILL: f32 = 0.16;
const UNDERLINE: f32 = 0.70;
/// How much of the blue what the agent wrote is washed with (`--blue` at 18%).
const WASH: f32 = 0.18;
/// What the widget says when it is hovered (`ts:code/editor.ts`).
const TIP: &str = "What the agent wrote here";

impl CodeEditor {
    /// Marks the clashes the sync holds for `key`, and takes the marks away once none is held.
    pub(super) fn clash(&mut self, key: &Key, cx: &mut Context<Self>) {
        let tokens = *Tokens::global(cx);
        let clashes = self.lives.get(key.0, &key.1).and_then(|live| live.sync()).map(|sync| sync.clashes()).unwrap_or_default();
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(state) = session.state.clone() else { return };
        let marks = session.clashes.get_or_insert_with(|| state.update(cx, |state, cx| state.create_decorations_collection(Vec::new(), cx))).clone();
        let style = HighlightStyle {
            background_color: Some(tokens.danger.opacity(FILL)),
            underline: Some(UnderlineStyle { thickness: px(1.), color: Some(tokens.danger.opacity(UNDERLINE)), wavy: true }),
            ..HighlightStyle::default()
        };
        session.theirs = clashes.clone();
        marks.set(clashes.iter().map(|clash| TextDecoration::new(clash.from..clash.to, style)).collect(), cx);
        cx.notify();
    }

    /// What the agent wrote at each clash, as boxes over the editor at the end of each clash, or
    /// nothing while none is held or the clash is out of view.
    pub(super) fn theirs(&self, window: &Window, cx: &gpui_kit::App, tokens: &Tokens) -> Vec<AnyElement> {
        let Some(session) = self.shown.as_ref().and_then(|key| self.sessions.get(key)) else { return Vec::new() };
        let Some(state) = session.state.as_ref() else { return Vec::new() };
        let state = state.read(cx);
        let (Some(height), text) = (state.line_height(), state.text().clone()) else { return Vec::new() };
        let ground = tokens.bg_sunken.blend(tokens.blue.opacity(WASH));
        let gap = pixels(tokens.sp_6, window.rem_size());
        session
            .theirs
            .iter()
            .enumerate()
            .filter_map(|(at, clash)| {
                let theirs = &clash.theirs;
                let end = clash.to.min(text.len());
                let place = state.range_to_bounds(&(end..end))?;
                let line = text.offset_to_point(end).row + 1;
                let said: SharedString = if theirs.is_empty() { "removed".into() } else { first_line(theirs).into() };
                let widget = div()
                    .id(("theirs", at))
                    .test_support()
                    .aria_label(SharedString::from(format!("Line {line}, the agent wrote {said}")))
                    .tooltip(tip(TIP))
                    .h(height)
                    .flex()
                    .items_center()
                    .px(rem(tokens.sp_4))
                    .rounded(rem(tokens.r_xs))
                    .bg(ground)
                    .italic()
                    .whitespace_nowrap()
                    .font_family(SharedString::from(tokens.font_mono))
                    .typed(tokens.type_small)
                    .text_size(rem(tokens.fs_md))
                    .text_color(tokens.fg_muted)
                    .child(said);
                Some(deferred(anchored().position(point(place.origin.x + gap, place.origin.y)).child(widget)).into_any_element())
            })
            .collect()
    }
}

/// What the agent wrote, cut to its first line and a length a line holds.
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
