//! A chart block: series over a shared x axis as a line, bars or an area, with its legend when it
//! has more than one series. An agent sends the data and a tone per series, never a colour.
//!
//! Where everything goes is core's (`views::blocks::chart`); `paint` paints the grid, labels and
//! lines, and the bars and dots here are elements so hovering one says what it is. The chart is
//! 480 by 160 units scaled to its box, which stops at 720 wide as the page's does, so its labels
//! keep their size when the panel grows rather than the chart scaling without end (CHART-2).
//!
//! A spark (a small line, no axes) is `spark`'s, and serves a stat's trend as well.

pub mod legend;
pub mod paint;
pub mod spark;

use std::rc::Rc;

use argus_desktop_core::views::blocks::chart::layout::{layout, Chart, DOT, H, W};
use argus_desktop_core::views::blocks::chart::words::summary;
use argus_protocol::{ChartShape, Series};
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::pen::Pen;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::views::chip::tone_colour;
use crate::views::ctx::ViewCtx;

pub fn chart(shape: ChartShape, x: &[String], series: &[Series], unit: Option<&str>, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}-chart", ctx.path);
    if shape == ChartShape::Spark {
        let first = series.first();
        let values: Vec<f64> = first.map(|s| s.values.iter().flatten().copied().collect()).unwrap_or_default();
        let tone = first.map(|s| s.tone).unwrap_or_default();
        return div().w_full().max_w(rem(tokens.spark_max_w)).child(spark::spark(&values, tone, tokens.spark_h, &key, tokens)).into_any_element();
    }
    let drawn = Rc::new(layout(shape, x, series, unit));
    // The box the panel gives it; before it is measured, the chart's own size.
    let room = pen.width(&format!("{key}|box"));
    let across = if room > 0.0 { room.min(tokens.chart_max_w) } else { W };
    let scale = across / W;
    let high = (H * scale).min(tokens.chart_max_h);
    v_flex()
        .id(SharedString::from(key.clone()))
        .role(Role::Image)
        .test_support()
        .aria_label(summary(shape, series, x.len(), unit))
        .relative()
        .gap(rem(tokens.sp_8))
        .child(pen.watch(format!("{key}|box")))
        .child(plot_box(&drawn, &key, scale, (across, high), pen))
        .children((series.len() > 1).then(|| legend::legend(series, &key, tokens)))
        .into_any_element()
}

/// The chart itself at `scale`, `size` big: the painted grid and lines under the bars and dots.
fn plot_box(drawn: &Rc<Chart>, key: &str, scale: f32, size: (f32, f32), pen: &Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let (w, h) = size;
    let unit = |v: f32| rem(v * scale);
    let marks = drawn.series.iter().enumerate().flat_map(|(si, series)| {
        let colour = tone_colour(series.tone, tokens);
        let bars = series.bars.iter().enumerate().map(move |(i, bar)| {
            div()
                .id(SharedString::from(format!("{key}-s{si}-bar{i}")))
                .test_support()
                .aria_label(bar.tip.clone())
                .absolute()
                .left(unit(bar.x))
                .top(unit(bar.y))
                .w(unit(bar.w))
                .h(unit(bar.h))
                .rounded(unit(tokens.r_sm))
                .bg(colour.opacity(series.fill))
                .tooltip(tip(bar.tip.clone()))
                .into_any_element()
        });
        let dots = series.dots.iter().enumerate().map(move |(i, dot)| {
            div()
                .id(SharedString::from(format!("{key}-s{si}-dot{i}")))
                .test_support()
                .aria_label(dot.tip.clone())
                .absolute()
                .left(unit(dot.x - DOT))
                .top(unit(dot.y - DOT))
                .size(unit(DOT * 2.0))
                .rounded_full()
                .bg(colour)
                .tooltip(tip(dot.tip.clone()))
                .into_any_element()
        });
        bars.chain(dots)
    });
    div().relative().w(rem(w)).h(rem(h)).child(paint::plot(drawn.clone(), tokens)).children(marks).into_any_element()
}
