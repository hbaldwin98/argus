//! What of a chart is painted and not made of elements: the grid and its labels, and the lines and
//! areas of the series. Its bars and dots are elements, so hovering one reads its value (CHART-6).
//!
//! Every place is core's (`views::blocks::chart::layout`) in the chart's 480 by 160 units, scaled to
//! the box. The grid and the labels scale with it, as the page's SVG does; a line keeps its width,
//! as `vector-effect: non-scaling-stroke` makes the page's (CHART-1).

use std::rc::Rc;

use argus_desktop_core::views::blocks::chart::layout::{Chart, LABEL_BASELINE, TICK_DROP, TICK_GAP, W};
use gpui_kit::{canvas, prelude::*, App, Bounds, Pixels, Window};

use crate::kit::ink::{polygon, stroke, text, Anchor, Face, Frame};
use crate::theme::Tokens;
use crate::views::chip::tone_colour;

/// The canvas that paints `chart`, as big as the element it is made the child of.
pub fn plot(chart: Rc<Chart>, tokens: &Tokens) -> impl IntoElement {
    let tokens = *tokens;
    canvas(
        |_, _, _| (),
        move |area: Bounds<Pixels>, _, window: &mut Window, cx: &mut App| {
            let frame = Frame::new(area.origin, f32::from(area.size.width) / W, window.rem_size());
            paint(&chart, &frame, &tokens, window, cx);
        },
    )
    .absolute()
    .size_full()
}

fn paint(chart: &Chart, frame: &Frame, tokens: &Tokens, window: &mut Window, cx: &mut App) {
    let face = Face { family: tokens.font_ui, size: frame.len(tokens.fs_xs), weight: tokens.weight_regular, italic: false, colour: tokens.fg_faint };
    for tick in &chart.ticks {
        stroke(window, frame, &[(chart.left, tick.y), (chart.right, tick.y)], frame.len(tokens.hairline), tokens.line, &[]);
        text(window, cx, &tick.label, frame.at((chart.left - TICK_GAP, tick.y + TICK_DROP)), Anchor::End, face);
    }
    for label in &chart.labels {
        text(window, cx, &label.text, frame.at((label.x, LABEL_BASELINE)), Anchor::Middle, face);
    }
    for series in &chart.series {
        let colour = tone_colour(series.tone, tokens);
        let dash: Vec<Pixels> = series.dash.iter().map(|d| frame.fixed(*d)).collect();
        for run in &series.runs {
            if series.area {
                let (first, last) = (run[0], run[run.len() - 1]);
                let mut shape = run.clone();
                shape.extend([(last.0, chart.baseline), (first.0, chart.baseline)]);
                polygon(window, frame, &shape, colour.opacity(series.area_fill()));
            }
            stroke(window, frame, run, frame.fixed(tokens.chart_stroke), colour, &dash);
        }
    }
}
