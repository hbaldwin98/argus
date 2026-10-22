//! Where everything of a line, bar or area chart goes, in a box 480 by 160: the grid and its ticks,
//! the thinned labels under it, and each series as bars, or runs of line with their dots (CHART-2
//! to CHART-6).
//!
//! Nothing here draws. A window scales the box to its panel and paints what it is told; the page
//! does the same with an SVG's `viewBox`.

use argus_protocol::{ChartShape, Series, Tone};

use super::scale::{axis, extent};
use super::words::{compact, hover};

pub const W: f32 = 480.0;
pub const H: f32 = 160.0;
const LEFT: f32 = 44.0;
const RIGHT: f32 = 10.0;
const TOP: f32 = 10.0;
const BOTTOM: f32 = 24.0;
/// Where a tick's label sits left of the grid, and how far its baseline is under the line.
pub const TICK_GAP: f32 = 6.0;
pub const TICK_DROP: f32 = 3.5;
/// How far under the plot the x labels' baseline is.
const LABEL_DROP: f32 = 14.0;
/// Where the x labels' baseline is.
pub const LABEL_BASELINE: f32 = H - BOTTOM + LABEL_DROP;
/// What one character of a label is taken to be wide, and the room kept between two labels.
const LABEL_CHAR: f32 = 6.0;
const LABEL_ROOM: f32 = 10.0;
/// How much of a band the bars of its points share, and the gap kept between neighbours.
const GROUP: f32 = 0.7;
const BAR_GAP: f32 = 0.5;

/// Series of one tone cannot be told apart by hue, so they differ in stroke: the nth to share a tone
/// takes this dash (on, off, on, off...; empty is solid) and this share of its tone as a fill.
const DASHES: [&[f32]; 4] = [&[], &[6.0, 3.0], &[2.0, 3.0], &[8.0, 3.0, 2.0, 3.0]];
const FILLS: [f32; 4] = [1.0, 0.6, 0.35, 0.8];
/// How faint an area is under its line, as a share of its series' fill.
const AREA: f32 = 0.18;
/// A dot's radius.
pub const DOT: f32 = 3.0;
/// The legend's key, a line in its series' dash across a box this big, inset at either end.
pub const KEY_W: f32 = 22.0;
pub const KEY_H: f32 = 8.0;
pub const KEY_INSET: f32 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub y: f32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct XLabel {
    pub x: f32,
    pub text: String,
}

/// A bar or a dot: where it is and what hovering it reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub x: f32,
    pub y: f32,
    /// A bar's size; a dot has none.
    pub w: f32,
    pub h: f32,
    pub tip: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub name: String,
    pub tone: Tone,
    /// The stroke's dashes, empty for solid.
    pub dash: &'static [f32],
    /// How much of the tone fills a bar, and (times 0.18) an area.
    pub fill: f32,
    /// Runs of consecutive present values, each of at least two points; a gap breaks the line.
    pub runs: Vec<Vec<(f32, f32)>>,
    /// Whether each run is closed down to the baseline and filled.
    pub area: bool,
    pub dots: Vec<Mark>,
    pub bars: Vec<Mark>,
}

impl Drawn {
    /// The share of its tone an area under the line is filled with.
    pub fn area_fill(&self) -> f32 {
        AREA * self.fill
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub left: f32,
    pub right: f32,
    pub ticks: Vec<Tick>,
    pub labels: Vec<XLabel>,
    pub series: Vec<Drawn>,
    /// Where a value of zero is, which areas close to and bars grow from.
    pub baseline: f32,
}

/// How many series before the one at `i` share its tone.
pub fn twin_index(series: &[Series], i: usize) -> usize {
    series[..i].iter().filter(|s| s.tone == series[i].tone).count()
}

/// The dash the nth series of a tone takes.
pub fn dash_of(twin: usize) -> &'static [f32] {
    DASHES[twin % DASHES.len()]
}

fn present(v: Option<f64>) -> Option<f64> {
    v.filter(|v| v.is_finite())
}

/// The runs of consecutive present points, so a gap breaks the line; a run of one is a dot alone.
fn runs(points: &[Option<(f32, f32)>]) -> Vec<Vec<(f32, f32)>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    for p in points {
        match p {
            Some(p) => cur.push(*p),
            None => {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// The chart of `shape` (line, bar or area) over the labels `x`.
pub fn layout(shape: ChartShape, x: &[String], series: &[Series], unit: Option<&str>) -> Chart {
    let n = x.len();
    let all: Vec<f64> = series.iter().flat_map(|s| s.values.iter().take(n).filter_map(|v| present(*v))).collect();
    let (min, max) = extent(&all, 0.0);
    let scale = axis(min, max);
    let plot_w = W - LEFT - RIGHT;
    let plot_h = H - TOP - BOTTOM;
    let bars = shape == ChartShape::Bar;
    // Bars sit in bands; a line runs edge to edge, or centred if it is one point.
    let band_x = |i: usize| {
        if bars || n < 2 {
            LEFT + plot_w * (i as f32 + 0.5) / n.max(1) as f32
        } else {
            LEFT + plot_w * i as f32 / (n - 1) as f32
        }
    };
    let y = |v: f64| TOP + plot_h * (1.0 - ((v - scale.lo) / (scale.hi - scale.lo)) as f32);

    let ticks = scale.ticks.iter().map(|t| Tick { y: y(*t), label: compact(*t) }).collect();

    let longest = x.iter().map(|label| label.chars().count()).fold(1, usize::max);
    let slot = plot_w / n.max(1) as f32;
    let every = (((longest as f32 * LABEL_CHAR + LABEL_ROOM) / slot).ceil() as usize).max(1);
    let labels = x.iter().enumerate().filter(|(i, _)| i % every == 0).map(|(i, text)| XLabel { x: band_x(i), text: text.clone() }).collect();

    let group_w = plot_w / n.max(1) as f32 * GROUP;
    let bar_w = group_w / series.len().max(1) as f32;
    let base = y(0.0);
    let drawn = series
        .iter()
        .enumerate()
        .map(|(si, s)| {
            let twin = twin_index(series, si);
            let values: Vec<Option<f64>> = s.values.iter().take(n).map(|v| present(*v)).collect();
            let tip = |i: usize, v: f64| hover(x.get(i).map_or("", String::as_str), v, unit);
            let mut mark = Drawn {
                name: s.name.clone(),
                tone: s.tone,
                dash: dash_of(twin),
                fill: FILLS[twin % FILLS.len()],
                runs: Vec::new(),
                area: shape == ChartShape::Area,
                dots: Vec::new(),
                bars: Vec::new(),
            };
            if bars {
                for (i, v) in values.iter().enumerate() {
                    let Some(v) = v else { continue };
                    let top = y(*v).min(base);
                    mark.bars.push(Mark {
                        x: band_x(i) - group_w / 2.0 + bar_w * si as f32 + BAR_GAP,
                        y: top,
                        w: (bar_w - 1.0).max(1.0),
                        h: (y(*v) - base).abs().max(1.0),
                        tip: tip(i, *v),
                    });
                }
            } else {
                let points: Vec<Option<(f32, f32)>> = values.iter().enumerate().map(|(i, v)| v.map(|v| (band_x(i), y(v)))).collect();
                mark.runs = runs(&points);
                for (i, v) in values.iter().enumerate() {
                    if let Some(v) = v {
                        mark.dots.push(Mark { x: band_x(i), y: y(*v), w: 0.0, h: 0.0, tip: tip(i, *v) });
                    }
                }
            }
            mark
        })
        .collect();
    Chart { left: LEFT, right: W - RIGHT, ticks, labels, series: drawn, baseline: base }
}

#[cfg(test)]
mod tests;
