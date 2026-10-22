//! A flow's sequence diagram painted from core's layout (BOARD-SEQ-2 to -4): frames under the lifelines,
//! the lifelines and their heads, then the arrows with their dots, and over them the guide a drag shows.
//!
//! The page drew it as SVG in its own units and scaled the SVG to its box; the strokes here are in the
//! same units and scale with the drawing, as an SVG's do. The geometry is `board::lenses::picture`'s;
//! this is only which colour and dash each part has: calls solid and replies dashed, the lit step in the
//! accent and strong, the one under the pointer lifted to the text colour.

use argus_desktop_core::board::lenses::picture::{self, Arrow, Line, Picture, HEAD, MARGIN, LOOP_BACK, LOOP_DROP, LOOP_OUT};
use gpui_kit::{App, Pixels, Window};

use crate::kit::ink::{boxed, circle, polygon, stroke, text, Anchor, Face, Frame};
use crate::theme::Tokens;

/// What a drag shows of where it would land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Guide {
    /// A horizontal line at this y: where the arrow would stand.
    Step(f32),
    /// A vertical line at this x: where the lifeline would stand.
    Lifeline(f32),
}

/// The mono face the diagram is set in, at `size` units.
fn face<'a>(tokens: &'a Tokens, frame: &Frame, size: f32, weight: f32, colour: gpui_kit::Hsla) -> Face<'a> {
    Face { family: tokens.font_mono, size: frame.len(size), weight, italic: false, colour }
}

fn dashes(frame: &Frame, on: f32, off: f32) -> [Pixels; 2] {
    [frame.len(on), frame.len(off)]
}

/// The arrowhead at `tip` coming from `dir` (`1` right, `-1` left): filled for a call, two open strokes
/// for a reply.
fn arrowhead(window: &mut Window, frame: &Frame, tokens: &Tokens, tip: (f32, f32), dir: f32, reply: bool, ink: (gpui_kit::Hsla, f32)) {
    let (colour, width) = ink;
    let back = tip.0 - dir * tokens.vd_head;
    let half = tokens.fd_head_half;
    if reply {
        stroke(window, frame, &[(back, tip.1 - half), tip, (back, tip.1 + half)], frame.len(width), colour, &[]);
    } else {
        polygon(window, frame, &[(back, tip.1 - half), tip, (back, tip.1 + half)], colour);
    }
}

fn arrow(window: &mut Window, cx: &mut App, frame: &Frame, tokens: &Tokens, arrow: &Arrow, hovered: bool) {
    let colour = if arrow.on {
        tokens.accent
    } else if hovered {
        tokens.fg
    } else {
        tokens.fg_muted
    };
    let width = if arrow.on { tokens.fd_on_stroke } else { tokens.vd_msg_stroke };
    let dash = if arrow.reply { dashes(frame, tokens.vd_reply_on, tokens.vd_reply_off).to_vec() } else { Vec::new() };
    match arrow.line {
        Line::Across { from, to, y } => {
            let dir = (to - from).signum();
            stroke(window, frame, &[(from, y), (to - dir * 2.0, y)], frame.len(width), colour, &dash);
            arrowhead(window, frame, tokens, (to, y), dir, arrow.reply, (colour, width));
        }
        Line::Loop { x, y } => {
            stroke(window, frame, &[(x, y), (x + LOOP_OUT, y), (x + LOOP_OUT, y + LOOP_DROP), (x + LOOP_BACK, y + LOOP_DROP)], frame.len(width), colour, &dash);
            arrowhead(window, frame, tokens, (x + LOOP_BACK, y + LOOP_DROP), -1.0, arrow.reply, (colour, width));
        }
    }
    let (weight, words) = if arrow.on { (tokens.weight_semibold, tokens.fg) } else { (tokens.weight_regular, tokens.fg_muted) };
    let anchor = if arrow.centred { Anchor::Middle } else { Anchor::Start };
    text(window, cx, &arrow.text, frame.at(arrow.text_at), anchor, face(tokens, frame, tokens.vd_font, weight, words));
    circle(window, frame, arrow.badge, tokens.fd_badge_r, Some(if arrow.on { tokens.accent } else { tokens.fill }), Some((frame.len(tokens.vd_stroke), tokens.line_strong)));
    let digit = if arrow.on { tokens.fg_on_accent } else { tokens.fg_faint };
    text(window, cx, &arrow.step.to_string(), frame.at((arrow.badge.0, arrow.badge.1 + 3.5)), Anchor::Middle, face(tokens, frame, tokens.vd_num_font, tokens.weight_regular, digit));
}

/// Paints the diagram into `frame`, one unit being `frame.unit` pixels. `hovered` is the step the pointer
/// is over, `guide` the drop line of a drag.
pub fn paint(window: &mut Window, cx: &mut App, frame: &Frame, picture: &Picture, hovered: Option<usize>, guide: Option<Guide>, tokens: &Tokens) {
    let thin = frame.len(tokens.vd_stroke);
    for f in &picture.frames {
        boxed(window, frame, (f.left, f.top, f.right - f.left, f.bottom - f.top), 4.0, None, Some((thin, tokens.line_strong)));
        let tab = picture::tab(f.left, f.top);
        polygon(window, frame, &tab, tokens.fill);
        stroke(window, frame, &tab, thin, tokens.line_strong, &[]);
        text(window, cx, &f.kind, frame.at((f.left + picture::TAB_KIND.0, f.top + picture::TAB_KIND.1)), Anchor::Start, face(tokens, frame, tokens.vd_font, tokens.weight_bold, tokens.fg));
        text(window, cx, &format!("[{}]", f.label), frame.at((f.left + picture::TAB_LABEL.0, f.top + picture::TAB_LABEL.1)), Anchor::Start, face(tokens, frame, tokens.vd_font, tokens.weight_regular, tokens.fg_faint));
        for &y in &f.arms {
            stroke(window, frame, &[(f.left, y), (f.right, y)], thin, tokens.line_strong, &dashes(frame, tokens.fd_farm_dash, tokens.fd_farm_dash));
        }
    }
    for lifeline in &picture.heads {
        stroke(window, frame, &[(lifeline.x, MARGIN + HEAD), (lifeline.x, picture.height - MARGIN)], thin, tokens.line_strong, &dashes(frame, tokens.fd_life_on, tokens.fd_life_off));
        let width = picture::head_width();
        boxed(window, frame, (lifeline.x - width / 2.0, MARGIN, width, HEAD), tokens.fd_box_radius, Some(tokens.bg_surface), Some((thin, tokens.line_strong)));
        text(window, cx, &lifeline.label, frame.at((lifeline.x, MARGIN + HEAD / 2.0 + 4.0)), Anchor::Middle, face(tokens, frame, tokens.vd_font, tokens.weight_semibold, tokens.fg));
    }
    for arm in &picture.arms {
        text(window, cx, &arm.label, frame.at(arm.at), Anchor::Start, face(tokens, frame, tokens.vd_font, tokens.weight_regular, tokens.fg_faint));
    }
    for a in &picture.arrows {
        arrow(window, cx, frame, tokens, a, hovered == Some(a.step));
    }
    match guide {
        Some(Guide::Step(y)) => stroke(window, frame, &[(MARGIN, y), (picture.width - MARGIN, y)], frame.len(tokens.fd_on_stroke), tokens.accent, &dashes(frame, tokens.fd_guide_on, tokens.fd_guide_off)),
        Some(Guide::Lifeline(x)) => stroke(window, frame, &[(x, MARGIN), (x, picture.height - MARGIN)], frame.len(tokens.fd_on_stroke), tokens.accent, &dashes(frame, tokens.fd_guide_on, tokens.fd_guide_off)),
        None => {}
    }
}
