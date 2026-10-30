//! The list under the chips: the Extensions and Views lists when the strip opened them, the
//! project's repositories as the filter keeps them, the quiet-repositories row at the foot, or the
//! quiet wait and the empty line when there is no project to list; dimmed while the daemon is gone
//! (`Heard::held`).
//!
//! It scrolls on its own, and the pane on screen is brought into view once per pane
//! (`scroll`), so a person reading the list is not fought for it.

use argus_desktop_core::connection::page::Rail;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context};

use super::filter::{listed, Filter};
use super::row::Ctx;
use super::slice::Slice;
use super::RailView;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn listing(&self, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let rail: &Rail = &slice.rail;
        // `.rail-list.held`: the tree is dimmed while the daemon is gone.
        let mut list = v_flex()
            .id("rail-list")
            .test_support()
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(rem(if ctx.icons { 0.0 } else { 6.0 }))
            .pb(rem(tokens.sp_12))
            // A folded rail starts its tiles a little lower.
            .when(ctx.icons, |list| list.pt(rem(tokens.sp_8)).items_center())
            .when(ctx.held, |list| list.opacity(tokens.opacity_dim));
        match &rail.project {
            // The stage is the one loading surface; the rail only waits.
            None if !rail.loaded => list = list.child(div().id("rail-wait").test_support()),
            // A narrow rail has no room for the words.
            None if rail.projects.is_empty() && !ctx.icons => {
                list = list.child(div().id("rail-empty").test_support().p(rem(tokens.sp_16)).text_color(tokens.fg_faint).child("No projects yet."))
            }
            None => {}
            // The project's name is the header's, so the list opens on its first repository.
            Some(project) => {
                let shown = listed(project, rail.repository, self.filter);
                if !ctx.icons {
                    list = list.child(self.tail(slice, ctx, tokens, cx));
                }
                let none = shown.shown.is_empty() && shown.quiet.is_empty();
                let rows: Vec<AnyElement> = shown.shown.iter().enumerate().map(|(at, repo)| self.repository(rail, repo, at == 0, ctx, tokens, cx)).collect();
                list = list.children(rows).children(self.quiet(&shown.quiet, rail, ctx, tokens, cx));
                if none && !ctx.icons {
                    list = list.child(
                        div()
                            .id("rail-filter-none")
                            .test_support()
                            .px(rem(10.0))
                            .py(rem(tokens.sp_12))
                            .text_size(rem(tokens.fs_sm))
                            .text_color(tokens.fg_faint)
                            .child(match self.filter {
                                Filter::NeedsYou => "Nothing needs you.",
                                Filter::Working => "Nothing is working.",
                                Filter::All => "Nothing here yet.",
                            }),
                    );
                }
                // A folded rail has no room for the words; the repository's menu still adds one.
                if !ctx.icons {
                    list = list.child(self.add_repository(project, ctx, tokens, cx));
                }
            }
        }
        list
    }
}
