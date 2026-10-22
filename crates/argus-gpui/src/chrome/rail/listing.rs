//! The list under the nav: the project's repositories, or the quiet wait and the empty line when
//! there is no project to list; dimmed while the daemon is gone (`Heard::held`).
//!
//! It scrolls on its own, and the pane on screen is brought into view once per pane
//! (`scroll`), so a person reading the list is not fought for it.

use argus_desktop_core::connection::page::Rail;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context};

use super::row::Ctx;
use super::RailView;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn listing(&self, rail: &Rail, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        // `.rail-list.held`: the tree is dimmed while the daemon is gone.
        let mut list = v_flex()
            .id("rail-list")
            .test_support()
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb(rem(tokens.sp_12))
            // A folded rail starts its tiles a little lower.
            .when(ctx.icons, |list| list.pt(rem(tokens.sp_8)))
            .when(ctx.held, |list| list.opacity(tokens.opacity_dim));
        match &rail.project {
            // The stage is the one loading surface; the rail only waits.
            None if !rail.loaded => list = list.child(div().id("rail-wait").test_support()),
            // A narrow rail has no room for the words.
            None if rail.projects.is_empty() && !ctx.icons => {
                list = list.child(
                    div()
                        .id("rail-empty")
                        .test_support()
                        .p(rem(tokens.sp_16))
                        .text_color(tokens.fg_faint)
                        .child("No projects yet."),
                )
            }
            None => {}
            // The project's name is the brand row's, so the list opens on its first repository.
            Some(project) => {
                list = list.children(project.repositories.iter().enumerate().map(|(at, repo)| self.repository(rail, repo, at == 0, ctx, tokens, cx)));
            }
        }
        list
    }
}
