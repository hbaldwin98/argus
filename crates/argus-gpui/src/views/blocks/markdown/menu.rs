//! The menu a right-click on selected text opens: `Copy` under `Selection` (SHELL-MENU-5), asked of
//! Rust. With nothing selected there is no menu, so a right-click opens nothing.

use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::{prelude::*, AnyElement, Context};

use super::Markdown;
use crate::kit::menu::{popup, selection};
use crate::theme::Tokens;

impl Markdown {
    /// `body`, with the Selection menu on a right-click while text is selected. The kit builds the
    /// menu when it is pressed, so what it offers to copy is what is selected then.
    pub(super) fn offer_copy<E>(&self, body: E, cx: &Context<Self>) -> AnyElement
    where
        E: InteractiveElement + ParentElement + Styled + IntoElement + 'static,
    {
        if self.selection.is_none() {
            return body.into_any_element();
        }
        let (markdown, caller) = (cx.entity(), self.caller.clone());
        body.context_menu(move |menu, window, cx| {
            let Some(items) = markdown.read(cx).selection_text().and_then(|text| selection::items(&text)) else {
                return menu;
            };
            let tokens = *Tokens::global(cx);
            let caller = caller.clone();
            // The daemon is not what a copy asks of, so no link being down holds it off.
            popup::fill(menu, &items, tokens, |_| false, move |call, _, _| caller.call(call.clone()), window, cx)
        })
        .into_any_element()
    }
}
