//! The menu a right-click on selected text in the diff opens: `Copy` under `Selection`
//! (SHELL-MENU-5).
//!
//! The kit's context menu is built when it is pressed, so the text it offers to copy is what is
//! selected then. Choosing it tells the owner (`DiffEvent::CopySelection`), which sends the text to
//! Rust, as Ctrl+C does; with nothing selected the diff has no menu, so a right-click on it opens
//! nothing.

use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::{prelude::*, AnyElement, Context};

use super::{DiffEvent, DiffView};
use crate::kit::menu::{popup, selection};
use crate::theme::Tokens;

impl DiffView {
    /// `body`, with the Selection menu on a right-click while text is selected.
    pub(super) fn offer_copy<E>(&self, body: E, cx: &Context<Self>) -> AnyElement
    where
        E: InteractiveElement + ParentElement + Styled + IntoElement + 'static,
    {
        if self.selection.is_none() {
            return body.into_any_element();
        }
        let diff = cx.entity();
        body.context_menu(move |menu, window, cx| {
            let Some(items) = diff.read(cx).selection_text().and_then(|text| selection::items(&text)) else {
                return menu;
            };
            let tokens = *Tokens::global(cx);
            let asked = diff.clone();
            // The daemon is not what a copy asks of, so no link being down holds it off.
            popup::fill(menu, &items, tokens, |_| false, move |_, _, cx| asked.update(cx, |_, cx| cx.emit(DiffEvent::CopySelection)), window, cx)
        })
        .into_any_element()
    }
}
