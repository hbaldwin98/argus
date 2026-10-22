//! The list under the nav: the project's repositories, or the quiet wait and the empty line when
//! there is no project to list; dimmed while the daemon is gone (`Heard::held`).

use argus_desktop_core::connection::page::{Rail, Stage};
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context};

use super::RailView;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn listing(&self, rail: &Rail, held: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        // `.rail-list.held`: the tree is dimmed while the daemon is gone.
        let mut list = v_flex()
            .id("rail-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb(px(tokens.sp_12))
            .when(held, |list| list.opacity(tokens.opacity_dim));
        let offstage = rail.stage != Stage::Workspace || self.heard.read(cx).git_up();
        match &rail.project {
            // The stage is the one loading surface; the rail only waits.
            None if !rail.loaded => list = list.child(div().id("rail-wait").test_support()),
            None if rail.projects.is_empty() => {
                list = list.child(
                    div()
                        .id("rail-empty")
                        .test_support()
                        .p(px(tokens.sp_16))
                        .text_color(tokens.fg_faint)
                        .child("No projects yet."),
                )
            }
            None => {}
            // The project's name is the brand row's, so the list opens on its first repository.
            Some(project) => {
                list = list.children(project.repositories.iter().enumerate().map(|(at, repo)| {
                    self.repository(rail, repo, at == 0, offstage, held, tokens, cx)
                }));
            }
        }
        list
    }
}
