//! A diff block: an agent's unified diff drawn by the diff view the review draws with, so a diff
//! looks the same wherever it is (PATCH-1 to PATCH-3).
//!
//! Core reads the text into rows (`views::blocks::patch`); text with no hunk in it is not a diff and
//! is drawn as a `diff` code block. The view takes no clicks and picks nothing. It owns its scroll,
//! so it is given a height: the rows' own up to `--diff-max-h`, past which it scrolls, and the wheel
//! goes on to the page when it can move no further.

use argus_desktop_core::views::blocks::patch::parse_patch;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::code::code;
use super::pen::Pen;
use crate::kit::wheel::Wheel;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::ctx::ViewCtx;

/// `.vdiff`: a header with the path (the block's own, else the diff's `+++` name) and what was added
/// and removed, over the rows.
pub fn patch(path: Option<&str>, text: &str, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let Some(read) = parse_patch(text) else { return code(Some("diff"), text, &ctx.path, pen) };
    let tokens = pen.draw.tokens;
    let key = format!("{}-diff", ctx.path);
    let name = path.map(str::to_owned).or(read.file);
    let view = pen.diff(&format!("{key}-view"));
    view.update(pen.cx, |view, cx| view.show(&read.rows, false, None, cx));
    let (height, handle) = {
        let view = view.read(pen.cx);
        (view.content_height().min(tokens.diff_max_h), view.scroll().base_handle().clone())
    };
    let wheel = Wheel::new(&handle);
    let label = match &name {
        Some(name) => format!("Diff of {name}"),
        None => "Diff".to_owned(),
    };
    let head = h_flex()
        .id(SharedString::from(format!("{key}-head")))
        .test_support()
        .items_center()
        .gap(rem(tokens.sp_12))
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_8))
        .bg(tokens.bg_chrome)
        .border_b_1()
        .border_color(tokens.line)
        .children(name.map(|name| div().min_w_0().truncate().font_family(SharedString::from(tokens.font_mono)).child(SharedString::from(name))))
        .child(div().flex_1())
        .child(
            h_flex()
                .gap(rem(tokens.sp_4))
                .typed(tokens.type_small)
                .child(count(format!("{key}-added"), format!("+{}", read.added), tokens.ok))
                .child(count(format!("{key}-removed"), format!("\u{2212}{}", read.removed), tokens.danger)),
        );
    v_flex()
        .id(SharedString::from(key.clone()))
        .test_support()
        .min_w_0()
        .overflow_hidden()
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_sm))
        .child(head)
        .child(
            div()
                .id(SharedString::from(format!("{key}-body")))
                .role(Role::Region)
                .test_support()
                .aria_label(label)
                .relative()
                .h(rem(height))
                .on_scroll_wheel(wheel.keep())
                .child(view)
                .child(wheel.probe()),
        )
        .into_any_element()
}

/// One of the two counts in the header, which says what it is for a reader that cannot see its colour.
fn count(id: String, words: String, colour: gpui_kit::Hsla) -> impl IntoElement {
    div().id(SharedString::from(id)).test_support().aria_label(SharedString::from(words.clone())).text_color(colour).child(SharedString::from(words))
}
