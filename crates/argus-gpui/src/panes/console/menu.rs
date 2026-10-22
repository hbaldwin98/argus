//! The kit's menu for the items the strip offers: each sends its calls through the caller, shows
//! the keys it names, and is disabled while the daemon is gone.

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{Context, Entity, Window};

use super::rename;
use super::strip::{Does, Offers};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::menu::popup;
use crate::theme::Tokens;

/// The kit's menu for `offers`: each choice sends its calls through `caller`, and `Rename...`
/// opens the dialog once the menu has let go of the keys (`popup::fill` runs a choice after the
/// menu has gone), so the dialog gives them back to what had them before the menu. While the
/// daemon is gone every choice is disabled, and a click that still arrives sends nothing.
pub(super) fn build(menu: PopupMenu, offers: &Offers, caller: &Caller, heard: &Entity<Heard>, tokens: Tokens, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let (caller, asking, opening) = (caller.clone(), heard.clone(), heard.clone());
    popup::fill(
        menu,
        offers,
        tokens,
        move |cx| asking.read(cx).held(),
        move |does, window, cx| match does {
            Does::Calls(calls) => calls.iter().for_each(|call| caller.call(call.clone())),
            Does::Rename { pane, title } => rename::open(*pane, title.clone(), caller.clone(), opening.clone(), window, cx),
        },
        window,
        cx,
    )
}
