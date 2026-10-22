//! How a table's text orders as the page's does: `localeCompare` with no locale named, and
//! `String(n)` for a figure that is compared against text.
//!
//! `localeCompare` is the browser's ICU collation, and for English that is the root collation:
//! letters ignoring case and accents first, then accents, then case with lowercase first,
//! punctuation before digits before letters, digits one at a time. `icu_collator` is the same
//! algorithm and data, so `ä` sorts beside `a` and `_x` before `x` here as it does there; code
//! point order would put `ä` after `z`. The page asks the machine's locale and this asks the
//! root's, which is the same for an English machine and the only one a reader can state.

use std::cmp::Ordering;
use std::sync::OnceLock;

use icu_collator::options::CollatorOptions;
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};

fn collator() -> Option<&'static CollatorBorrowed<'static>> {
    static COLLATOR: OnceLock<Option<CollatorBorrowed<'static>>> = OnceLock::new();
    COLLATOR.get_or_init(|| Collator::try_new(CollatorPreferences::default(), CollatorOptions::default()).ok()).as_ref()
}

/// `a.localeCompare(b)`.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    match collator() {
        Some(collator) => collator.compare(a, b),
        // Compiled data that will not load is a build fault; the order is still a total one.
        None => a.cmp(b),
    }
}

/// `String(n)`: JavaScript writes a figure in decimal from 1e-6 up to 1e21 and in exponent form
/// outside that, and never writes `-0` or a `.0`.
pub fn js_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_owned();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if n == 0.0 {
        return "0".to_owned();
    }
    if n.abs() >= 1e21 || n.abs() < 1e-6 {
        // Rust writes `1e21` and `1.5e-7`; JavaScript signs the exponent.
        let written = format!("{n:e}");
        return match written.split_once('e') {
            Some((digits, exponent)) if !exponent.starts_with('-') => format!("{digits}e+{exponent}"),
            _ => written,
        };
    }
    n.to_string()
}

#[cfg(test)]
mod tests;
