//! What a chart says in words: the compact numbers on its axis, what a screen reader is told it
//! shows, and what hovering a bar or a dot reads.

use argus_protocol::{ChartShape, Series};

use super::scale::extent;
use crate::views::values::grouped;

const UNITS: [(f64, &str); 4] = [(1e3, "K"), (1e6, "M"), (1e9, "B"), (1e12, "T")];

/// A number as `toLocaleString('en-US', { notation: 'compact', maximumFractionDigits: 2 })` writes
/// it: `999`, `1.23K`, `15K`, `2.5M`.
pub fn compact(n: f64) -> String {
    if !n.is_finite() {
        return n.to_string();
    }
    let trimmed = |v: f64| {
        let mut s = format!("{:.2}", v.abs());
        if s.contains('.') {
            s = s.trim_end_matches('0').trim_end_matches('.').to_owned();
        }
        s
    };
    let abs = n.abs();
    let sign = if n < 0.0 && trimmed(n) != "0" { "-" } else { "" };
    // How many units it has outgrown, and its size in the last of them.
    let mut unit = UNITS.iter().take_while(|(size, _)| abs >= *size).count();
    let in_unit = |unit: usize| if unit == 0 { abs } else { abs / UNITS[unit - 1].0 };
    // 999 999 rounds to 1000K, which is said as 1M.
    if unit < UNITS.len() && trimmed(in_unit(unit)).parse::<f64>().unwrap_or(0.0) >= 1000.0 {
        unit += 1;
    }
    let scaled = in_unit(unit);
    let suffix = if unit == 0 { "" } else { UNITS[unit - 1].1 };
    format!("{sign}{}{suffix}", trimmed(scaled))
}

/// The shape as the words say it.
fn shape_word(shape: ChartShape) -> &'static str {
    match shape {
        ChartShape::Line => "line",
        ChartShape::Bar => "bar",
        ChartShape::Area => "area",
        ChartShape::Spark => "spark",
    }
}

/// `bar chart of injected, brief over 6 points, from 0 to 11K tokens`.
pub fn summary(shape: ChartShape, series: &[Series], points: usize, unit: Option<&str>) -> String {
    let names = series.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
    let all: Vec<f64> = series.iter().flat_map(|s| s.values.iter().flatten().copied()).filter(|v| v.is_finite()).collect();
    let unit = unit.filter(|unit| !unit.is_empty()).map(|unit| format!(" {unit}")).unwrap_or_default();
    let range = match all.first() {
        Some(first) => {
            let (lo, hi) = extent(&all, *first);
            format!(", from {} to {}{unit}", compact(lo), compact(hi))
        }
        None => String::new(),
    };
    let names = if names.is_empty() { "no series" } else { &names };
    format!("{} chart of {names} over {points} points{range}", shape_word(shape))
}

/// What hovering the point at `label` with `value` reads: `start: 10,600 tokens`.
pub fn hover(label: &str, value: f64, unit: Option<&str>) -> String {
    let unit = unit.filter(|unit| !unit.is_empty()).map(|unit| format!(" {unit}")).unwrap_or_default();
    format!("{label}: {}{unit}", grouped(value))
}

#[cfg(test)]
mod tests;
