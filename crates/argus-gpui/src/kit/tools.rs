//! The tool buttons of a head (`project/Kit.dc.html`, "Tools"): a ghost button with its key after
//! the label and one outlined primary. The switch's pills are `segment::switch_option`.
//!
//! Code and Git name the same tool the same way ("Open in editor"), so the buttons are drawn here
//! once. What a press sends is the caller's `on_click`.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, ElementId, Role, SharedString};

use super::icon_button::Node;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.gb`: 28px, no edge, muted until the pointer is on it; `pressed` is a toggle that is on
/// (`Outline`). `key` is the key that does the same, in mono after the label.
pub fn ghost(id: impl Into<ElementId>, words: impl Into<SharedString>, key: Option<&str>, pressed: bool, tokens: &Tokens) -> Node {
    let (fill, text, over) = (tokens.fill, tokens.fg, tokens.hover_fill);
    let words = words.into();
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(words.clone())
        .aria_selected(pressed)
        .flex_shrink_0()
        .h(rem(tokens.icon_btn))
        .px(rem(9.0))
        .gap(rem(tokens.sp_6))
        .items_center()
        .rounded(rem(tokens.r_sm))
        .text_size(rem(tokens.fs_sm))
        .whitespace_nowrap()
        .cursor_pointer()
        .map(|button| if pressed { button.bg(fill).text_color(text) } else { button.text_color(tokens.fg_muted).hover(move |style| style.bg(over).text_color(text)) })
        .child(words)
        .children(key.map(|key| {
            div().font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_xs)).text_color(tokens.fg_faint).child(SharedString::from(key.to_owned()))
        }))
}

/// `.pb`: the one outlined primary of a head (`Stage file`): the text colour on a raised fill and a
/// strong edge, lifting to `--fill`.
pub fn primary(id: impl Into<ElementId>, words: impl Into<SharedString>, tokens: &Tokens) -> Node {
    outlined(id, words, false, tokens)
}

/// [`primary`] at 24px, for a band that is itself short (`Stage hunk`).
pub fn primary_small(id: impl Into<ElementId>, words: impl Into<SharedString>, tokens: &Tokens) -> Node {
    outlined(id, words, true, tokens)
}

fn outlined(id: impl Into<ElementId>, words: impl Into<SharedString>, small: bool, tokens: &Tokens) -> Node {
    let (fill, press) = (tokens.fill, tokens.press_fill);
    let words = words.into();
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(words.clone())
        .flex_shrink_0()
        .h(rem(if small { tokens.control_h_sm } else { tokens.icon_btn }))
        .px(rem(if small { tokens.sp_8 } else { tokens.sp_12 }))
        .items_center()
        .rounded(rem(tokens.r_sm))
        .border_1()
        .border_color(tokens.line_strong)
        .bg(tokens.bg_raised)
        .text_color(tokens.fg)
        .text_size(rem(tokens.fs_sm))
        .weighted(tokens.weight_medium)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |style| style.bg(fill))
        .active(move |style| style.bg(press))
        .child(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::host;
    use gpui_kit::TestAppContext;

    /// Kit.dc.html: the buttons are 28px and the key follows the label inside the button.
    #[gpui_kit::test]
    fn a_ghost_button_is_28px_and_holds_its_key_after_the_label(cx: &mut TestAppContext) {
        let handle = host::open(cx, 600.0, |tokens| {
            h_flex()
                .gap(rem(8.0))
                .child(ghost("plain", "Whole file", None, false, tokens))
                .child(ghost("keyed", "Open in editor", Some("E"), false, tokens))
                .child(primary("main", "Stage file", tokens))
                .into_any_element()
        });
        let (_, _, plain, h) = host::bounds(cx, handle, "plain");
        let (_, _, keyed, _) = host::bounds(cx, handle, "keyed");
        let (_, _, _, primary_h) = host::bounds(cx, handle, "main");
        assert_eq!((h, primary_h), (28.0, 28.0));
        assert!(plain > 0.0 && keyed > plain, "the keyed button is wider than its words");
    }
}
