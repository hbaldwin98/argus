//! Where a sequence diagram's lifelines, arrows and branch bands go, from the participants and rows
//! Rust already read out of the Mermaid source (DIAGRAM-3, DIAGRAM-4).
//!
//! Nothing here draws: a window paints what it is handed, in the diagram's own units. Lifelines are
//! spaced until the longest message between two neighbours fits, an arrow is a straight line between
//! two lifelines with its text above it, and a participant never declared but named by a message gets
//! a lifeline of its own.

use std::collections::HashMap;

use super::label::{clip, width};
use crate::connection::page::{FlowLine, PageDiagram};

/// The most characters of a label drawn; the rest is cut with an ellipsis.
pub const MAX_LABEL: usize = 80;
pub const PAD: f32 = 20.0;
pub const HEAD_H: f32 = 30.0;
/// The room a stick figure takes over its box, and the margin kept over everything.
const ACTOR_H: f32 = 28.0;
/// Where an actor's stick figure starts.
pub const ACTOR_AT: f32 = 2.0;
const MARGIN: f32 = 4.0;
const STEP_H: f32 = 44.0;
const SELF_H: f32 = 52.0;
const BRANCH_H: f32 = 32.0;
pub const BRANCH_BAND_H: f32 = 22.0;
/// How far a self-call reaches right of its lifeline, and how deep its loop is.
pub const SELF_W: f32 = 32.0;
pub const SELF_DROP: f32 = 18.0;
/// The room between a box's text and its edge, and its narrowest.
const BOX_ROOM: f32 = 24.0;
const BOX_MIN: f32 = 90.0;
/// The room an arrow's text needs besides its own width.
const TEXT_ROOM: f32 = 44.0;
/// How far a gap is kept past the two boxes it lies between.
const GAP_ROOM: f32 = 16.0;
/// Where a numbered dot sits from its lifeline, and its radius.
pub const DOT_OFF: f32 = 9.0;
pub const DOT_R: f32 = 8.0;
const FIRST_ROW: f32 = 12.0;
const FOOT: f32 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub id: String,
    /// What is written in its box, held to [`MAX_LABEL`].
    pub label: String,
    pub actor: bool,
    pub box_w: f32,
    /// Where its lifeline is.
    pub x: f32,
}

/// A label cut at [`MAX_LABEL`], where it is drawn in the diagram's units, and the whole of what
/// it said: a viewer hovers the rectangle to read it (DIAGRAM-3; the page's `<title>`).
#[derive(Debug, Clone, PartialEq)]
pub struct Clipped {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub full: String,
}

// A line of the diagram's text stands about this tall, its baseline this far down it.
const TEXT_H: f32 = 14.0;
const TEXT_ASCENT: f32 = 11.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A band across the diagram with a tab at its left naming the branch.
    Branch { y: f32, tab_w: f32, word: String, when: String },
    /// A message between two lifelines, from `from_x` (just off the sender's lifeline, under its
    /// numbered dot) to `to_x`; its text is centred at `mid_x`, halfway between the lifelines.
    Arrow { y: f32, from_x: f32, to_x: f32, mid_x: f32, number: usize, text: String, reply: bool },
    /// A message a participant sends itself: a loop out of its lifeline and back.
    SelfCall { y: f32, x: f32, number: usize, text: String, reply: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub cols: Vec<Column>,
    pub width: f32,
    pub height: f32,
    /// Where the boxes at the top start, and the ones at the foot.
    pub top: f32,
    pub bottom: f32,
    pub items: Vec<Item>,
    /// What a screen reader hears.
    pub summary: String,
    /// Every label that was cut, to be hovered for its whole text.
    pub clipped: Vec<Clipped>,
}

/// A box wide enough for `label`.
fn box_w(label: &str) -> f32 {
    BOX_MIN.max(width(label, MAX_LABEL) + BOX_ROOM)
}

struct Step<'a> {
    from: usize,
    to: Option<usize>,
    text: &'a str,
}

/// `Sequence diagram · 4 steps`, which the figure opened larger is titled.
pub fn title(d: &PageDiagram) -> String {
    let steps = d.rows.iter().filter(|row| matches!(row, FlowLine::Step { .. })).count();
    format!("Sequence diagram \u{b7} {steps} step{}", if steps == 1 { "" } else { "s" })
}

/// The diagram for `d`, or none when there is nothing to draw: no participant, or no row.
pub fn layout(d: &PageDiagram) -> Option<Drawn> {
    if d.participants.is_empty() || d.rows.is_empty() {
        return None;
    }
    let mut fulls: Vec<String> = d.participants.iter().map(|p| p.label.clone()).collect();
    let mut cols: Vec<Column> =
        d.participants.iter().map(|p| Column { id: p.id.clone(), label: clip(&p.label, MAX_LABEL), actor: p.actor, box_w: box_w(&p.label), x: 0.0 }).collect();
    let mut index: HashMap<String, usize> = cols.iter().enumerate().map(|(i, c)| (c.id.clone(), i)).collect();
    // A message may name someone the parse never declared; give them a lifeline.
    let mut lookup = |id: &str, cols: &mut Vec<Column>, fulls: &mut Vec<String>| -> usize {
        *index.entry(id.to_owned()).or_insert_with(|| {
            fulls.push(id.to_owned());
            cols.push(Column { id: id.to_owned(), label: clip(id, MAX_LABEL), actor: false, box_w: box_w(id), x: 0.0 });
            cols.len() - 1
        })
    };
    let mut steps: Vec<Step> = Vec::new();
    for row in &d.rows {
        if let FlowLine::Step { from, to, text, .. } = row {
            let from = lookup(from, &mut cols, &mut fulls);
            let to = to.as_deref().map(|to| lookup(to, &mut cols, &mut fulls));
            steps.push(Step { from, to, text });
        }
    }

    // The gaps between neighbouring lifelines, widened until the longest message between them fits.
    let mut gaps: Vec<f32> = cols.windows(2).map(|pair| (pair[0].box_w + pair[1].box_w) / 2.0 + GAP_ROOM).collect();
    for s in &steps {
        let need = width(s.text, MAX_LABEL) + TEXT_ROOM;
        match s.to {
            None => {
                if let Some(gap) = gaps.get_mut(s.from) {
                    *gap = gap.max(need + SELF_W);
                }
            }
            Some(to) => {
                let (lo, hi) = (s.from.min(to), s.from.max(to));
                for gap in gaps.iter_mut().take(hi).skip(lo) {
                    *gap = gap.max(need / (hi - lo) as f32);
                }
            }
        }
    }
    let mut x = PAD + cols[0].box_w / 2.0;
    for (i, c) in cols.iter_mut().enumerate() {
        c.x = x;
        x += gaps.get(i).copied().unwrap_or(0.0);
    }
    let last = cols.len() - 1;
    let mut w = cols[last].x + cols[last].box_w / 2.0 + PAD;
    for s in &steps {
        // The last lifeline's self-call label spills past its box.
        if s.to.is_none() && s.from == last {
            w = w.max(cols[last].x + SELF_W + 8.0 + width(s.text, MAX_LABEL) + PAD);
        }
    }
    for row in &d.rows {
        if let FlowLine::Branch { word, when } = row {
            w = w.max(width(word, MAX_LABEL) + width(when, MAX_LABEL) + 2.0 * PAD + 24.0);
        }
    }

    let top = if cols.iter().any(|c| c.actor) { ACTOR_H + MARGIN } else { MARGIN };
    let mut y = top + HEAD_H + FIRST_ROW;
    let mut items = Vec::new();
    let mut clipped = Vec::new();
    let mut cut = |text: &str, x: f32, baseline: f32, anchor: f32| {
        if text.chars().count() > MAX_LABEL {
            let w = width(text, MAX_LABEL);
            clipped.push(Clipped { x: x - w * anchor, y: baseline - TEXT_ASCENT, w, h: TEXT_H, full: text.to_owned() });
        }
    };
    let mut taken = 0;
    for row in &d.rows {
        match row {
            FlowLine::Branch { word, when } => {
                cut(word, 12.0, y + 15.0, 0.0);
                cut(when, width(word, MAX_LABEL) + 16.0 + 12.0, y + 15.0, 0.0);
                items.push(Item::Branch { y, tab_w: width(word, MAX_LABEL) + 16.0, word: clip(word, MAX_LABEL), when: clip(when, MAX_LABEL) });
                y += BRANCH_H;
            }
            FlowLine::Step { number, reply, text, .. } => {
                let step = &steps[taken];
                taken += 1;
                match step.to {
                    None => {
                        cut(text, cols[step.from].x + SELF_W + 8.0, y + 14.0 + 4.0, 0.0);
                        items.push(Item::SelfCall { y: y + 14.0, x: cols[step.from].x, number: *number, text: clip(text, MAX_LABEL), reply: *reply });
                        y += SELF_H;
                    }
                    // A step from a participant to itself that is not a self-call is skipped.
                    Some(to) if to == step.from => y += STEP_H,
                    Some(to) => {
                        let (a, b) = (cols[step.from].x, cols[to].x);
                        let dir = if b > a { 1.0 } else { -1.0 };
                        cut(text, (a + b) / 2.0, y + 28.0 - 7.0, 0.5);
                        items.push(Item::Arrow { y: y + 28.0, from_x: a + dir * DOT_OFF, to_x: b, mid_x: (a + b) / 2.0, number: *number, text: clip(text, MAX_LABEL), reply: *reply });
                        y += STEP_H;
                    }
                }
            }
        }
    }
    let bottom = y + FOOT;
    for (c, full) in cols.iter().zip(&fulls) {
        for at in [top, bottom] {
            if full.chars().count() > MAX_LABEL {
                clipped.push(Clipped { x: c.x - c.box_w / 2.0, y: at, w: c.box_w, h: HEAD_H, full: full.clone() });
            }
        }
    }
    let height = bottom + HEAD_H + FOOT;
    let n = d.rows.iter().filter(|row| matches!(row, FlowLine::Step { .. })).count();
    let summary = format!("Sequence diagram: {n} step{} between {}", if n == 1 { "" } else { "s" }, cols.iter().map(|c| c.label.as_str()).collect::<Vec<_>>().join(", "));
    Some(Drawn { cols, width: w, height, top, bottom, items, summary, clipped })
}

#[cfg(test)]
mod tests;
