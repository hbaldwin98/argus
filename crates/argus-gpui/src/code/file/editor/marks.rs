//! What is drawn over the text: the change marks in the gutter, and an agent's write lit for a
//! moment. Both are range decorations, so they move with what is typed (`ts:code/editor.ts`
//! `changeMarks` mapped through each change) until Rust reads the file again and sends new marks.

use std::time::Duration;

use argus_desktop_core::code::pace::LIT_MS;
use argus_desktop_core::code::steps::{Kind, Mark};
use gpui_kit::component::input::{RangeDecoration, RangeDecorationStyle, RopeExt as _};
use gpui_kit::Context;

use super::session::{Key, Marks};
use super::CodeEditor;
use crate::theme::Tokens;

/// How much of the blue an agent's write is lit with at first (`--blue` at 35%).
const LIT_SHARE: f32 = 0.35;

impl CodeEditor {
    /// Draws `marks`, the lines Rust last read the file as changing, unless they are the ones
    /// already drawn. A mark past the document's end is dropped. A line is its text and its
    /// break, since a decoration with no text is dropped by the editor.
    pub(super) fn mark(&mut self, key: &Key, cx: &mut Context<Self>) {
        let tokens = *Tokens::global(cx);
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(state) = session.state.clone() else { return };
        let marks = session.wanted.clone();
        let said = format!("{}:{}:{}", tokens.name, marks.len(), marks.iter().map(|m| format!("{}{}", m.line, if m.kind == Kind::Added { '+' } else { '-' })).collect::<Vec<_>>().join(","));
        let drawn = session.marks.get_or_insert_with(|| {
            state.update(cx, |state, cx| Marks {
                added: state.create_range_decorations_collection(Vec::new(), cx),
                removed: state.create_range_decorations_collection(Vec::new(), cx),
                lit: state.create_range_decorations_collection(Vec::new(), cx),
                said: String::new(),
            })
        });
        if drawn.said == said {
            return;
        }
        drawn.said = said;
        let text = state.read(cx).text().clone();
        let lines = text.lines_len();
        let line = |mark: &Mark| {
            let at = mark.line.checked_sub(1).filter(|at| *at < lines)?;
            let from = text.line_start_offset(at);
            Some(from..(text.line_end_offset(at) + 1).min(text.len()))
        };
        let of = |kind: Kind, style: RangeDecorationStyle, colour| {
            marks.iter().filter(|mark| mark.kind == kind).filter_map(line).map(|range| RangeDecoration::new(range).with_style(style).with_color(colour)).collect::<Vec<_>>()
        };
        drawn.added.set(of(Kind::Added, RangeDecorationStyle::GutterBar, tokens.ok), cx);
        drawn.removed.set(of(Kind::Removed, RangeDecorationStyle::GutterTick, tokens.danger), cx);
    }

    /// Lights the places an agent's write put text, and lets them go after `LIT_MS`. A write that
    /// only deleted lights nothing; a second write before the first's time is up is lit with it
    /// and let go with it.
    pub(super) fn light(&mut self, key: &Key, ranges: Vec<(usize, usize)>, cx: &mut Context<Self>) {
        let tokens = *Tokens::global(cx);
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(lit) = session.marks.as_ref().map(|marks| marks.lit.clone()) else { return };
        let wash = tokens.blue.opacity(LIT_SHARE);
        lit.set(ranges.into_iter().filter(|(from, to)| from < to).map(|(from, to)| RangeDecoration::new(from..to).with_style(RangeDecorationStyle::Fill).with_color(wash)).collect(), cx);
        let key = key.clone();
        session.unlight = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(LIT_MS)).await;
            this.update(cx, |this, cx| {
                if let Some(marks) = this.sessions.get_mut(&key).and_then(|s| s.marks.as_ref()) {
                    marks.lit.clear(cx);
                }
            })
            .ok();
        }));
    }
}
