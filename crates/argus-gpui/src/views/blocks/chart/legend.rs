//! A chart's legend: each series' line, in its dash and tone, and its name, wrapping under the
//! chart. Series of one tone are told apart by dash, so the key shows it (CHART-4, CHART-7).

use argus_desktop_core::views::blocks::chart::layout::{dash_of, twin_index, KEY_H, KEY_INSET, KEY_W};
use argus_protocol::Series;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{canvas, div, prelude::*, AnyElement, App, Bounds, Pixels, SharedString, Window};

use crate::kit::ink::{stroke, Frame};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::chip::tone_colour;

/// `.vchart-legend`.
pub fn legend(series: &[Series], key: &str, tokens: &Tokens) -> AnyElement {
    let dashes: Vec<&'static [f32]> = (0..series.len()).map(|i| dash_of(twin_index(series, i))).collect();
    h_flex()
        .id(SharedString::from(format!("{key}-legend")))
        .test_support()
        .flex_wrap()
        .gap_x(rem(tokens.sp_12))
        .gap_y(rem(tokens.sp_4))
        .text_size(rem(tokens.fs_sm))
        .text_color(tokens.fg_muted)
        .children(series.iter().zip(dashes).map(|(s, dash)| {
            let colour = tone_colour(s.tone, tokens);
            let width = tokens.chart_stroke;
            let swatch = canvas(
                |_, _, _| (),
                move |area: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
                    let frame = Frame::new(area.origin, f32::from(area.size.width) / KEY_W, window.rem_size());
                    let dash: Vec<Pixels> = dash.iter().map(|d| frame.fixed(*d)).collect();
                    let across = [(KEY_INSET, KEY_H / 2.0), (KEY_W - KEY_INSET, KEY_H / 2.0)];
                    stroke(window, &frame, &across, frame.fixed(width), colour, &dash);
                },
            )
            .w(rem(KEY_W))
            .h(rem(KEY_H));
            h_flex().items_center().gap(rem(tokens.sp_4)).child(swatch).child(div().typed(tokens.type_small).child(SharedString::from(s.name.clone())))
        }))
        .into_any_element()
}
