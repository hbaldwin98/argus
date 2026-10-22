//! A tinted note in the view's tone, with the glyph the tone means. The glyphs are the page's own
//! paths (`kit/icons`), not the shared set's, which has no warning or info mark of that hand.

use argus_protocol::Tone;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use crate::kit::icons::Glyph;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::chip::tone_colour;

/// The mark a tone means; a tone with none of its own (neutral, active) gets the info mark.
pub fn glyph_of(tone: Tone) -> Glyph {
    match tone {
        Tone::Warn => Glyph::NoteWarn,
        Tone::Bad => Glyph::NoteBad,
        Tone::Good => Glyph::NoteGood,
        Tone::Info | Tone::Neutral | Tone::Active => Glyph::NoteInfo,
    }
}

/// `.vcallout`: the glyph in the tone and the words on a wash of it; newlines in the text stay.
pub fn callout(tone: Tone, text: &str, key: &str, tokens: &Tokens) -> AnyElement {
    let colour = tone_colour(tone, tokens);
    h_flex()
        .id(SharedString::from(format!("{key}-callout")))
        .role(Role::Note)
        .test_support()
        .aria_label(text.to_owned())
        .items_start()
        .gap(rem(tokens.sp_8))
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_8))
        .rounded(rem(tokens.r_md))
        .bg(colour.opacity(tokens.share_callout))
        .child(div().flex_shrink_0().mt(rem(tokens.sp_2)).text_color(colour).child(glyph_of(tone).at(tokens.icon_sm)))
        .child(div().min_w_0().flex_1().child(SharedString::from(text.to_owned())))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tone_with_a_mark_of_its_own_draws_it_and_the_rest_draw_the_info_mark() {
        assert_eq!(glyph_of(Tone::Warn), Glyph::NoteWarn);
        assert_eq!(glyph_of(Tone::Bad), Glyph::NoteBad);
        assert_eq!(glyph_of(Tone::Good), Glyph::NoteGood);
        for tone in [Tone::Info, Tone::Neutral, Tone::Active] {
            assert_eq!(glyph_of(tone), Glyph::NoteInfo, "{tone:?}");
        }
    }
}
