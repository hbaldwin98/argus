//! The kit's menu for the items the strip offers: each item sends its calls through the caller,
//! shows the keys it names, and is disabled while the daemon is gone.

use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{prelude::*, App, Entity, SharedString};

use super::rename;
use super::strip::{Does, Offer};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::keycap::keycap;
use crate::theme::Tokens;

/// The keys an item names, each press a cap, the alternatives apart by a slash.
fn keycaps(keys: &[Vec<String>], tokens: &Tokens) -> impl IntoElement {
    let mut caps = h_flex().gap_1().items_center().text_xs().text_color(tokens.fg_muted);
    let mono = SharedString::from(tokens.font_mono);
    for (n, presses) in keys.iter().enumerate() {
        if n > 0 {
            caps = caps.child("/");
        }
        for press in presses {
            caps = caps.child(keycap(press.clone(), mono.clone(), tokens));
        }
    }
    caps
}

/// The kit's menu for `offers`: each item sends its calls through `caller`,
/// and `Rename...` opens the dialog once the menu has let go of the keys, so
/// the dialog gives them back to what had them before the menu. While the
/// daemon is gone every item is disabled, and a click that still arrives
/// sends nothing.
pub(super) fn build(mut menu: PopupMenu, offers: &[Offer], caller: &Caller, heard: &Entity<Heard>, tokens: Tokens, cx: &App) -> PopupMenu {
    let held = heard.read(cx).held();
    for offer in offers {
        menu = match offer {
            Offer::Heading(text) => menu.label(text.clone()),
            Offer::Separator => menu.separator(),
            Offer::Item { label, does, danger, here, keys } => {
                let (caller, heard, does) = (caller.clone(), heard.clone(), does.clone());
                let label = SharedString::from(label.clone());
                let item = if *danger || !keys.is_empty() {
                    let (shown, keys) = (label.clone(), keys.clone());
                    let colour = if *danger { tokens.danger } else { tokens.fg };
                    PopupMenuItem::element(move |_, _| {
                        h_flex()
                            .w_full()
                            .gap_4()
                            .justify_between()
                            .text_color(colour)
                            .child(shown.clone())
                            .child(keycaps(&keys, &tokens))
                    })
                } else {
                    PopupMenuItem::new(label)
                };
                menu.item(item.checked(*here).disabled(*here || held).on_click(move |_, window, cx| {
                    if heard.read(cx).held() {
                        return;
                    }
                    match &does {
                        Does::Calls(calls) => calls.iter().for_each(|call| caller.call(call.clone())),
                        Does::Rename { pane, title } => {
                            let (pane, title, caller, heard) = (*pane, title.clone(), caller.clone(), heard.clone());
                            window.defer(cx, move |window, cx| rename::open(pane, title, caller, heard, window, cx));
                        }
                    }
                }))
            }
        };
    }
    menu
}
