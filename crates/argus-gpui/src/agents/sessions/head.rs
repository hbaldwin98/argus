//! The dialog's head: what it lists, the search field, the Filter button and the close button
//! (SESSIONS-1, -8, -9, -23).

use argus_desktop_core::agents::sessions_dialog::SessionsPage;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Context, SharedString, TestSupportExt as _};

use super::SessionsView;
use crate::agents::ask::ask;
use crate::agents::search::filter;
use crate::kit::dialog::sheet_head;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl SessionsView {
    /// `.dhead.review-head` in a sheet dialog: the title and what it lists, the search and the
    /// Filter at the right, and the ✕, which is labelled by what it closes.
    pub(super) fn head(&self, page: &SessionsPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (heard, caller) = (self.heard.clone(), self.caller.clone());
        let title = h_flex()
            .gap(rem(tokens.sp_12))
            .items_center()
            .min_w_0()
            .child(div().id("sessions-title").test_support().aria_label(SharedString::from(format!("Recent sessions \u{b7} {}", page.repository))).flex_shrink_0().child(SharedString::from(format!("Recent sessions \u{b7} {}", page.repository))))
            .child(div().flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child("newest 100, every checkout"));
        let aside = h_flex().gap(rem(tokens.sp_12)).items_center().child(filter("sessions-search", "Search sessions by title, harness, checkout or id", &self.search, tokens)).child(self.filter_button(page, tokens, cx));
        sheet_head(
            "sessions",
            title,
            aside,
            "Close the recent sessions",
            move |_, cx| ask(&heard, &caller, Call::SessionsAction { action: Action::CloseLayer, row: None }, cx),
            None,
            tokens,
        )
        .into_any_element()
    }
}

