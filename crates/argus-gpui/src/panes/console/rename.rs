//! Naming a console: a modal dialog with one field, asked of the daemon on
//! Enter.
//!
//! The row keeps its old name until the daemon's tree says the new one, so a
//! refused rename leaves it showing what is true (`rename.ts`). The dialog is
//! the overlay host's (`kit/overlay`): it takes the keys, traps Tab, closes on
//! Escape and gives the keys back to what had them when it opened, so this is
//! only the field and the one call. Nothing is sent for an empty name or the
//! old one.

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{Dialog, DialogFooter};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::label::Label;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, Entity, Keystroke, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::overlay::host::{Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::theme::Tokens;

/// The call a name typed asks for: none for a blank name or the old one.
pub fn asked(pane: u64, old: &str, typed: &str) -> Option<Call> {
    let next = typed.trim();
    (!next.is_empty() && next != old).then(|| Call::RenamePane { pane, title: next.to_owned() })
}

/// Opens the dialog over the window, naming `pane`, called `title` now.
pub fn open(pane: u64, title: String, caller: Caller, heard: Entity<Heard>, window: &mut Window, cx: &mut App) {
    let input = cx.new(|cx| InputState::new(window, cx).default_value(title.clone()));
    let host = Overlays::get(cx);
    let closing = host.clone();
    host.open(
        Spec::new(Overlay::Rename),
        move |dialog: Dialog, window, cx, first| {
            // The field has the keys, with the old name selected to type over, once.
            if first {
                input.update(cx, |input, cx| {
                    input.focus(window, cx);
                    input.select_all(window, cx);
                });
            }
            let ask = {
                let (input, caller, heard, title) = (input.clone(), caller.clone(), heard.clone(), title.clone());
                move |cx: &mut App| {
                    let typed = input.read(cx).value();
                    if let Some(call) = asked(pane, &title, &typed) {
                        if !heard.read(cx).held() {
                            caller.call(call);
                        }
                    }
                }
            };
            let confirm = ask.clone();
            let (cancelling, asking) = (closing.clone(), closing.clone());
            dialog
                .title(format!("Rename {title}"))
                .w(gpui_kit::px(Tokens::global(cx).dialog_w))
                .child(v_flex().gap_1().child(Label::new("Name")).child(Input::new(&input)))
                // Enter in the field is the dialog's confirm, as the page's Enter is.
                .on_ok(move |_, _, cx| {
                    confirm(cx);
                    true
                })
                .footer(
                    DialogFooter::new()
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(Tokens::global(cx).fg_muted)
                                .child("An agent may rename its own pane later."),
                        )
                        .child(
                            Button::new("rename-cancel")
                                .child(h_flex().gap_1().child("Cancel").child(Kbd::new(Keystroke::parse("escape").expect("a key"))))
                                .on_click(move |_, window, cx| {
                                    cancelling.close(Overlay::Rename, window, cx);
                                }),
                        )
                        .child(Button::new("rename-ok").primary().label("Rename").on_click(move |_, window, cx| {
                            ask(cx);
                            asking.close(Overlay::Rename, window, cx);
                        })),
                )
        },
        window,
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_or_unchanged_name_asks_for_nothing() {
        assert!(asked(5, "pwsh", "").is_none());
        assert!(asked(5, "pwsh", "   ").is_none());
        assert!(asked(5, "pwsh", "pwsh").is_none());
        assert!(asked(5, "pwsh", " pwsh ").is_none(), "trimmed before it is compared");
    }

    #[test]
    fn a_new_name_is_asked_of_the_daemon_trimmed() {
        let call = asked(5, "pwsh", "  build  ").expect("a rename");
        assert_eq!(format!("{call:?}"), "RenamePane { pane: 5, title: \"build\" }");
    }
}
