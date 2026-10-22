//! A repository's row: its chevron, name, status dot and count, the clock that opens its recent
//! sessions, its right-click menu, and the checkouts under it while it is open.
//!
//! A narrow rail draws a hairline between one repository's agents and the next one's instead, its
//! name the tooltip (SHELL-RAIL-33).

use argus_desktop_core::connection::page::{Call, Rail, Repository};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, SharedString};

use super::menus;
use super::popup;
use super::row::{chevron, line, quiet_button, Ctx};
use super::status::{dot_of, status_colour};
use super::RailView;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn repository(&self, rail: &Rail, repo: &Repository, first: bool, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let id = repo.id;
        let chosen = rail.repository == Some(id);
        let body = if ctx.icons {
            self.hairline(repo, first, tokens).into_any_element()
        } else {
            self.heading(repo, chosen, first, ctx, tokens, cx).into_any_element()
        };
        v_flex()
            .child(body)
            // A shut repository draws none of what is under it.
            .when(repo.open, |col| {
                col.children(repo.checkouts.iter().map(|checkout| self.checkout(rail, &repo.name, checkout, ctx, tokens, cx)))
            })
    }

    /// The repository's name row: a button that chooses it, and the clock at its end.
    fn heading(&self, repo: &Repository, chosen: bool, first: bool, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, held) = (repo.id, ctx.held);
        let count = repo.used.clone().or_else(|| (repo.panes > 0).then(|| repo.panes.to_string()));
        let dot = repo.status.and_then(|word| status_colour(word, tokens));
        let fill = tokens.fill;
        let tokens = *tokens;
        // `.repobtn`: muted and semibold, the chosen one in the text colour with an
        // accent chevron, a quiet one faint and regular.
        let colour = if chosen { tokens.fg } else if repo.quiet { tokens.fg_faint } else { tokens.fg_muted };
        let group = SharedString::from(format!("repo-row-{id}"));
        let items = menus::repository(repo, chosen);
        let (rail, heard) = (cx.weak_entity(), self.heard.clone());
        // `.repo` is 2px under the nav, and 8 after another repository's rows.
        h_flex()
            .id(SharedString::from(format!("repo-row-{id}")))
            .test_support()
            .group(group.clone())
            .px(rem(tokens.sp_8))
            .mt(rem(if first { tokens.sp_2 } else { tokens.sp_8 }))
            .items_center()
            .child(
                line(format!("repo-{id}"), tokens.row_h, &tokens)
                    .flex_1()
                    .min_w_0()
                    .px(rem(tokens.sp_4))
                    .gap(rem(tokens.sp_6))
                    .text_size(rem(tokens.fs_sm))
                    .weighted(if repo.quiet { tokens.weight_regular } else { tokens.weight_semibold })
                    .text_color(colour)
                    .aria_selected(chosen)
                    .aria_expanded(repo.open)
                    .aria_label(repo.name.clone())
                    .tooltip(tip(repo.name.clone()))
                    .hover(move |style| style.bg(fill).text_color(tokens.fg))
                    .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::ChooseRepository { repository: id })))
                    // Only a mark: a press on it chooses the repository with the
                    // rest of the row, as the page's does (SHELL-RAIL-5).
                    .child(chevron(format!("repo-{id}-chevron"), repo.open, tokens.icon_sm, if chosen { tokens.accent } else { tokens.fg_faint }, tokens.icon_xs))
                    .child(div().flex_1().min_w_0().truncate().child(repo.name.clone()))
                    .when_some(dot, |row, colour| row.child(dot_of(colour, &tokens)))
                    .when_some(count, |row, count| {
                        row.child(
                            div()
                                .id(format!("repo-{id}-count"))
                                .test_support()
                                .aria_label(count.clone())
                                .text_size(rem(tokens.fs_xs))
                                .weighted(tokens.weight_regular)
                                .text_color(tokens.fg_faint)
                                .child(count),
                        )
                    }),
            )
            // `.repo .icon`: drawn while the row is pointed at.
            .child(
                quiet_button(format!("repo-{id}-sessions"), format!("Recent agent sessions in {}", repo.name), "Recent agent sessions", &tokens)
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::OpenSessions { repository: id })))
                    .child(Glyph::Clock.at(tokens.icon_sm)),
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
