//! A status pill an agent drew: it picks a tone, a meaning, and the window owns the hue, so a view
//! looks the same in every theme whoever wrote it.

use argus_protocol::{Chip, Tone};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Hsla, SharedString};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The colour a tone means, `.t-good` and the rest: a theme's own, never an agent's.
pub fn tone_colour(tone: Tone, tokens: &Tokens) -> Hsla {
    match tone {
        Tone::Neutral => tokens.fg_muted,
        Tone::Good => tokens.ok,
        Tone::Warn => tokens.attention,
        Tone::Bad => tokens.danger,
        Tone::Active => tokens.accent,
        Tone::Info => tokens.info,
    }
}

/// `.vchip`: the label in its tone on a wash (`share_chip`) of it, a pill.
pub fn chip(chip: &Chip, tokens: &Tokens) -> impl IntoElement {
    let colour = tone_colour(chip.tone, tokens);
    div()
        .flex_shrink_0()
        .px(rem(tokens.sp_8))
        .py(rem(tokens.hairline))
        .rounded(rem(tokens.r_pill))
        .bg(colour.opacity(tokens.share_chip))
        .text_color(colour)
        .text_size(rem(tokens.fs_xs))
        .weighted(tokens.weight_semibold)
        .whitespace_nowrap()
        .child(SharedString::from(chip.label.clone()))
}

/// `.vdotted`: the quiet form of a status in a dense table, a dot in the tone's colour and the word
/// in muted text, the word itself coloured only for a bad one.
pub fn dotted(chip: &Chip, tokens: &Tokens) -> impl IntoElement {
    let colour = tone_colour(chip.tone, tokens);
    h_flex()
        .items_center()
        .gap(rem(tokens.sp_8))
        .whitespace_nowrap()
        .child(div().size(rem(tokens.dot_sm)).flex_shrink_0().rounded_full().bg(colour))
        .child(
            div()
                .text_size(rem(tokens.fs_xs))
                .text_color(if chip.tone == Tone::Bad { tokens.danger } else { tokens.fg_muted })
                .child(SharedString::from(chip.label.clone())),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::themes::ledger;

    #[test]
    fn each_tone_is_a_colour_of_the_theme_and_no_two_are_alike() {
        let tokens = ledger::tokens();
        let all = [Tone::Neutral, Tone::Good, Tone::Warn, Tone::Bad, Tone::Active, Tone::Info].map(|tone| tone_colour(tone, &tokens));
        for (k, colour) in all.iter().enumerate() {
            assert!(all.iter().skip(k + 1).all(|other| other != colour), "{k}");
        }
        assert_eq!(tone_colour(Tone::Good, &tokens), tokens.ok);
        assert_eq!(tone_colour(Tone::Warn, &tokens), tokens.attention, "warn is what needs the person");
        assert_eq!(tone_colour(Tone::Neutral, &tokens), tokens.fg_muted);
    }

    #[test]
    fn another_theme_gives_the_same_tone_its_own_hue() {
        let ledger = tone_colour(Tone::Bad, &ledger::tokens());
        let night = tone_colour(Tone::Bad, &crate::theme::derive::tokens(&crate::theme::themes::night::NIGHT));
        assert_ne!(ledger, night);
    }
}
