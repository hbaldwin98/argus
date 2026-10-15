//! What an agent view's values and tones read as in a terminal: a tone as
//! one of the theme's roles, and each kind of value as styled text.
//!
//! The agent chooses a meaning and never a colour, so the one table from
//! [`Tone`] to a [`Theme`] role lives here and every block draws through it.

use argus_protocol::{Chip, Tone, Value};

use super::*;

/// The theme role a tone stands for. `Info` is the quiet one: it says
/// "worth reading" without asking for anything.
pub(super) fn tone_color(tone: Tone, th: Theme) -> Color {
    match tone {
        Tone::Neutral => th.text,
        Tone::Good => th.ok,
        Tone::Warn => th.warn,
        Tone::Bad => th.err,
        Tone::Active => th.accent,
        Tone::Info => th.muted,
    }
}

pub(super) fn chip_span(chip: &Chip, th: Theme) -> Span<'static> {
    Span::styled(
        format!("[{}]", one_line(&chip.label)),
        Style::default().fg(tone_color(chip.tone, th)),
    )
}

/// Text on one line: a value that holds a newline would otherwise break the
/// row it sits in.
pub(super) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

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
        Value::Empty => muted("—".to_string()),
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

/// Words onto lines `width` cells wide, a word longer than a line broken
/// where it must be. Paragraph breaks in `text` are kept.
pub(super) fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        let mut used = 0;
        for word in paragraph.split_whitespace() {
            let mut word: Vec<char> = word.chars().collect();
            let len = |w: &[char]| w.iter().map(|c| Span::raw(c.to_string()).width()).sum::<usize>();
            while len(&word) > width {
                if used > 0 {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                let mut take = 0;
                let mut cells = 0;
                for c in &word {
                    let w = Span::raw(c.to_string()).width();
                    if cells + w > width && take > 0 {
                        break;
                    }
                    cells += w;
                    take += 1;
                }
                lines.push(word.drain(..take).collect());
            }
            let cells = len(&word);
            if used > 0 && used + 1 + cells > width {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            if used > 0 {
                line.push(' ');
                used += 1;
            }
            line.extend(word);
            used += cells;
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
