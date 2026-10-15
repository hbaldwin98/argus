//! The one table from an agent view's [`Tone`] to a [`Theme`] role, and the
//! chip drawn through it.
//!
//! The agent chooses a meaning and never a colour, so every block that
//! shows a tone draws through here.

use argus_protocol::{Chip, Tone};

use super::text::one_line;
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
