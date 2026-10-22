//! The Browsers stage with no browser open (SHELL-BROWSER-3): what an old daemon, a window that
//! has heard none and the last browser going all show, with no notice and nothing disabled.

use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, Font, FontFeatures, FontStyle, FontWeight, Hsla, Role, SharedString, StyledText, TestSupportExt as _, TextRun};

use crate::theme::scale::rem;
use crate::theme::Tokens;

pub const HEADLINE: &str = "No agent has a browser open.";

/// The hint's words either side of the command, which is set in the code face.
pub const HINT_BEFORE: &str = "An agent opens one with ";
pub const COMMAND: &str = "argus-hook browser";
pub const HINT_AFTER: &str = " and drives it with its own browser tool; it shows here as it works.";

/// The hint as one wrapped run: GPUI's highlights cannot change a family, so each cut is told its
/// face (the same reason `views/blocks/markdown/runs` does).
fn hint(tokens: &Tokens) -> StyledText {
    let run = |len: usize, mono: bool, colour: Hsla| TextRun {
        len,
        font: Font {
            family: SharedString::from(if mono { tokens.font_mono } else { tokens.font_ui }),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight(tokens.weight_regular),
            style: FontStyle::Normal,
        },
        color: colour,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let text = format!("{HINT_BEFORE}{COMMAND}{HINT_AFTER}");
    let runs = vec![run(HINT_BEFORE.len(), false, tokens.fg_muted), run(COMMAND.len(), true, tokens.fg_muted), run(HINT_AFTER.len(), false, tokens.fg_muted)];
    StyledText::new(text).with_runs(runs)
}

/// `.empty` over `.empty-body`: the two lines centred in the stage.
pub fn stage(tokens: &Tokens) -> impl IntoElement {
    v_flex().id("browsers-empty").role(Role::Group).test_support().size_full().items_center().justify_center().bg(tokens.see_surface).child(
        v_flex()
            .id("browsers-empty-body")
            .items_center()
            .gap(rem(tokens.sp_16))
            // The page's `max-width` is a content box and GPUI's is a border box, so the padding is added.
            .max_w(rem(tokens.empty_w + tokens.sp_24 * 2.))
            .p(rem(tokens.sp_24))
            .text_center()
            // The page's two `<p>`s keep the browser's 1em margins, which a flex column does not collapse.
            .child(div().my(rem(tokens.fs_md)).child(HEADLINE))
            .child(div().my(rem(tokens.fs_md)).child(hint(tokens))),
    )
}
