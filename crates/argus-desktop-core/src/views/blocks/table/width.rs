//! How wide each column of a table is taken to be, from what its cells say.
//!
//! The page lets the browser size a table's columns to their content. A window that sizes by flex
//! rows has to be told, so a column is as wide as its widest cell is estimated to be (a width per
//! character, as a diagram's boxes are sized), and a long text cell is cut at 280 px, which is what
//! the page's `td.vtable-long` does. The rows then share any room beyond that in proportion to these
//! widths, as an auto table does.

use argus_protocol::Value;

use crate::views::blocks::label::CHAR_W;
use crate::views::values::{cell_text, grouped};

/// A cell's own padding either side.
pub const CELL_PAD: f32 = 16.0;
/// What a character of monospace is taken to be.
const MONO_W: f32 = 7.8;
/// Past this many characters a text cell is cut and shows its whole text as a tooltip.
pub const LONG: usize = 24;
/// How wide a long text cell grows before it is cut.
pub const LONG_MAX: f32 = 280.0;
/// An avatar and the gap after it, a chip's own padding, a meter and its percentage.
const AVATAR: f32 = 26.0;
const CHIP_PAD: f32 = 16.0;
const METER: f32 = 112.0;
/// A header's sort arrow and its gap, and how wide a capital of the label type is.
const ARROW: f32 = 16.0;
const CAP_W: f32 = 8.0;

fn chars(text: &str) -> f32 {
    text.chars().count() as f32
}

/// How wide the cell holding `value` is estimated to be, its padding not counted.
pub fn estimate(value: &Value) -> f32 {
    match value {
        Value::Text { text } => (chars(text) * CHAR_W).min(if text.chars().count() > LONG { LONG_MAX } else { f32::MAX }),
        Value::Code { text } => chars(text) * MONO_W + 12.0,
        Value::Number { number, unit } => chars(&grouped(*number)) * MONO_W + unit.as_deref().map_or(0.0, |unit| (chars(unit) + 1.0) * CHAR_W),
        Value::Bool { .. } => 48.0,
        Value::Date { .. } => 64.0,
        Value::DateTime { .. } => 72.0,
        Value::Person { name } => AVATAR + chars(name) * CHAR_W,
        Value::People { names } => AVATAR * names.len().min(3) as f32 + chars(&names.join(", ")) * CHAR_W,
        Value::Chip { chip } => chars(&chip.label) * CHAR_W * 0.9 + CHIP_PAD,
        Value::Chips { chips } => chips.iter().map(|chip| chars(&chip.label) * CHAR_W * 0.9 + CHIP_PAD + 4.0).sum(),
        Value::Ticket { key, title, status, .. } => {
            chars(key) * MONO_W + 12.0 + title.as_deref().map_or(0.0, |title| chars(title) * CHAR_W + 8.0) + status.as_ref().map_or(0.0, |s| chars(&s.label) * CHAR_W * 0.9 + CHIP_PAD + 8.0)
        }
        Value::Progress { .. } => METER,
        Value::Path { .. } | Value::Commit { .. } | Value::Feature { .. } => chars(&cell_text(value)) * MONO_W + 12.0,
        Value::Link { .. } | Value::Duration { .. } | Value::Pane { .. } | Value::View { .. } => chars(&cell_text(value)) * CHAR_W,
        Value::Empty | Value::Unknown => 0.0,
    }
}

/// How wide a header is: its title in capitals and its sort arrow.
pub fn header(title: &str) -> f32 {
    chars(title) * CAP_W + ARROW
}

/// The width of each of `titles`' columns: the widest of its header and its cells, and its padding.
pub fn columns(titles: &[&str], rows: &[&[Value]]) -> Vec<f32> {
    titles
        .iter()
        .enumerate()
        .map(|(c, title)| rows.iter().filter_map(|cells| cells.get(c)).map(estimate).fold(header(title), f32::max) + CELL_PAD)
        .collect()
}

#[cfg(test)]
mod tests;
