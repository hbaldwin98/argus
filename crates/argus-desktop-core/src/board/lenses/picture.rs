//! Where each part of a flow's sequence diagram sits, and how it is scaled and picked: the page's
//! `drawSequence` and its fit, zoom and drag arithmetic (`sequence.ts`, `flows.ts`) as pure functions
//! a window paints from and a test can hold.
//!
//! `sequence` reads the source into lifelines and items; this puts them on a plane in the diagram's
//! own units, which the window scales to its box. Nothing here draws.

use crate::connection::page::{Sequence, SequenceItem};

/// The diagram's plane: its margin, how far apart the lifelines stand, how tall a lifeline's head is
/// and how far a call and a call to itself advance.
pub const MARGIN: f32 = 28.0;
pub const COLUMN: f32 = 176.0;
pub const HEAD: f32 = 34.0;
pub const STEP: f32 = 46.0;
pub const SELF_STEP: f32 = 60.0;
/// The first arrow's distance under a head.
const FIRST: f32 = 30.0;
/// What a frame's opening, a new arm and a closing advance, and where their lines sit against them.
const OPEN_BY: f32 = 22.0;
const ARM_BY: f32 = 20.0;
const CLOSE_BY: f32 = 6.0;
const ARM_RISE: f32 = 18.0;
const CLOSE_RISE: f32 = 16.0;
/// A frame is inset from the margin by this, and by this more for each frame it is inside.
const INSET: f32 = 6.0;
const NEST: f32 = 8.0;
/// A call to itself loops out this far, drops this far and writes its text this far from its lifeline.
pub const LOOP_OUT: f32 = 40.0;
pub const LOOP_DROP: f32 = 22.0;
pub const LOOP_BACK: f32 = 4.0;
const SELF_TEXT: f32 = 48.0;
/// A lifeline to itself on the last column writes its text past it, so the frame is this much wider.
const LOOP_ROOM: f32 = 120.0;
/// One character of the 12px mono face the page measures a self-call's text in, and the room left after it.
const CHAR: f32 = 7.2;
const REACH_ROOM: f32 = 8.0;
/// A lifeline's head is a column less this wide.
const HEAD_LESS: f32 = 28.0;

/// The inline diagram is never drawn smaller than this nor larger than its own size.
pub const INLINE_LEAST: f32 = 0.7;
/// What the box holding the inline diagram keeps for its own padding and border.
pub const INLINE_PADDING: f32 = 24.0;
/// The expanded diagram's zoom bounds and the step a press changes it by.
pub const ZOOM_LEAST: f32 = 0.25;
pub const ZOOM_MOST: f32 = 3.0;
pub const ZOOM_BY: f32 = 0.1;
/// What fit leaves of the canvas, and the most it enlarges.
const FIT_PADDING: f32 = 32.0;
const FIT_MOST: f32 = 2.0;
/// How far a press must move before it is a drag and not a click.
pub const DRAG_AT: f32 = 5.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub x: f32,
    pub label: String,
}

/// An arrow between two lifelines, or a loop on one.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Across { from: f32, to: f32, y: f32 },
    /// Out from the lifeline at `x`, down and back to it, starting at `y`.
    Loop { x: f32, y: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arrow {
    pub step: usize,
    pub reply: bool,
    pub on: bool,
    pub line: Line,
    /// Where the text stands, and whether it is centred on `at` (an arrow across) or starts there.
    pub text: String,
    pub text_at: (f32, f32),
    pub centred: bool,
    /// The numbered dot at its tail.
    pub badge: (f32, f32),
    /// `Step N: from → to, text`, for a reader that cannot see it.
    pub label: String,
    /// The y a drag measures this arrow by.
    pub y: f32,
}

/// A frame (`alt`, `opt`, `loop`...) round its steps: its box, its tab, and where its arms divide it.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub kind: String,
    pub label: String,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub arms: Vec<f32>,
}

/// The words of an arm, `[else]`, where it begins.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmText {
    pub label: String,
    pub at: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: f32,
    pub height: f32,
    pub heads: Vec<Head>,
    pub frames: Vec<Frame>,
    pub arms: Vec<ArmText>,
    pub arrows: Vec<Arrow>,
}

/// The x of lifeline `at`.
pub fn x_of(at: usize) -> f32 {
    MARGIN + COLUMN / 2.0 + at as f32 * COLUMN
}

/// How wide a lifeline's head is.
pub fn head_width() -> f32 {
    COLUMN - HEAD_LESS
}

pub fn layout(sequence: &Sequence) -> Picture {
    let count = sequence.lifelines.len().max(1);
    let looped = sequence.items.iter().any(|item| matches!(item, SequenceItem::Message { from, to, .. } if from == to && *from == count - 1));
    // A self-call's text starts 48 right of its lifeline; the frame must reach past its end.
    let reach = sequence.items.iter().fold(0.0_f32, |most, item| match item {
        SequenceItem::Message { from, to, text, .. } if from == to => most.max(MARGIN + COLUMN / 2.0 + *from as f32 * COLUMN + SELF_TEXT + text.chars().count() as f32 * CHAR),
        _ => most,
    });
    let width = (MARGIN * 2.0 + count as f32 * COLUMN + if looped { LOOP_ROOM } else { 0.0 }).max(reach.ceil() + MARGIN + REACH_ROOM);
    let name = |at: usize| sequence.lifelines.get(at).map_or("", |l| l.label.as_str());

    let (mut arrows, mut frames, mut arms) = (Vec::new(), Vec::new(), Vec::new());
    let mut open: Vec<(String, String, f32, Vec<f32>)> = Vec::new();
    let mut y = MARGIN + HEAD + FIRST;
    for item in &sequence.items {
        match item {
            SequenceItem::Message { step, from, to, reply, text, on } => {
                let (here, there) = (x_of(*from), x_of(*to));
                let label = format!("Step {step}: {} \u{2192} {}, {text}", name(*from), name(*to));
                if from == to {
                    arrows.push(Arrow {
                        step: *step,
                        reply: *reply,
                        on: *on,
                        line: Line::Loop { x: here, y },
                        text: text.clone(),
                        text_at: (here + SELF_TEXT, y + 15.0),
                        centred: false,
                        badge: (here, y),
                        label,
                        y: y + LOOP_DROP / 2.0,
                    });
                    y += SELF_STEP;
                } else {
                    arrows.push(Arrow {
                        step: *step,
                        reply: *reply,
                        on: *on,
                        line: Line::Across { from: here, to: there, y },
                        text: text.clone(),
                        text_at: ((here + there) / 2.0, y - 8.0),
                        centred: true,
                        badge: (here, y),
                        label,
                        y,
                    });
                    y += STEP;
                }
            }
            SequenceItem::Open { frame, label } => {
                open.push((frame.clone(), label.clone(), y - OPEN_BY, Vec::new()));
                y += OPEN_BY;
            }
            SequenceItem::Arm { label } => {
                if let Some(frame) = open.last_mut() {
                    frame.3.push(y - ARM_RISE);
                    arms.push(ArmText { label: format!("[{label}]"), at: (MARGIN + 14.0, y - 4.0) });
                }
                y += ARM_BY;
            }
            SequenceItem::Close => {
                let Some((kind, label, top, dividing)) = open.pop() else { continue };
                let inset = INSET + open.len() as f32 * NEST;
                frames.push(Frame { kind, label, left: MARGIN + inset, right: width - MARGIN - inset, top, bottom: y - CLOSE_RISE, arms: dividing });
                y += CLOSE_BY;
            }
        }
    }
    let heads = sequence.lifelines.iter().enumerate().map(|(at, l)| Head { x: x_of(at), label: l.label.clone() }).collect();
    Picture { width, height: y + MARGIN, heads, frames, arms, arrows }
}

/// The inline diagram's scale: down to 70% when that is what makes the whole width fit the box, never
/// above its own size. `holder` is the box's width; its padding is scrolled inside, so it is taken off.
pub fn inline_scale(holder: f32, width: f32) -> f32 {
    let room = holder - INLINE_PADDING;
    if room <= 0.0 || width <= 0.0 {
        return 1.0;
    }
    INLINE_LEAST.max(1.0_f32.min(room / width))
}

/// The expanded diagram's zoom when fit: the canvas's room over the diagram's, at most twice its size
/// and at least a quarter, to a hundredth.
pub fn fit_zoom(canvas: (f32, f32), diagram: (f32, f32)) -> f32 {
    let fits = ((canvas.0 - FIT_PADDING) / diagram.0).min((canvas.1 - FIT_PADDING) / diagram.1).min(FIT_MOST);
    ZOOM_LEAST.max((fits * 100.0).round() / 100.0)
}

/// Zoom `by` further, to a tenth, held between a quarter and three times.
pub fn zoom_by(zoom: f32, by: f32) -> f32 {
    (((zoom + by) * 10.0).round() / 10.0).clamp(ZOOM_LEAST, ZOOM_MOST)
}

/// The corners of a frame's tab, which hangs from its top left: along under the kind, a slope up to its
/// top, and back along the top.
pub fn tab(left: f32, top: f32) -> [(f32, f32); 5] {
    [(left, top + 18.0), (left + 40.0, top + 18.0), (left + 48.0, top + 10.0), (left + 48.0, top), (left, top)]
}

/// Where a frame writes its kind and its label, from its top left.
pub const TAB_KIND: (f32, f32) = (6.0, 13.0);
pub const TAB_LABEL: (f32, f32) = (56.0, 13.0);

/// How far a scrolled box moves so the part of its content from `from` to `to` (along the scrolled
/// axis) shows, only if it is outside: `offset` is how far the content is scrolled (zero or more, the
/// amount that has gone off the start), `view` how much of it shows, and `margin` what is left past the
/// edge it was brought to. The page's `scrollIntoView({ block: 'nearest' })` and `flows.ts`'s 24px.
pub fn scroll_into(offset: f32, view: f32, from: f32, to: f32, margin: f32) -> f32 {
    if from < offset {
        (from - margin).max(0.0)
    } else if to > offset + view {
        to + margin - view
    } else {
        offset
    }
}

/// The step whose arrow is nearest `y` (in the diagram's units), by number.
pub fn nearest_step(picture: &Picture, y: f32) -> Option<usize> {
    picture.arrows.iter().min_by(|a, b| (a.y - y).abs().total_cmp(&(b.y - y).abs())).map(|arrow| arrow.step)
}

/// The lifeline nearest `x`, by its place from the left.
pub fn nearest_lifeline(picture: &Picture, x: f32) -> Option<usize> {
    picture.heads.iter().enumerate().min_by(|(_, a), (_, b)| (a.x - x).abs().total_cmp(&(b.x - x).abs())).map(|(at, _)| at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::lenses::sequence;
    use crate::fixtures;

    fn seq(source: &str) -> Sequence {
        sequence::read(source, Some(1))
    }

    // `drawSequence` (sequence.ts:51-188): the first arrow stands at 28 + 34 + 30, a call advances 46
    // and a call to itself 60, and the diagram is 28 taller than where it ends.
    #[test]
    fn the_first_arrow_stands_under_the_heads_and_each_advances_by_its_kind() {
        let picture = layout(&seq("sequenceDiagram\n  a->>b: one\n  b->>b: two\n  b-->>a: three"));
        let ys: Vec<f32> = picture.arrows.iter().map(|a| match a.line {
            Line::Across { y, .. } | Line::Loop { y, .. } => y,
        }).collect();
        assert_eq!(ys, [92.0, 92.0 + 46.0, 92.0 + 46.0 + 60.0]);
        assert_eq!(picture.height, 92.0 + 46.0 + 60.0 + 46.0 + 28.0);
        assert_eq!(picture.heads.iter().map(|h| h.x).collect::<Vec<_>>(), [116.0, 292.0]);
    }

    #[test]
    fn the_width_is_the_columns_and_widens_for_a_loop_on_the_last_lifeline_or_a_long_self_call() {
        assert_eq!(layout(&seq("sequenceDiagram\n  a->>b: one")).width, 56.0 + 2.0 * 176.0);
        let looped = layout(&seq("sequenceDiagram\n  a->>b: one\n  b->>b: x"));
        assert_eq!(looped.width, 56.0 + 2.0 * 176.0 + 120.0, "a self-call on the last lifeline writes past it");
        let words = "a very long self call text that runs well past the column";
        let long = layout(&seq(&format!("sequenceDiagram\n  a->>a: {words}\n  a->>b: x")));
        let reach = (116.0_f32 + 48.0 + words.chars().count() as f32 * 7.2).ceil() + 36.0;
        assert_eq!(long.width, reach);
    }

    #[test]
    fn a_straight_arrow_writes_centred_above_its_line_and_a_loop_beside_it() {
        let picture = layout(&seq("sequenceDiagram\n  a->>b: one\n  a->>a: two"));
        assert_eq!(picture.arrows[0].text_at, ((116.0 + 292.0) / 2.0, 92.0 - 8.0));
        assert!(picture.arrows[0].centred && !picture.arrows[1].centred);
        assert_eq!(picture.arrows[1].text_at, (116.0 + 48.0, 138.0 + 15.0));
        assert_eq!(picture.arrows[1].label, "Step 2: a \u{2192} a, two");
        assert!(picture.arrows[0].on && !picture.arrows[1].on);
    }

    // `close` in drawSequence: a frame is inset 6 and 8 more for each frame it sits in; an arm divides it.
    #[test]
    fn frames_nest_inside_one_another_and_an_arm_divides_its_frame() {
        let picture = layout(&seq("sequenceDiagram\n  a->>b: x\n  alt ok\n    b->>a: y\n  else no\n    b->>a: z\n  end"));
        assert_eq!(picture.frames.len(), 1);
        let frame = &picture.frames[0];
        assert_eq!((frame.kind.as_str(), frame.label.as_str()), ("alt", "ok"));
        assert_eq!((frame.left, frame.right), (34.0, picture.width - 34.0));
        assert_eq!(frame.top, 92.0 + 46.0 - 22.0, "opens 22 above where its first step would have stood");
        assert_eq!(frame.arms.len(), 1);
        assert_eq!(picture.arms[0].label, "[no]");
        let nested = layout(&seq("sequenceDiagram\n  alt a\n    opt b\n      a->>b: x\n    end\n  end"));
        assert_eq!(nested.frames.iter().map(|f| f.left).collect::<Vec<_>>(), [34.0 + 8.0, 34.0], "the inner closes first, so it is inset the more");
    }

    #[test]
    fn a_frame_the_source_never_closed_is_closed_at_the_foot() {
        let picture = layout(&seq("sequenceDiagram\n  alt a\n    a->>b: x"));
        assert_eq!(picture.frames.len(), 1);
    }

    // `fitted` (flows.ts:213-241): `max(0.7, min(1, room / width))`, room = holder - 24.
    #[test]
    fn the_inline_diagram_shrinks_to_fit_but_not_below_seventy_percent_nor_above_itself() {
        assert_eq!(inline_scale(1000.0, 400.0), 1.0);
        assert!((inline_scale(324.0, 600.0) - 0.5_f32.max(INLINE_LEAST)).abs() < 1e-6, "300 of 600 is half, held to 70%");
        assert!((inline_scale(444.0, 600.0) - 0.7).abs() < 1e-6);
        assert!((inline_scale(474.0, 600.0) - 0.75).abs() < 1e-6);
        assert_eq!(inline_scale(10.0, 600.0), 1.0, "a box with no room is not measured yet");
    }

    // `drawExpanded` (sequence.ts:275-281): fit is min((w-32)/dw, (h-32)/dh, 2), to 0.01, at least 0.25.
    #[test]
    fn fit_fills_the_canvas_to_twice_the_size_at_most() {
        assert_eq!(fit_zoom((832.0, 632.0), (400.0, 300.0)), 2.0);
        assert_eq!(fit_zoom((532.0, 632.0), (1000.0, 300.0)), 0.5);
        assert_eq!(fit_zoom((100.0, 100.0), (1000.0, 1000.0)), 0.25);
        assert_eq!(fit_zoom((632.0, 3000.0), (900.0, 900.0)), 0.67);
    }

    #[test]
    fn zoom_moves_a_tenth_at_a_time_between_a_quarter_and_three_times() {
        assert_eq!(zoom_by(1.0, ZOOM_BY), 1.1);
        assert_eq!(zoom_by(1.0, -ZOOM_BY), 0.9);
        assert_eq!(zoom_by(0.3, -ZOOM_BY), 0.25);
        assert_eq!(zoom_by(2.95, ZOOM_BY), 3.0);
        assert_eq!(zoom_by(0.67, ZOOM_BY), 0.8, "a fitted zoom lands on the tenths");
    }

    #[test]
    fn a_drag_names_the_arrow_and_the_lifeline_nearest_the_pointer() {
        let picture = layout(&seq("sequenceDiagram\n  a->>b: one\n  b->>c: two\n  c->>a: three"));
        assert_eq!(nearest_step(&picture, 0.0), Some(1));
        assert_eq!(nearest_step(&picture, 140.0), Some(2));
        assert_eq!(nearest_step(&picture, 1000.0), Some(3));
        assert_eq!(nearest_lifeline(&picture, 0.0), Some(0));
        assert_eq!(nearest_lifeline(&picture, 300.0), Some(1));
        assert_eq!(nearest_lifeline(&picture, 1000.0), Some(2));
        assert_eq!(nearest_step(&layout(&seq("sequenceDiagram")), 5.0), None);
    }

    // `scrollIntoView({ block: 'nearest' })` and flows.ts's 24px: nothing moves while it shows.
    #[test]
    fn a_box_scrolls_only_as_far_as_it_must_to_show_what_it_is_asked_to() {
        assert_eq!(scroll_into(100.0, 200.0, 150.0, 250.0, 0.0), 100.0, "inside: stays");
        assert_eq!(scroll_into(100.0, 200.0, 40.0, 80.0, 0.0), 40.0, "before it: its start is brought to the edge");
        assert_eq!(scroll_into(100.0, 200.0, 280.0, 340.0, 0.0), 140.0, "after it: its end is brought to the other");
        assert_eq!(scroll_into(100.0, 200.0, 280.0, 340.0, 24.0), 164.0, "with 24 to spare");
        assert_eq!(scroll_into(100.0, 200.0, 10.0, 30.0, 24.0), 0.0, "never before the start");
    }

    #[test]
    fn a_frames_tab_hangs_from_its_top_left() {
        let tab = tab(34.0, 100.0);
        assert_eq!(tab[0], (34.0, 118.0));
        assert_eq!(tab[2], (82.0, 110.0));
        assert_eq!(tab[4], (34.0, 100.0));
    }

    #[test]
    fn the_fixture_flow_lays_out_with_every_step_numbered_once() {
        let list = fixtures::diagrams("trim");
        let picture = layout(&sequence::read(&list.diagrams[0].body, Some(3)));
        let steps: Vec<usize> = picture.arrows.iter().map(|a| a.step).collect();
        assert!(!steps.is_empty() && steps.windows(2).all(|pair| pair[0] + 1 == pair[1]), "{steps:?}");
        assert_eq!(picture.arrows.iter().filter(|a| a.on).count(), 1);
    }
}
