//! How a table's rows sort when a header is clicked: what a cell sorts by, how two of those
//! compare, and which way a click turns a column.
//!
//! Text compares as the page's `localeCompare` does, on its lowercase form (`collate`). Rows that
//! compare equal keep the order they were first drawn in, so sorting twice never shuffles ties
//! (TABLE-2).

use std::cmp::Ordering;

use argus_protocol::Value;
use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;
use jiff::Timestamp;

use super::collate::{js_string, locale_compare};

/// Which way a column is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ascending,
    Descending,
}

/// What a cell sorts by: figures and times by magnitude, the rest by lowercase text.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Number(f64),
    Text(String),
}

/// `Date.parse(text)` in milliseconds, or NaN where it reads nothing. The forms an agent writes:
/// an instant with an offset or `Z`; a bare date, month or year, which is midnight UTC; and a
/// date and time with no offset, which is the machine's own zone. What the browser's older,
/// looser reader takes beyond those (`Mar 1, 2025`, `2025/03/01`) is unreadable here.
fn time(text: &str) -> f64 {
    let text = text.trim();
    if let Ok(at) = text.parse::<Timestamp>() {
        return at.as_millisecond() as f64;
    }
    if let Ok(date) = text.parse::<Date>() {
        return utc_midnight(date);
    }
    // `2025-03` and `2025`.
    let mut parts = text.splitn(2, '-');
    let year = parts.next().filter(|y| y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()));
    if let (Some(year), month) = (year, parts.next()) {
        let month = match month {
            None => Some(1),
            Some(m) if m.len() == 2 && m.bytes().all(|b| b.is_ascii_digit()) => m.parse::<i8>().ok(),
            Some(_) => None,
        };
        if let (Ok(year), Some(month)) = (year.parse::<i16>(), month) {
            if let Ok(date) = Date::new(year, month, 1) {
                return utc_midnight(date);
            }
        }
    }
    match text.parse::<DateTime>().ok().and_then(|at| at.to_zoned(TimeZone::system()).ok()) {
        Some(at) => at.timestamp().as_millisecond() as f64,
        None => f64::NAN,
    }
}

fn utc_midnight(date: Date) -> f64 {
    match date.at(0, 0, 0, 0).to_zoned(TimeZone::UTC) {
        Ok(at) => at.timestamp().as_millisecond() as f64,
        Err(_) => f64::NAN,
    }
}

fn text(text: &str) -> Key {
    Key::Text(text.to_lowercase())
}

/// What `value` sorts by.
pub fn key(value: &Value) -> Key {
    match value {
        Value::Number { number, .. } => Key::Number(*number),
        Value::Date { date } => Key::Number(time(date)),
        Value::DateTime { at } => Key::Number(time(at)),
        Value::Text { text: t } | Value::Code { text: t } => text(t),
        Value::Bool { value } => Key::Number(f64::from(u8::from(*value))),
        Value::Person { name } => text(name),
        Value::People { names } => text(&names.join(" ")),
        Value::Chip { chip } => text(&chip.label),
        Value::Chips { chips } => text(&chips.iter().map(|chip| chip.label.as_str()).collect::<Vec<_>>().join(" ")),
        Value::Link { label, .. } => text(label),
        Value::Ticket { key, .. } => text(key),
        Value::Progress { value, max, .. } => Key::Number(value / max),
        Value::Duration { ms } => Key::Number(*ms as f64),
        Value::Path { path, .. } => text(path),
        Value::Commit { sha, .. } => text(sha),
        Value::Feature { number } => Key::Number(*number as f64),
        Value::Pane { pane, label } => text(&label.clone().unwrap_or_else(|| pane.to_string())),
        Value::View { id, label } => text(label.as_deref().unwrap_or(id)),
        Value::Empty | Value::Unknown => Key::Text(String::new()),
    }
}

fn shown(key: &Key) -> String {
    match key {
        Key::Number(n) => js_string(*n),
        Key::Text(t) => t.clone(),
    }
}

/// How two keys order. A number that is not one (an unparseable date) goes last, ascending; a number against
/// text compares as the text `String(n)` writes.
fn compare(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Number(a), Key::Number(b)) => match (a.is_nan(), b.is_nan()) {
            // `a - b` in the page: -0 and 0 are one figure, and so are two infinities.
            (false, false) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (x, y) => x.cmp(&y),
        },
        _ => locale_compare(&shown(a), &shown(b)),
    }
}

/// The indexes of `rows` ordered by column `c`. A row with no cell there sorts as an empty text.
pub fn order(rows: &[&[Value]], c: usize, direction: Direction) -> Vec<usize> {
    let keys: Vec<Key> = rows.iter().map(|cells| cells.get(c).map_or(Key::Text(String::new()), key)).collect();
    let mut at: Vec<usize> = (0..rows.len()).collect();
    at.sort_by(|&x, &y| {
        let (a, b) = (&keys[x], &keys[y]);
        // As the page does, the direction turns the gaps over with the rest: an unreadable date
        // is last going up and first coming down.
        let by = match direction {
            Direction::Ascending => compare(a, b),
            Direction::Descending => compare(b, a),
        };
        by.then(x.cmp(&y))
    });
    at
}

/// The column and direction a click on header `c` leaves, given the column sorted now (the agent's
/// `sorted_by` at first, whose direction is not known) and whether it runs ascending: a second
/// click on a column that runs ascending turns it descending; anything else starts it ascending.
pub fn click(active: Option<usize>, ascending_now: bool, c: usize) -> (usize, Direction) {
    if active == Some(c) && ascending_now {
        (c, Direction::Descending)
    } else {
        (c, Direction::Ascending)
    }
}

#[cfg(test)]
mod tests;
