//! A sequence diagram painted from core's layout: lifelines, boxes at the head and foot, stick
//! figures for actors, arrows with their heads, numbered dots, self-call loops and branch bands.
//!
//! One element serves every place a diagram is drawn (the diagram block, and later the Feature
//! stage's Flows lens). It paints in the diagram's own units scaled to the box it is given, and owns
//! no state: what is drawn is what core laid out (`views::blocks::diagram`), in the theme's colours.

use std::rc::Rc;

use argus_desktop_core::views::blocks::diagram::{Drawn, Item, ACTOR_AT, BRANCH_BAND_H, DOT_R, HEAD_H, SELF_DROP, SELF_W};
use gpui_kit::{canvas, div, prelude::*, App, Bounds, Hsla, Pixels, SharedString, Window};

use super::tip::tip;
use super::ink::{boxed, circle, head, stroke, text, Anchor, Face, Frame, Pt};
use crate::theme::scale::rem;
use crate::theme::Tokens;

// Where the page's SVG puts each part, in the diagram's units. A band is inset from the edges and
// washed to half the fill; its words sit on a baseline in the tab; a message's text rides above its
// line and a self-call's beside its loop; a step's dot sits behind its tail, with the digit dropped
// to centre it; a head is `markerWidth` long, an open one four fifths of that, both a fifth wide,
// and a filled one stands a tenth past the line's end.
const BAND_INSET: f32 = 4.0;
const BAND_FILL: f32 = 0.5;
const WORD_X: f32 = 12.0;
const WORD_BASE: f32 = 15.0;
const LABEL_RISE: f32 = 7.0;
const SELF_END: f32 = 2.0;
const SELF_LABEL_GAP: f32 = 8.0;
const SELF_LABEL_DROP: f32 = 4.0;
const DOT_BACK: f32 = 12.0;
const NAME_DROP: f32 = 4.0;
const DIGIT_DROP: f32 = 3.5;
const OPEN_LEN: f32 = 0.8;
const HEAD_HALF: f32 = 0.4;
const HEAD_OVER: f32 = 0.1;
// An actor's stick figure: a head, a neck down to the hips, arms across, and legs to the feet.
const FIG_HEAD_AT: f32 = 5.0;
const FIG_HEAD_R: f32 = 4.0;
const FIG_NECK: f32 = 9.0;
const FIG_HIPS: f32 = 18.0;
const FIG_ARMS: f32 = 12.0;
const FIG_ARM: f32 = 6.0;
const FIG_FOOT: f32 = 25.0;
const FIG_STEP: f32 = 5.0;

/// The diagram, `scale` times its laid-out size.
pub fn sequence(drawn: Rc<Drawn>, scale: f32, tokens: &Tokens) -> impl IntoElement {
    let tokens = *tokens;
    let (w, h) = (drawn.width * scale, drawn.height * scale);
    // A label that was cut says the rest on hover: a box over where it is drawn, as the page's
    // `<title>` on the text (DIAGRAM-3).
    let hovers: Vec<_> = drawn
        .clipped
        .iter()
        .enumerate()
        .map(|(i, c)| {
            div()
                .id(SharedString::from(format!("seq-clipped-{i}")))
                .absolute()
                .left(rem(c.x * scale))
                .top(rem(c.y * scale))
                .w(rem(c.w * scale))
                .h(rem(c.h * scale))
                .tooltip(tip(c.full.clone()))
        })
        .collect();
    let paint = canvas(
        |_, _, _| (),
        move |area: Bounds<Pixels>, _, window: &mut Window, cx: &mut App| {
            let unit = f32::from(area.size.width) / drawn.width.max(1.0);
            let frame = Frame::new(area.origin, unit, window.rem_size());
            paint(&drawn, &frame, &tokens, window, cx);
        },
    )
    .w(rem(w))
    .h(rem(h));
    div().relative().w(rem(w)).h(rem(h)).child(paint).children(hovers)
}

fn paint(d: &Drawn, frame: &Frame, tokens: &Tokens, window: &mut Window, cx: &mut App) {
    let ui = |weight: f32, size: f32, colour: Hsla, italic: bool| Face { family: tokens.font_ui, size: frame.len(size), weight, italic, colour };
    let name = ui(tokens.weight_semibold, tokens.vd_font, tokens.fg, false);
    let label = ui(tokens.weight_regular, tokens.vd_font, tokens.fg, false);
    let number = ui(tokens.weight_semibold, tokens.vd_num_font, tokens.fg, false);
    let thin = frame.len(tokens.vd_stroke);
    let msg = frame.len(tokens.vd_msg_stroke);
    let dash = |on: f32, off: f32| [frame.len(on), frame.len(off)];

    // Lifelines, under everything.
    for c in &d.cols {
        stroke(window, frame, &[(c.x, d.top + HEAD_H), (c.x, d.bottom)], thin, tokens.line_strong, &dash(tokens.vd_life_dash, tokens.vd_life_dash));
    }
    // Bands sit under the arrows but over the lifelines.
    for item in &d.items {
        match item {
            Item::Branch { y, tab_w, word, when } => {
                boxed(window, frame, (BAND_INSET, *y, d.width - BAND_INSET * 2.0, BRANCH_BAND_H), tokens.vd_branch_radius, Some(tokens.fill.opacity(BAND_FILL)), Some((thin, tokens.line)));
                boxed(window, frame, (BAND_INSET, *y, *tab_w, BRANCH_BAND_H), tokens.vd_branch_radius, Some(tokens.bg_chrome), Some((thin, tokens.line)));
                let baseline = y + WORD_BASE;
                text(window, cx, word, frame.at((WORD_X, baseline)), Anchor::Start, ui(tokens.weight_semibold, tokens.vd_font, tokens.fg_faint, false));
                text(window, cx, when, frame.at((tab_w + WORD_X, baseline)), Anchor::Start, ui(tokens.weight_regular, tokens.vd_font, tokens.fg_muted, true));
            }
            Item::Arrow { y, from_x, to_x, mid_x, number: n, text: words, reply } => {
                let dashes = if *reply { dash(tokens.vd_reply_on, tokens.vd_reply_off).to_vec() } else { Vec::new() };
                stroke(window, frame, &[(*from_x, *y), (*to_x, *y)], msg, tokens.fg_muted, &dashes);
                arrowhead(window, frame, (*to_x, *y), (*from_x, *y), *reply, tokens);
                text(window, cx, words, frame.at((*mid_x, y - LABEL_RISE)), Anchor::Middle, label);
                numbered(window, cx, frame, (*from_x, *y), *n, tokens, number);
            }
            Item::SelfCall { y, x, number: n, text: words, reply } => {
                let path = [(*x, *y), (x + SELF_W, *y), (x + SELF_W, y + SELF_DROP), (x + SELF_END, y + SELF_DROP)];
                let dashes = if *reply { dash(tokens.vd_reply_on, tokens.vd_reply_off).to_vec() } else { Vec::new() };
                stroke(window, frame, &path, msg, tokens.fg_muted, &dashes);
                arrowhead(window, frame, path[3], path[2], *reply, tokens);
                text(window, cx, words, frame.at((x + SELF_W + SELF_LABEL_GAP, y + SELF_LABEL_DROP)), Anchor::Start, label);
                numbered(window, cx, frame, (x - DOT_BACK, *y), *n, tokens, number);
            }
        }
    }
    for c in &d.cols {
        for y in [d.top, d.bottom] {
            boxed(window, frame, (c.x - c.box_w / 2.0, y, c.box_w, HEAD_H), tokens.vd_box_radius, Some(tokens.bg_surface), Some((thin, tokens.line_strong)));
            text(window, cx, &c.label, frame.at((c.x, y + HEAD_H / 2.0 + NAME_DROP)), Anchor::Middle, name);
        }
        if c.actor {
            person(window, frame, c.x, tokens);
        }
    }
}

/// A message's head: filled for a request, two open strokes for a reply.
fn arrowhead(window: &mut Window, frame: &Frame, tip: Pt, from: Pt, reply: bool, tokens: &Tokens) {
    let len = tokens.vd_head;
    if reply {
        // The page's open marker, `M1 1L9 5L1 9` in a box ten across and `markerWidth` wide, its
        // tip on the line's end.
        let [tip, a, b] = head(tip, from, len * OPEN_LEN, len * HEAD_HALF);
        stroke(window, frame, &[a, tip, b], frame.len(tokens.vd_msg_stroke), tokens.fg_muted, &[]);
    } else {
        // The filled one, `M0 1L10 5L0 9z`, whose tip stands a tenth past the line's end.
        let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
        let norm = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
        let over = (tip.0 + dx / norm * len * HEAD_OVER, tip.1 + dy / norm * len * HEAD_OVER);
        super::ink::polygon(window, frame, &head(over, from, len, len * HEAD_HALF), tokens.fg_muted);
    }
}

/// A step's number in a dot at its tail.
fn numbered(window: &mut Window, cx: &mut App, frame: &Frame, at: Pt, n: usize, tokens: &Tokens, face: Face) {
    circle(window, frame, at, DOT_R, Some(tokens.fill), Some((frame.len(tokens.vd_stroke), tokens.line_strong)));
    text(window, cx, &n.to_string(), frame.at((at.0, at.1 + DIGIT_DROP)), Anchor::Middle, face);
}

/// An actor's stick figure over its box: a head and a body with arms and legs.
fn person(window: &mut Window, frame: &Frame, cx_: f32, tokens: &Tokens) {
    let y = ACTOR_AT;
    let width = frame.len(tokens.vd_actor_stroke);
    circle(window, frame, (cx_, y + FIG_HEAD_AT), FIG_HEAD_R, None, Some((width, tokens.fg_muted)));
    stroke(window, frame, &[(cx_, y + FIG_NECK), (cx_, y + FIG_HIPS)], width, tokens.fg_muted, &[]);
    stroke(window, frame, &[(cx_ - FIG_ARM, y + FIG_ARMS), (cx_ + FIG_ARM, y + FIG_ARMS)], width, tokens.fg_muted, &[]);
    stroke(window, frame, &[(cx_ - FIG_STEP, y + FIG_FOOT), (cx_, y + FIG_HIPS), (cx_ + FIG_STEP, y + FIG_FOOT)], width, tokens.fg_muted, &[]);
}

