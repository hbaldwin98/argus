//! A spark: a small line with no axes that sits inline, in a stat's tile or as a chart of its own.
//!
//! Its points are in a box 100 wide and 32 high, which the drawing stretches to the tile while the
//! stroke keeps its width (CHART-1).

use super::scale::extent;
use super::words::compact;

/// The box the points are in.
pub const W: f32 = 100.0;
pub const H: f32 = 32.0;
/// Where the lowest and the highest value sit, and where a flat series does.
const BOTTOM: f64 = 28.0;
const RISE: f64 = 24.0;
const MIDDLE: f32 = 16.0;

/// The values that are numbers.
pub fn finite(values: &[f64]) -> Vec<f64> {
    values.iter().copied().filter(|v| v.is_finite()).collect()
}

/// The points of the line through `values`, spread evenly across the width. Fewer than two points
/// draw no line, and a flat series is drawn at mid height.
pub fn points(values: &[f64]) -> Vec<(f32, f32)> {
    let vals = finite(values);
    let Some(first) = vals.first() else { return Vec::new() };
    let (lo, hi) = extent(&vals, *first);
    vals.iter()
        .enumerate()
        .map(|(i, v)| {
            let x = if vals.len() < 2 { W / 2.0 } else { W * i as f32 / (vals.len() - 1) as f32 };
            let y = if hi == lo { MIDDLE } else { (BOTTOM - RISE * (v - lo) / (hi - lo)) as f32 };
            (x, y)
        })
        .collect()
}

/// What a screen reader hears: `trend over 6 points, from 2 to 9`.
pub fn summary(values: &[f64]) -> String {
    let vals = finite(values);
    match vals.first() {
        Some(first) => {
            let (lo, hi) = extent(&vals, *first);
            format!("trend over {} points, from {} to {}", vals.len(), compact(lo), compact(hi))
        }
        None => "no trend".to_owned(),
    }
}

#[cfg(test)]
mod tests;
