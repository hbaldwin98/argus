//! The y axis of a chart: round ticks from at most zero to at least the data's top, three to five of
//! them, whatever the data (CHART-2).

/// The most ticks one axis is given; a step too small for the span would otherwise never reach the
/// top (a float that cannot advance at 1e15), and the cap is what ends the loop regardless.
pub const MAX_TICKS: usize = 12;

/// The axis: its range and the values that are ticked.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    pub lo: f64,
    pub hi: f64,
    pub ticks: Vec<f64>,
}

/// A step of 1, 2, 5 or 10 times a power of ten that is at least `raw`.
fn nice_step(raw: f64) -> f64 {
    let pow = 10f64.powf(raw.log10().floor());
    let f = raw / pow;
    let n = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    n * pow
}

/// A float to twelve significant digits, so `0.1 + 0.2` ticks as `0.3`.
fn tidy(value: f64) -> f64 {
    format!("{value:.11e}").parse().unwrap_or(value)
}

/// The smallest and largest of `values`, the `seed` included. A loop and not a fold over a spread, so
/// a long array cannot overflow anything (CHART-9).
pub fn extent(values: &[f64], seed: f64) -> (f64, f64) {
    let (mut lo, mut hi) = (seed, seed);
    for &v in values {
        if v < lo {
            lo = v;
        }
        if v > hi {
            hi = v;
        }
    }
    (lo, hi)
}

/// The axis for data running from `min` to `max`. A range that is not a number falls back to 0 to 1.
pub fn axis(min: f64, max: f64) -> Axis {
    let mut lo = min.min(0.0);
    let mut hi = max.max(lo);
    if hi - lo < 1e-9 {
        hi = lo + 1.0;
    }
    let step = nice_step((hi - lo) / 3.0);
    lo = (lo / step + 1e-9).floor() * step;
    hi = (hi / step - 1e-9).ceil() * step;
    if ![lo, hi, step].iter().all(|v| v.is_finite()) || step <= 0.0 {
        return Axis { lo: 0.0, hi: 1.0, ticks: vec![0.0, 1.0] };
    }
    let mut ticks = Vec::new();
    let mut t = lo;
    while ticks.len() < MAX_TICKS && t <= hi + step / 2.0 {
        ticks.push(tidy(t));
        t += step;
    }
    Axis { lo, hi, ticks }
}

#[cfg(test)]
mod tests;
