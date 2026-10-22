//! The note's head (BOARD-CAPTURE-2, -9, -12): the repository and the way back as a crumb, the
//! title, and Discard beside Create, which waits while the note cannot be made or is being made.

use argus_desktop_core::board::capture_note::CapturePage;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use super::CaptureNote;
use crate::board::lenses::forms::view::chord;
use crate::kit::button::{button, off, Look};
use crate::kit::keycap::keycap;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.fhead.capture-head`: the crumbs over the title row, under a hairline.
pub fn head(page: &CapturePage, note: &Entity<CaptureNote>, tokens: &Tokens) -> AnyElement {
    let (leaving, discarding, creating) = (note.clone(), note.clone(), note.clone());
    let mono = SharedString::from(tokens.font_mono);
    let waiting = page.blocked.is_some() || page.creating;
    let create = if waiting {
        off("capture-create", if page.creating { "Creating\u{2026}" } else { "Create" }, false, tokens)
            .child(if page.creating { "Creating\u{2026}" } else { "Create" })
            .child(keycap(chord(), mono, tokens))
            .when_some(page.blocked.clone(), |button, reason| button.tooltip(tip(reason)))
    } else {
        button("capture-create", "Create", Look::Primary, false, tokens)
            .child("Create")
            .child(keycap(chord(), mono, tokens).text_color(tokens.fg_on_accent).border_color(tokens.fg_on_accent.opacity(tokens.share_edge)))
            .on_click(move |_, window, cx| creating.update(cx, |note, cx| note.create(window, cx)))
    };
    v_flex()
        .id("capture-head")
        .test_support()
        .flex_shrink_0()
        .px(rem(tokens.sp_20))
        .py(rem(tokens.sp_8))
        .gap(rem(tokens.sp_8))
        .border_b_1()
        .border_color(tokens.line)
        .child(
            h_flex()
                .items_center()
                .gap(rem(tokens.sp_8))
                .typed(tokens.type_small)
                .text_color(tokens.fg_muted)
                .child(div().child(SharedString::from(page.repository.clone())))
                .child(div().text_color(tokens.fg_faint).child("/"))
                .child(
                    div()
                        .id("capture-crumb")
                        .test_support()
                        .aria_label("Features")
                        .role(gpui_kit::Role::Button)
                        .h(rem(tokens.control_h_sm))
                        .px(rem(tokens.sp_4))
                        .flex()
                        .items_center()
                        .rounded(rem(tokens.r_sm))
                        .cursor_pointer()
                        .hover(|crumb| crumb.bg(tokens.fill).text_color(tokens.fg))
                        .child("Features")
                        .on_click(move |_, window, cx| leaving.update(cx, |note, cx| note.discard(window, cx))),
                ),
        )
        .child(
            h_flex()
                .items_center()
                .gap(rem(tokens.sp_12))
                .child(div().typed(tokens.type_title).text_color(tokens.fg).child("New feature"))
                .child(div().flex_1())
                .child(
                    button("capture-discard", "Discard", Look::Plain, false, tokens)
                        .child("Discard")
                        .tooltip(tip("Discard the note (Esc)"))
                        .on_click(move |_, window, cx| discarding.update(cx, |note, cx| note.discard(window, cx))),
                )
                .child(create),
        )
        .into_any_element()
}
