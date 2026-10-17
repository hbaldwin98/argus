//! What each kind of agent view value reads as in a terminal: styled text
//! on one line, or wrapped lines for a long text.

use argus_protocol::Value;

use super::tone::{chip_span, tone_color};
use crate::ui::text::{one_line, wrap_words};
use super::*;

/// A number as a person writes it: whole when it is whole.
pub(super) fn number_text(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        let text = format!("{number:.2}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// `4m 12s`, `830ms`, `2h 5m`.
pub(super) fn duration_text(ms: u64) -> String {
    let secs = ms / 1000;
    match secs {
        0 => format!("{ms}ms"),
        1..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m {}s", secs / 60, secs % 60),
        _ => format!("{}h {}m", secs / 3600, secs % 3600 / 60),
    }
}

/// A meter `width` cells wide, filled in proportion to `value` of `max`.
pub(super) fn meter(value: f64, max: f64, width: usize) -> String {
    let filled = (fraction(value, max) * width as f64).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled.min(width)))
}

pub(super) fn percent(value: f64, max: f64) -> String {
    format!("{}%", (fraction(value, max) * 100.0).round() as u32)
}

fn fraction(value: f64, max: f64) -> f64 {
    if max > 0.0 && value.is_finite() {
        (value / max).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// A trend as the eight-step bars, scaled to its own range.
pub(super) fn spark(values: &[f64]) -> String {
    const STEPS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let finite = values.iter().copied().filter(|v| v.is_finite());
    let low = finite.clone().fold(f64::INFINITY, f64::min);
    let high = finite.fold(f64::NEG_INFINITY, f64::max);
    values
        .iter()
        .map(|v| {
            if !v.is_finite() || high <= low {
                STEPS[3]
            } else {
                STEPS[(((v - low) / (high - low)) * 7.0).round() as usize]
            }
        })
        .collect()
}

/// A value as spans on one line. Only a long text is ever more than one
/// line, and [`value_lines`] is what wraps it.
pub(super) fn value_spans(value: &Value, th: Theme) -> Vec<Span<'static>> {
    let plain = |text: String| vec![Span::styled(text, Style::default().fg(th.text))];
    let muted = |text: String| vec![Span::styled(text, Style::default().fg(th.muted))];
    match value {
        Value::Text { text } => plain(one_line(text)),
        Value::Number { number, unit } => plain(match unit.as_deref() {
            None | Some("") => number_text(*number),
            Some("%") => format!("{}%", number_text(*number)),
            Some(unit) => format!("{} {unit}", number_text(*number)),
        }),
        Value::Bool { value } => vec![Span::styled(
            if *value { "yes" } else { "no" },
            Style::default().fg(if *value { th.ok } else { th.muted }),
        )],
        Value::Date { date } => plain(date.clone()),
        Value::DateTime { at } => plain(date_time_text(at)),
        Value::Person { name } => plain(one_line(name)),
        Value::People { names } => plain(names.iter().map(|n| one_line(n)).collect::<Vec<_>>().join(", ")),
        Value::Chip { chip } => vec![chip_span(chip, th)],
        Value::Chips { chips } => {
            let mut spans = Vec::new();
            for (at, chip) in chips.iter().enumerate() {
                if at > 0 {
                    spans.push(Span::raw(" "));
                }
                spans.push(chip_span(chip, th));
            }
            spans
        }
        Value::Link { label, url } => {
            if label.is_empty() || label == url {
                vec![Span::styled(url.clone(), Style::default().fg(th.accent))]
            } else {
                vec![
                    Span::styled(one_line(label), Style::default().fg(th.accent)),
                    Span::styled(format!(" ({url})"), Style::default().fg(th.dim)),
                ]
            }
        }
        Value::Ticket { key, title, url: _, status } => {
            let mut spans = vec![Span::styled(
                key.clone(),
                Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
            )];
            if let Some(title) = title {
                spans.push(Span::styled(format!(" {}", one_line(title)), Style::default().fg(th.text)));
            }
            if let Some(status) = status {
                spans.push(Span::raw(" "));
                spans.push(chip_span(status, th));
            }
            spans
        }
        Value::Progress { value, max, tone } => vec![
            Span::styled(meter(*value, *max, 10), Style::default().fg(tone_color(*tone, th))),
            Span::styled(format!(" {}", percent(*value, *max)), Style::default().fg(th.muted)),
        ],
        Value::Duration { ms } => plain(duration_text(*ms)),
        Value::Code { text } => vec![Span::styled(
            format!("`{}`", one_line(text)),
            Style::default().fg(th.syntax.string),
        )],
        Value::Path { path, line } => plain(match line {
            Some(line) => format!("{path}:{line}"),
            None => path.clone(),
        }),
        Value::Commit { sha, title } => {
            let mut spans = vec![Span::styled(
                sha.chars().take(8).collect::<String>(),
                Style::default().fg(th.syntax.number),
            )];
            if let Some(title) = title {
                spans.push(Span::styled(format!(" {}", one_line(title)), Style::default().fg(th.text)));
            }
            spans
        }
        Value::Feature { number } => plain(format!("feature #{number}")),
        Value::Pane { pane, label } => plain(match label {
            Some(label) if !label.is_empty() => one_line(label),
            _ => format!("pane {pane}"),
        }),
        // A link reads as its label and a marker; `l` on a saved view lists
        // the ones it names.
        Value::View { id, label } => plain(match label {
            Some(label) if !label.is_empty() => format!("{} ›", one_line(label)),
            _ => format!("view {} ›", one_line(id)),
        }),
        Value::Empty => muted("—".to_string()),
        Value::Unknown => muted("(needs a newer Argus)".to_string()),
    }
}

/// `2026-10-15T12:30:00Z` as `2026-10-15 12:30`: the source's own words when
/// it did not write an RFC 3339 time.
pub(super) fn date_time_text(at: &str) -> String {
    match at.split_once('T') {
        Some((date, time)) if time.len() >= 5 => format!("{date} {}", &time[..5]),
        _ => at.to_string(),
    }
}

/// A value as lines no wider than `width`: a long text wraps, anything else
/// is cut on its one line.
pub(super) fn value_lines(value: &Value, width: usize, th: Theme) -> Vec<Vec<Span<'static>>> {
    if let Value::Text { text } = value {
        return wrap_words(text, width)
            .into_iter()
            .map(|line| vec![Span::styled(line, Style::default().fg(th.text))])
            .collect();
    }
    vec![ellipsize_spans(value_spans(value, th), width)]
}
