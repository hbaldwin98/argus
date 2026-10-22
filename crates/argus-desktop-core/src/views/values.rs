//! What a value in an agent's view says, as words: a number with its thousands, how long ago a time
//! was, the two units a duration is told in, a person's initials and the tint that follows their
//! name, the text a cell filters and copies by, and which references are drawn as buttons.
//!
//! The page writes each of these in `views/values.ts`, `refs.ts` and `table/cell_text.ts`; both
//! windows say them here. Every string is an agent's, read from a ticket, so none is read as
//! anything but text.

use argus_protocol::{Tone, Value};

use crate::connection::page::Reference;
use crate::system::clock::{short, Clock};

/// Up to this many people are drawn with an avatar; the rest are counted.
pub const SHOWN_PEOPLE: usize = 3;

/// The tones an avatar is tinted with, so it follows the theme.
const TINTS: [Tone; 5] = [Tone::Active, Tone::Info, Tone::Good, Tone::Warn, Tone::Bad];

/// A number as `toLocaleString` writes it in English: thousands separated, up to three decimals.
pub fn grouped(number: f64) -> String {
    if number.is_nan() {
        return "NaN".to_owned();
    }
    if number.is_infinite() {
        return if number < 0.0 { "-\u{221e}" } else { "\u{221e}" }.to_owned();
    }
    let fixed = format!("{:.3}", number.abs());
    let (whole, fraction) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let fraction = fraction.trim_end_matches('0');
    let mut out = String::new();
    for (k, digit) in whole.chars().enumerate() {
        if k > 0 && (whole.len() - k) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    if !fraction.is_empty() {
        out.push('.');
        out.push_str(fraction);
    }
    // A negative that rounds to nothing is still written with its sign, as the page's is.
    if number.is_sign_negative() {
        out.insert(0, '-');
    }
    out
}

/// A span of time in the two units that matter: `850 ms`, `4m 12s`, `1h 3m`.
pub fn duration(ms: u64) -> String {
    const SECOND: u64 = 1000;
    const MINUTE: u64 = 60 * SECOND;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    if ms < SECOND {
        format!("{ms} ms")
    } else if ms < MINUTE {
        format!("{}s", ms / SECOND)
    } else if ms < HOUR {
        format!("{}m {}s", ms / MINUTE, (ms % MINUTE) / SECOND)
    } else if ms < DAY {
        format!("{}h {}m", ms / HOUR, (ms % HOUR) / MINUTE)
    } else {
        format!("{}d {}h", ms / DAY, (ms % DAY) / HOUR)
    }
}

/// The first letter of each of the first two words, upper-cased.
pub fn initials(name: &str) -> String {
    name.split_whitespace().take(2).filter_map(|word| word.chars().next()).flat_map(char::to_uppercase).collect()
}

/// The tone a name is tinted with: a hash of its characters, the same in both windows and on every
/// run, so a person keeps their colour.
pub fn tint(name: &str) -> Tone {
    let hash = name.chars().fold(0u32, |hash, c| hash.wrapping_mul(31).wrapping_add(c as u32));
    TINTS[hash as usize % TINTS.len()]
}

/// The people drawn, and how many more there are.
pub fn people(names: &[String]) -> (&[String], usize) {
    let shown = names.len().min(SHOWN_PEOPLE);
    (&names[..shown], names.len() - shown)
}

/// How far along, 0 to 1; an agent's overshoot, a zero max or a number that is not one is not a
/// broken bar.
pub fn fraction(value: f64, max: f64) -> f32 {
    if max.is_nan() || max <= 0.0 || !value.is_finite() {
        return 0.0;
    }
    (value / max).clamp(0.0, 1.0) as f32
}

/// The whole percentage a progress value says.
pub fn percent(value: f64, max: f64) -> u32 {
    (f64::from(fraction(value, max)) * 100.0).round() as u32
}

/// Whether `at` begins as a date and a time: `YYYY-MM-DD` and then `T` or a space.
fn dated(at: &str) -> bool {
    let bytes = at.as_bytes();
    bytes.len() > 10
        && bytes[..10].iter().enumerate().all(|(k, b)| if k == 4 || k == 7 { *b == b'-' } else { b.is_ascii_digit() })
        && matches!(bytes[10], b'T' | b' ')
}

/// How long ago an RFC 3339 time was, `4 min ago`; the text as given when it is not one.
pub fn ago(clock: &Clock, at: &str) -> String {
    let Some(then) = dated(at).then(|| clock.zoned(at)).flatten() else { return at.to_owned() };
    let now = clock.now_zoned();
    let minutes = (now.timestamp().as_millisecond() - then.timestamp().as_millisecond()).div_euclid(60_000);
    // A time ahead of this clock is skew, not the future.
    if minutes < 1 {
        "just now".to_owned()
    } else if minutes < 60 {
        format!("{minutes} min ago")
    } else if minutes < 24 * 60 {
        format!("{} h ago", minutes / 60)
    } else if now.date().yesterday().is_ok_and(|yesterday| then.date() == yesterday) {
        "yesterday".to_owned()
    } else if then.year() == now.year() {
        short(then.date())
    } else {
        format!("{}, {}", short(then.date()), then.year())
    }
}

/// A calendar day as `Oct 12, 2026`. A bare `YYYY-MM-DD` is a day, not an instant, so it is read as
/// it is written and not moved by a zone; anything unreadable is given as it came.
pub fn short_date(clock: &Clock, raw: &str) -> String {
    let date = raw.parse::<jiff::civil::Date>().ok().or_else(|| clock.zoned(raw).map(|at| at.date()));
    match date {
        Some(date) => format!("{}, {}", short(date), date.year()),
        None => raw.to_owned(),
    }
}

/// An instant as the person's own clock reads it, for a tooltip: `Oct 19, 2026 13:52`.
pub fn local_time(clock: &Clock, at: &str) -> Option<String> {
    let at = clock.zoned(at)?;
    Some(format!("{}, {} {:02}:{:02}", short(at.date()), at.year(), at.hour(), at.minute()))
}

/// What a value is written as where only text is read: a filter's match and a table copied as TSV.
pub fn cell_text(value: &Value) -> String {
    match value {
        Value::Text { text } | Value::Code { text } => text.clone(),
        // Not grouped: a filter matches the digits an agent wrote.
        Value::Number { number, unit } => match unit.as_deref().filter(|unit| !unit.is_empty()) {
            Some(unit) => format!("{} {unit}", plain(*number)),
            None => plain(*number),
        },
        Value::Bool { value } => if *value { "yes" } else { "no" }.to_owned(),
        Value::Date { date } => date.clone(),
        Value::DateTime { at } => at.clone(),
        Value::Person { name } => name.clone(),
        Value::People { names } => names.join(", "),
        Value::Chip { chip } => chip.label.clone(),
        Value::Chips { chips } => chips.iter().map(|chip| chip.label.as_str()).collect::<Vec<_>>().join(", "),
        Value::Link { label, .. } => label.clone(),
        Value::Ticket { key, title, .. } => match title.as_deref().filter(|title| !title.is_empty()) {
            Some(title) => format!("{key} {title}"),
            None => key.clone(),
        },
        Value::Progress { value, max, .. } => format!("{}/{}", plain(*value), plain(*max)),
        Value::Duration { ms } => format!("{ms} ms"),
        Value::Path { path, line } => match line {
            Some(line) => format!("{path}:{line}"),
            None => path.clone(),
        },
        Value::Commit { sha, title } => match title.as_deref().filter(|title| !title.is_empty()) {
            Some(title) => format!("{sha} {title}"),
            None => sha.clone(),
        },
        Value::Feature { number } => format!("#{number}"),
        Value::Pane { pane, label } => label.clone().unwrap_or_else(|| format!("pane {pane}")),
        Value::View { id, label } => label.clone().unwrap_or_else(|| id.clone()),
        Value::Empty | Value::Unknown => String::new(),
    }
}

/// A number as JavaScript's `String(n)` writes it: no trailing `.0`.
fn plain(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

/// A reference drawn as a button: what it says, what it is for, and what pressing it asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pointer {
    pub text: String,
    /// The tooltip.
    pub title: &'static str,
    pub reference: Reference,
}

/// The button a path, a commit's hash, a feature or a pane is drawn as, or none: a value of another
/// kind, and a path that is not inside the checkout (that one is plain text, since a button for it
/// could only say no).
pub fn pointer(value: &Value) -> Option<Pointer> {
    match value {
        Value::Path { path, line } if argus_protocol::is_checkout_relative(path) => Some(Pointer {
            text: line.map_or_else(|| path.clone(), |line| format!("{path}:{line}")),
            title: "Open in your editor",
            reference: Reference::Path { path: path.clone(), line: *line },
        }),
        Value::Commit { sha, .. } => Some(Pointer {
            text: sha.chars().take(7).collect(),
            title: "Show in History",
            reference: Reference::Commit { sha: sha.clone() },
        }),
        Value::Feature { number } => Some(Pointer {
            text: format!("#{number}"),
            title: "Show in the Feature stage",
            reference: Reference::Feature { number: *number },
        }),
        Value::Pane { pane, label } => Some(Pointer {
            text: label.clone().unwrap_or_else(|| format!("pane #{pane}")),
            title: "Go to the pane",
            reference: Reference::Pane { pane: *pane },
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
