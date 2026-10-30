//! A repository in the tree: the one line of a repository with a single checkout (`checkout`), or
//! its header row with the checkouts under it, or the header row alone while it is shut.
//!
//! (`project/Blend.dc.html`, `project/Worktrees.dc.html`.) The header is the repository's name, bold,
//! and at its end how many worktrees it has, or while it is shut how many panes, with the clock that
//! opens its recent sessions under the pointer and its right-click menu. A press chooses it, and
//! choosing the chosen one folds it, as the session decides.
//!
//! A narrow rail draws a hairline between one repository's agents and the next one's instead, its
//! name the tooltip (SHELL-RAIL-33).

use argus_desktop_core::connection::page::{Call, Rail, Repository};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context, SharedString};

use super::checkout::{Lead, REPO_H};
use super::menus;
use super::popup;
use super::row::{line, quiet_button, Ctx};
use super::status::{dot_of, status_colour};
use super::RailView;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn repository(&self, rail: &Rail, repo: &Repository, first: bool, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let chosen = rail.repository == Some(repo.id);
        let kept: Vec<_> = repo.checkouts.iter().filter(|checkout| self.filter.keeps_checkout(checkout)).collect();
        // A repository with one checkout is that checkout's line, so its panes hang straight under the name.
        if let Some(only) = kept.first().filter(|_| repo.open && !ctx.icons && repo.checkouts.len() == 1) {
            return self.checkout(rail, repo, Lead::Repo { chosen }, only, first, ctx, tokens, cx).into_any_element();
        }
        let body = if ctx.icons { self.hairline(repo, first, tokens).into_any_element() } else { self.heading(rail, repo, first, ctx, tokens, cx).into_any_element() };
        v_flex()
            .child(body)
            // A shut repository draws none of what is under it.
            .when(repo.open, |col| col.children(kept.iter().enumerate().map(|(at, checkout)| self.checkout(rail, repo, Lead::Branch, checkout, at == 0, ctx, tokens, cx))))
            .into_any_element()
    }

    /// The repository's name row: a button that chooses it, and the clock at its end.
    fn heading(&self, rail: &Rail, repo: &Repository, first: bool, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, held) = (repo.id, ctx.held);
        let chosen = rail.repository == Some(id);
        let dot = (!repo.open).then(|| repo.status.and_then(|word| status_colour(word, tokens))).flatten();
        let fill = tokens.fill;
        let tokens = *tokens;
        let mono = SharedString::from(tokens.font_mono);
        // The chosen one in the text colour, the others a step under it, a quiet one faint.
        let colour = if chosen {
            tokens.fg
        } else if repo.quiet {
            tokens.fg_faint
        } else {
            tokens.fg_muted.blend(tokens.fg.opacity(0.6))
        };
        let meta = if repo.open {
            (repo.checkouts.len() > 1).then(|| format!("{} worktrees", repo.checkouts.len()))
        } else {
            repo.used.clone().filter(|_| repo.panes == 0).or_else(|| (repo.panes > 0).then(|| format!("{} {}", repo.panes, if repo.panes == 1 { "pane" } else { "panes" })))
        };
        let group = SharedString::from(format!("repo-row-{id}"));
        let items = menus::repository(repo, chosen, rail.project.as_ref());
        let (rail, heard) = (cx.weak_entity(), self.heard.clone());
        h_flex()
            .id(SharedString::from(format!("repo-row-{id}")))
            .test_support()
            .group(group.clone())
            .relative()
            .mt(rem(if first { tokens.sp_2 } else { 10.0 }))
            .items_center()
            .child(
                line(format!("repo-{id}"), REPO_H, &tokens)
                    .flex_1()
                    .min_w_0()
                    .px(rem(10.0))
                    .gap(rem(8.0))
                    .text_size(rem(13.0))
                    .weighted(tokens.weight_semibold)
                    .text_color(colour)
                    .aria_selected(chosen)
                    .aria_expanded(repo.open)
                    .aria_label(repo.name.clone())
                    .tooltip(tip(repo.name.clone()))
                    .hover(move |style| style.bg(fill))
                    .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::ChooseRepository { repository: id })))
                    .child(div().id(format!("repo-{id}-name")).test_support().min_w_0().truncate().child(repo.name.clone()))
                    .child(div().flex_1())
                    .when_some(dot, |row, colour| row.child(dot_of(colour, &tokens)))
                    .when_some(meta, |row, meta| {
                        row.child(
                            div()
                                .id(format!("repo-{id}-count"))
                                .test_support()
                                .aria_label(meta.clone())
                                .flex_none()
                                .font_family(mono)
                                .text_size(rem(11.5))
                                .weighted(tokens.weight_regular)
                                .text_color(tokens.fg_faint)
                                .child(meta),
                        )
                    })
                    // Only a mark: a press on it chooses the repository with the rest of the row.
                    .when(!repo.open, |row| row.child(div().id(format!("repo-{id}-chevron")).test_support().flex_none().text_color(tokens.fg_faint).child(Glyph::Right.at(tokens.icon_xs)))),
            )
            // `.repo .icon`: drawn over the end while the row is pointed at.
            .child(
                h_flex()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(rem(tokens.sp_4))
                    .items_center()
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .child(
                        quiet_button(format!("repo-{id}-sessions"), format!("Recent agent sessions in {}", repo.name), "Recent agent sessions", &tokens)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.ask(held, Call::OpenSessions { repository: id });
                            }))
                            .child(Glyph::Clock.at(tokens.icon_sm)),
                    ),
            )
            .context_menu(move |menu, window, cx| popup::fill(menu, &items, &rail, &heard, tokens, window, cx))
    }

    /// `.repo` in a folded rail: a hairline, 12px high, between this repository's agents and the
    /// last one's. The first has none above it.
    fn hairline(&self, repo: &Repository, first: bool, tokens: &Tokens) -> impl IntoElement {
        div()
            .id(format!("repo-{}", repo.id))
            .test_support()
            .aria_label(repo.name.clone())
            .tooltip(tip(repo.name.clone()))
            .h(rem(tokens.sp_12))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .when(first, |row| row.hidden())
            .px(rem(tokens.sp_12))
            .child(div().w_full().h(rem(tokens.hairline)).bg(tokens.line))
    }
}
