//! A spark: a small line with no axes, in a stat's tile under its figures or as a chart of its own.
//! It is as wide as what holds it and a fixed height, and its line keeps its width however it is
//! stretched (CHART-1).

use argus_desktop_core::views::blocks::chart::spark::{points, summary, H, W};
use argus_protocol::Tone;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{canvas, div, prelude::*, App, Bounds, Pixels, Role, SharedString, Window};

use crate::kit::ink::{stroke, Frame};
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::chip::tone_colour;

/// The spark of `values` in `tone`, `height` tall. `key` names it for a test.
pub fn spark(values: &[f64], tone: Tone, height: f32, key: &str, tokens: &Tokens) -> impl IntoElement {
    let (line, colour, width) = (points(values), tone_colour(tone, tokens), tokens.chart_stroke);
    let line = canvas(
        |_, _, _| (),
        move |area: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
            // Stretched to the box on both axes: one unit across is a hundredth of its width.
            let frame = Frame::new(area.origin, f32::from(area.size.width) / W, window.rem_size());
            let stretch = f32::from(area.size.height) / H / (f32::from(area.size.width) / W);
            let pts: Vec<(f32, f32)> = line.iter().map(|(x, y)| (*x, y * stretch)).collect();
            stroke(window, &frame, &pts, frame.fixed(width), colour, &[]);
        },
    )
    .size_full();
    div().id(SharedString::from(format!("{key}-spark"))).role(Role::Image).test_support().aria_label(summary(values)).w_full().h(rem(height)).child(line)
}
