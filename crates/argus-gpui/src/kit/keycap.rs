//! What one key's cap looks like wherever a key is named: the page's `.kbd`.

use gpui_kit::{div, prelude::*, SharedString};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.kbd`: 10px mono on a hairline box, faint. `mono` is the theme's code face.
pub fn keycap(text: impl Into<SharedString>, mono: SharedString, tokens: &Tokens) -> gpui_kit::Div {
    div()
        .flex_shrink_0()
        .font_family(mono)
        .text_size(rem(tokens.fs_2xs))
        // `line-height: 14px` and a 1px border make the 16px cap.
        .line_height(rem(tokens.key_leading))
        .px(rem(tokens.sp_4))
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_xs))
        .text_color(tokens.fg_faint)
        .child(text.into())
}

/// `.btn.primary .kbd`: the cap on an accent-filled button, in the colour the button's words are,
/// since the faint ink and the strong edge of a plain cap are lost on the accent; the edge is 45% of it
/// (`color-mix(in srgb, var(--fg-on-accent) 45%, transparent)`).
pub fn keycap_on_accent(text: impl Into<SharedString>, mono: SharedString, tokens: &Tokens) -> gpui_kit::Div {
    keycap(text, mono, tokens).text_color(tokens.fg_on_accent).border_color(tokens.fg_on_accent.opacity(0.45))
}

/// The keys an item or tooltip names, each press a cap, the alternatives apart by a slash:
/// one list of presses per alternative, as `keys::caps::presses` gives them.
pub fn keycaps(keys: &[Vec<String>], tokens: &Tokens) -> gpui_kit::Div {
    spread(keys, tokens, keycap)
}

/// The same caps on a primary button, where `.kbd` takes the colour of the button's words.
pub fn keycaps_on_accent(keys: &[Vec<String>], tokens: &Tokens) -> gpui_kit::Div {
    spread(keys, tokens, keycap_on_accent).text_color(tokens.fg_on_accent)
}

fn spread(keys: &[Vec<String>], tokens: &Tokens, cap: fn(String, SharedString, &Tokens) -> gpui_kit::Div) -> gpui_kit::Div {
    let mono = SharedString::from(tokens.font_mono);
    let mut caps = gpui_kit::div().flex().items_center().gap(rem(tokens.sp_4)).text_size(rem(tokens.fs_xs)).text_color(tokens.fg_muted);
    for (n, presses) in keys.iter().enumerate() {
        if n > 0 {
            caps = caps.child("/");
        }
        for press in presses {
            caps = caps.child(cap(press.clone(), mono.clone(), tokens));
        }
    }
    caps
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;

    use super::*;

    #[gpui_kit::test]
    fn a_cap_on_a_primary_button_is_in_the_colour_of_the_buttons_words(cx: &mut TestAppContext) {
        // `.btn.primary:not([disabled]) .kbd`: the plain cap's faint ink is lost on the accent.
        let tokens = cx.update(|cx| {
            crate::window::init(cx);
            *Tokens::global(cx)
        });
        let mono = SharedString::from(tokens.font_mono);
        let mut plain = keycap("N", mono.clone(), &tokens);
        let mut lit = keycap_on_accent("N", mono, &tokens);
        assert_eq!(plain.style().text.color, Some(tokens.fg_faint));
        assert_eq!(lit.style().text.color, Some(tokens.fg_on_accent));
        assert_eq!(lit.style().border_color, Some(tokens.fg_on_accent.opacity(0.45)));
        assert_ne!(tokens.fg_on_accent, tokens.fg_faint, "else the cap is told apart by nothing");
    }
}
