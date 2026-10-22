//! The row an extension's view carries as the stage's page: whose it is, the view-wide actions as
//! small buttons with their key caps, and the way back into a dock (EXTVIEW-23).
//!
//! The strip above already names the view. Docked, the dock's own head or tab strip holds the
//! title and the tools (`layout/tools`), so a docked view draws none of this.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::extensions::row_actions::wide;
use argus_desktop_core::extensions::views::ExtTable;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context, SharedString};

use super::actions::wide_button;
use super::ExtensionView;
use crate::keys::caps::presses;
use crate::kit::button::{button, Look};
use crate::kit::keycap::keycaps;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl ExtensionView {
    pub(super) fn page_head(&self, view: &ExtTable, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let actions: Vec<AnyElement> = wide(view, true).into_iter().map(|action| wide_button(view, action, &self.caller, tokens).into_any_element()).collect();
        let keys = presses(&self.heard.read(cx).keymap, Action::ExtensionDock, Place::Extension);
        let (caller, panel) = (self.caller.clone(), view.panel.clone());
        let dock = button(SharedString::from(format!("ext-dock-{}", view.panel)), "Dock", Look::Plain, true, tokens)
            .child("Dock")
            .children((!keys.is_empty()).then(|| keycaps(&keys, tokens)))
            .on_click(move |_, _, _| caller.call(Call::ExtensionMode { panel: panel.clone(), page: false }));
        h_flex()
            .id(SharedString::from(format!("ext-page-head-{}", view.panel)))
            .test_support()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .gap(rem(tokens.sp_8))
            .px(rem(tokens.sp_16))
            .py(rem(tokens.sp_12))
            .border_b_1()
            .border_color(tokens.line)
            .child(div().typed(tokens.type_small).text_size(rem(tokens.fs_xs)).text_color(tokens.fg_faint).child(SharedString::from(format!("from {}", view.extension))))
            .child(h_flex().flex_shrink_0().items_center().gap(rem(tokens.sp_8)).ml_auto().children(actions).child(dock))
            .into_any_element()
    }
}
