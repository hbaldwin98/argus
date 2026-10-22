//! How a table's rows sort when a header is clicked: what a cell sorts by, how two of those
//! compare, and which way a click turns a column.
//!
//! The page sorts with `localeCompare`, which depends on the machine's locale. Here text compares
//! as its lowercase form by code point, which orders the same for the ASCII an agent's labels are
//! written in and is the same everywhere; a rule a reader can state is better than the browser's.
//! Rows that compare equal keep the order they were first drawn in, so sorting twice never shuffles
//! ties (TABLE-2).

use std::cmp::Ordering;

use argus_protocol::Value;
use jiff::civil::Date;
use jiff::Timestamp;

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

fn time(text: &str) -> f64 {
    if let Ok(at) = text.trim().parse::<Timestamp>() {
        return at.as_millisecond() as f64;
    }
    // A bare date is midnight UTC, as `Date.parse` reads one.
    match text.trim().parse::<Date>().ok().and_then(|date| date.at(0, 0, 0, 0).in_tz("UTC").ok()) {
        Some(at) => at.timestamp().as_millisecond() as f64,
        None => f64::NAN,
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
        Key::Number(n) => n.to_string(),
        Key::Text(t) => t.clone(),
    }
}

/// How two keys order. A number that is not one (an unparseable date) goes last, ascending; a number against
/// text compares as text.
fn compare(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Number(a), Key::Number(b)) => match (a.is_nan(), b.is_nan()) {
            (false, false) => a.total_cmp(b),
            (x, y) => x.cmp(&y),
        },
        _ => shown(a).cmp(&shown(b)),
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
