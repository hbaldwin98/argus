//! A repository's row: its chevron, name, status dot and count, and the checkouts under it while
//! it is open.

use argus_desktop_core::connection::page::{Call, Rail, Repository};
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context};

use super::RailView;
use super::row::{chevron, line};
use super::status::{dot_of, status_colour};
use crate::style::Typed as _;
use crate::theme::Tokens;

impl RailView {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn repository(
        &self,
        rail: &Rail,
        repo: &Repository,
        first: bool,
        offstage: bool,
        held: bool,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = repo.id;
        let chosen = rail.repository == Some(id);
        let count = repo.used.clone().or_else(|| (repo.panes > 0).then(|| repo.panes.to_string()));
        let dot = repo.status.and_then(|word| status_colour(word, tokens));
        let fill = tokens.fill;
        // `.repobtn`: muted and semibold, the chosen one in the text colour with an
        // accent chevron, a quiet one faint and regular.
        let colour = if chosen { tokens.fg } else if repo.quiet { tokens.fg_faint } else { tokens.fg_muted };
        v_flex()
            // `repo.divider` is not drawn: the page never draws it (SHELL-RAIL-2).
            // `.repo` is 2px under the nav, and 8 after another repository's rows.
            .child(
                div().px(px(tokens.sp_8)).mt(px(if first { tokens.sp_2 } else { tokens.sp_8 })).child(
                    line(format!("repo-{id}"), tokens.row_h, tokens)
                        .w_full()
                        .px(px(tokens.sp_4))
                        .gap(px(tokens.sp_6))
                        .text_size(px(tokens.fs_sm))
                        .weighted(if repo.quiet { tokens.weight_regular } else { tokens.weight_semibold })
                        .text_color(colour)
                        .aria_selected(chosen)
                        .aria_expanded(repo.open)
                        .aria_label(repo.name.clone())
                        .hover(move |style| style.bg(fill).text_color(tokens.fg))
                        .on_click(cx.listener(move |this, _, _, _| {
                            this.ask(held, Call::ChooseRepository { repository: id })
                        }))
                        // Only a mark: a press on it chooses the repository with the
                        // rest of the row, as the page's does (SHELL-RAIL-5).
                        .child(chevron(
                            format!("repo-{id}-chevron"),
                            repo.open,
                            tokens.icon_sm,
                            if chosen { tokens.accent } else { tokens.fg_faint },
                        ))
                        .child(div().flex_1().min_w_0().truncate().child(repo.name.clone()))
                        .when_some(dot, |row, colour| row.child(dot_of(colour, tokens)))
                        .when_some(count, |row, count| {
                            row.child(
                                div()
                                    .id(format!("repo-{id}-count"))
                                    .test_support()
                                    .aria_label(count.clone())
                                    .text_size(px(tokens.fs_xs))
                                    .weighted(tokens.weight_regular)
                                    .text_color(tokens.fg_faint)
                                    .child(count),
                            )
                        }),
                ),
            )
            // A shut repository draws none of what is under it.
            .when(repo.open, |col| {
                col.children(repo.checkouts.iter().map(|checkout| self.checkout(checkout, offstage, held, tokens, cx)))
            })
    }
}
