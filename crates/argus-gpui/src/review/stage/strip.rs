//! The strip above the tab: the branch, the tabs, the files' count and the refresh and close
//! buttons, and the filter field while History is the tab.

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::{h_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Role, SharedString};

use super::GitStage;
use crate::kit::tip::tip;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::segment;
use crate::review::entry::Tab;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl GitStage {
    pub(super) fn tab(&self, id: &'static str, label: &'static str, count: Option<usize>, tab: Tab, on: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        segment::pill(id, Role::Tab, on, &tokens)
            .child(label)
            .when_some(count, |pill, n| pill.child(segment::count(n.to_string(), &tokens)))
            .on_click(cx.listener(move |this, _, window, cx| {
                // A pill takes focus when clicked, and the tab's keys are heard
                // only on the stage itself.
                window.focus(&this.focus, cx);
                this.show(tab, cx)
            }))
    }

    /// The strip above the tab: the branch, the tabs, the files' count and the
    /// two buttons.
    pub(super) fn strip(&self, tab: Tab, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let heard = self.heard.read(cx);
        let (review, history) = (heard.review.clone(), heard.history.clone());
        let branch = match tab {
            Tab::Changes => review.as_ref().map(|page| (page.branch.clone(), page.name.clone())),
            Tab::History => history.as_ref().map(|page| (page.branch.clone(), page.name.clone())),
        };
        let files = review.as_ref().map(|page| page.files.len());
        let changes_on = tab == Tab::Changes;
        let mono = cx.theme().mono_font_family.clone();
        let close_label = if changes_on { "Close the review" } else { "Close the history" };
        h_flex()
            .id("git-strip")
            .flex_shrink_0()
            .h(rem(tokens.sp_44))
            .px(rem(tokens.sp_24))
            .gap(rem(tokens.sp_12))
            .items_center()
            .border_b_1()
            .border_color(tokens.line)
            // `.chip-branch`: the branch once, as a pill, in the reference colour.
            .when_some(branch, |strip, (branch, name)| {
                strip.child(
                    h_flex()
                        .id("git-branch")
                        .test_support()
                        .aria_label(SharedString::from(name.clone()))
                        .tooltip(tip(name.clone()))
                        .flex_shrink_0()
                        .h(rem(tokens.icon_btn))
                        .px(rem(tokens.sp_8))
                        .gap(rem(tokens.sp_6))
                        .items_center()
                        .rounded(rem(tokens.r_pill))
                        .border_1()
                        .border_color(tokens.line_strong)
                        .text_color(tokens.ref_)
                        .font_family(mono)
                        .text_size(rem(tokens.fs_md))
                        .child(Glyph::Branch.at(14.0))
                        .child(SharedString::from(branch)),
                )
            })
            .child(
                segment::track("git-tabs", "Git view", Role::TabList, tokens)
                    .child(self.tab("git-tab-changes", "Changes", files, Tab::Changes, changes_on, cx))
                    .child(self.tab("git-tab-history", "History", None, Tab::History, !changes_on, cx)),
            )
            // `.meta`: what the history lists, beside the tabs as the page's head says it.
            .when(!changes_on && history.is_some(), |strip| {
                strip.child(div().typed(tokens.type_small).text_color(tokens.fg_muted).child("newest 100 on HEAD"))
            })
            .child(div().flex_1())
            .when(!changes_on && history.is_some(), |strip| strip.child(self.filter.clone()))
            .when_some(review.as_ref().filter(|page| changes_on && !page.files.is_empty()), |strip, page| {
                let plural = if page.files.len() == 1 { "" } else { "s" };
                strip.child(
                    h_flex()
                        .id("review-count")
                        .test_support()
                        .aria_label(SharedString::from(format!(
                            "{} file{plural} \u{b7} +{} \u{2212}{}",
                            page.files.len(),
                            page.added,
                            page.removed
                        )))
                        .gap(rem(tokens.sp_4))
                        .typed(tokens.type_small)
                        .text_color(tokens.fg_muted)
                        .child(SharedString::from(format!("{} file{plural} \u{b7}", page.files.len())))
                        .child(div().text_color(tokens.ok).child(SharedString::from(format!("+{}", page.added))))
                        .child(div().text_color(tokens.danger).child(SharedString::from(format!("\u{2212}{}", page.removed)))),
                )
            })
            .when(changes_on && review.is_some(), |strip| {
                strip.child(
                    icon_button("review-refresh", "Refresh", false, tokens)
                        .tooltip(tip("Refresh"))
                        .child(Glyph::Refresh.at(14.0))
                        .on_click(cx.listener(|this, _, window, cx| {
                            // A button takes focus when clicked, and the review's keys
                            // are heard only on the stage itself.
                            window.focus(&this.focus, cx);
                            this.ask(Call::ReviewRefresh, cx)
                        })),
                )
            })
            .child(
                icon_button("git-close", close_label, false, tokens)
                    .tooltip(tip(close_label))
                    .child(Glyph::Close.at(14.0))
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            )
    }
}
