//! The dialog that asks why a feature is held (`holdDialog` of `feature_stage.ts`): a title, one
//! field with the keys, a note that agents read the reason, and Cancel and Hold.
//!
//! Enter in the field or Hold sends `Call::Hold` with the text as typed and closes it; Escape,
//! Cancel, the close button and a press on the scrim close it sending nothing. An empty reason is
//! sent too: Rust turns it into lifting the hold (BOARD-MENU-8), which is the page's behaviour.

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::{prelude::*, App, SharedString, Window};

use crate::board::ctx::Ctx;
use crate::kit::button::{button, Look};
use crate::kit::dialog::{esc, field_label, foot, head, why};
use crate::kit::overlay::host::{Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// Opens the dialog over the window for `slug`, called `#number title`.
pub fn open(slug: String, number: i64, title: String, ctx: Ctx, window: &mut Window, cx: &mut App) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("until the ratatui bump lands"));
    let host = Overlays::get(cx);
    let closing = host.clone();
    host.open(
        Spec::new(Overlay::Hold),
        move |dialog, window, cx, first| {
            // The field has the keys, as the page focuses it on opening.
            if first {
                input.update(cx, |input, cx| input.focus(window, cx));
            }
            let tokens = *Tokens::global(cx);
            let hold = {
                let (input, ctx, slug) = (input.clone(), ctx.clone(), slug.clone());
                move |cx: &mut App| ctx.send(Call::Hold { slug: slug.clone(), reason: Some(input.read(cx).value().to_string()) }, cx)
            };
            let (confirm, holding) = (hold.clone(), hold);
            let (cancel, close) = (closing.clone(), closing.clone());
            dialog
                .w(pixels(tokens.dialog_w, window.rem_size()))
                .p_0()
                // Enter in the field is the dialog's confirm, as the page's `writing` place makes it.
                .on_ok(move |_, _, cx| {
                    confirm(cx);
                    true
                })
                .child(
                    v_flex()
                        .id("hold")
                        .child(head("hold", SharedString::from(format!("Hold #{number} {title}")), move |window, cx| {
                            close.close(Overlay::Hold, window, cx);
                        }, None, &tokens))
                        .child(
                            v_flex()
                                .gap(rem(tokens.sp_8))
                                .px(rem(tokens.sp_24))
                                .pb(rem(tokens.sp_24))
                                .child(field_label("Why is it held?", &tokens))
                                .child(Input::new(&input).id("hold-reason").aria_label("Why is it held?")),
                        )
                        .child(
                            foot(&tokens)
                                .child(why("Agents read the reason.", &tokens))
                                .child(
                                    button("hold-cancel", "Cancel", Look::Plain, false, &tokens)
                                        .child("Cancel")
                                        .child(esc(&tokens))
                                        .on_click(move |_, window, cx| {
                                            cancel.close(Overlay::Hold, window, cx);
                                        }),
                                )
                                .child({
                                    let closing = closing.clone();
                                    button("hold-confirm", "Hold", Look::Primary, false, &tokens).child("Hold").on_click(move |_, window, cx| {
                                        holding(cx);
                                        closing.close(Overlay::Hold, window, cx);
                                    })
                                }),
                        ),
                )
        },
        window,
        cx,
    );
}

