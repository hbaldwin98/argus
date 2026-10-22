//! Naming the branch of a new worktree: a dialog with one mono field, `Off <branch>` in its foot,
//! asked of a checkout row (`ck/stage.ts`, `nameDialog`).
//!
//! The window holds this dialog, as the page does: Enter in the field or `Create` sends
//! `NewWorktree` with the text as it stands and closes it, and Escape, `Cancel`, the close button
//! and the scrim close it sending nothing. An empty name is sent too: the session refuses it with
//! a notice, which is Rust's to say (`a new worktree needs a branch name`).

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, Entity, SharedString, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::button::{button, Look};
use crate::kit::dialog::{esc, field_label, foot, head};
use crate::kit::overlay::host::{Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// Opens the dialog over the window, a new branch `off` the branch named.
pub fn open(off: String, caller: Caller, heard: Entity<Heard>, window: &mut Window, cx: &mut App) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("desktop-checkouts"));
    let host = Overlays::get(cx);
    let closing = host.clone();
    host.open(
        Spec::new(Overlay::Worktree),
        move |dialog, window, cx, first| {
            // The field has the keys, as the page focuses it on opening.
            if first {
                input.update(cx, |input, cx| input.focus(window, cx));
            }
            let tokens = *Tokens::global(cx);
            let create = {
                let (input, caller, heard) = (input.clone(), caller.clone(), heard.clone());
                move |cx: &mut App| {
                    if !heard.read(cx).held() {
                        caller.call(Call::NewWorktree { name: Some(input.read(cx).value().to_string()) });
                    }
                }
            };
            let (confirm, creating, closed) = (create.clone(), create, closing.clone());
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
                        .id("worktree")
                        .child(head("worktree", SharedString::from("New worktree"), move |window, cx| {
                            close.close(Overlay::Worktree, window, cx);
                        }, None, &tokens))
                        .child(
                            v_flex()
                                .gap(rem(tokens.sp_8))
                                .px(rem(tokens.sp_24))
                                .pb(rem(tokens.sp_24))
                                .child(field_label("Branch", &tokens))
                                .child(div().font_family(SharedString::from(tokens.font_mono)).child(Input::new(&input).id("worktree-name").aria_label("Branch"))),
                        )
                        .child(
                            foot(&tokens)
                                .child(
                                    h_flex()
                                        .id("worktree-off")
                                        .test_support()
                                        .aria_label(SharedString::from(format!("Off {off}")))
                                        .flex_1()
                                        .min_w_0()
                                        .gap(rem(tokens.sp_4))
                                        .typed(tokens.type_small)
                                        .text_color(tokens.fg_muted)
                                        .child("Off")
                                        .child(div().font_family(SharedString::from(tokens.font_mono)).truncate().child(SharedString::from(off.clone()))),
                                )
                                .child(
                                    button("worktree-cancel", "Cancel", Look::Plain, false, &tokens)
                                        .child("Cancel")
                                        .child(esc(&tokens))
                                        .on_click(move |_, window, cx| {
                                            cancel.close(Overlay::Worktree, window, cx);
                                        }),
                                )
                                .child(button("worktree-create", "Create", Look::Primary, false, &tokens).child("Create").on_click(move |_, window, cx| {
                                    creating(cx);
                                    closed.close(Overlay::Worktree, window, cx);
                                })),
                        ),
                )
        },
        window,
        cx,
    );
}
