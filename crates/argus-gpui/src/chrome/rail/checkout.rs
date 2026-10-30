//! A checkout's line: its branch (or, for a repository's only checkout, the repository's name
//! with the branch after it), its marks, the buttons that appear under the pointer, its right-click
//! menu, and the panes hanging from it while it is open.
//!
//! (`project/Blend.dc.html`, `project/Worktrees.dc.html`.) A repository with one checkout is one
//! line, `Lead::Repo`, its panes directly under it; one with several has a header row
//! (`repository`) and each checkout a mono branch line, `Lead::Branch`, its panes on a hairline
//! guide. The git marks and the shared mark sit at the end; the + that starts a shell or an agent
//! here, the fold of its panes and, on a repository's line, the recent sessions are drawn over them
//! while the row is pointed at, picked or its menu is open. A checkout with no panes is dimmed and
//! says so.
//!
//! A folded rail draws none of the line, only the panes under it (SHELL-RAIL-33).

use argus_desktop_core::chrome::cursor::{Pick, StartFrom};
use argus_desktop_core::connection::page::{Call, Checkout, Rail, Repository};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, SharedString};

use super::menus;
use super::popup;
use super::row::{line, quiet_button, Ctx};
use super::status::{dot_of, status_colour};
use super::RailView;
use crate::kit::icons::Glyph;
use crate::kit::spots::{publish, Spot, Spots};
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// A repository's line is 26px and a checkout's among several 24px, with the air above that tells
/// repositories apart.
pub(super) const REPO_H: f32 = 26.0;
const BRANCH_H: f32 = 24.0;

/// A zero-size marker a test finds by id; it paints nothing.
fn mark(id: String) -> impl IntoElement {
    div().id(id).test_support().size_0()
}

/// How a checkout's line leads.
#[derive(Clone, Copy)]
pub(super) enum Lead {
    /// One of several: the branch in mono behind the branch mark.
    Branch,
    /// A repository's only checkout: the repository's name, then the branch faint.
    Repo { chosen: bool },
}

impl RailView {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn checkout(&self, rail: &Rail, repo: &Repository, lead: Lead, checkout: &Checkout, first: bool, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let id = checkout.id;
        // A narrow rail has no room for the line; its panes keep their tiles.
        let row = (!ctx.icons).then(|| self.checkout_row(rail, repo, lead, checkout, first, ctx, tokens, cx));
        let panes: Vec<_> = checkout.panes.iter().filter(|pane| self.filter.keeps_pane(pane)).map(|pane| self.pane(rail, id, pane, ctx, tokens, cx)).collect();
        let nested = matches!(lead, Lead::Branch);
        v_flex()
            .children(row)
            // A folded checkout draws no pane, whatever it was given. `.tree`: the panes of a branch hang
            // from a hairline under it; a repository's only checkout has them directly under its line.
            .when(checkout.open && !panes.is_empty(), |col| {
                col.child(v_flex().when(!ctx.icons && nested, |nest| nest.ml(rem(17.0)).pl(rem(tokens.sp_4)).border_l_1().border_color(tokens.line)).children(panes))
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn checkout_row(&self, rail: &Rail, repo: &Repository, lead: Lead, checkout: &Checkout, first: bool, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, held) = (checkout.id, ctx.held);
        let picked = rail.cursor.pick == Some(Pick { checkout: id, pane: None });
        let hot = rail.cursor.start.is_some_and(|menu| menu.checkout == id && menu.from == StartFrom::Rail);
        let where_ = if checkout.kind == "main" { "main checkout" } else { "worktree" };
        let branch = if checkout.branch.is_empty() { checkout.name.clone() } else { checkout.branch.clone() };
        let solo = match lead {
            Lead::Repo { .. } => Some(repo),
            Lead::Branch => None,
        };
        let tooltip = match (solo, checkout.branch.is_empty()) {
            (Some(repo), _) => format!("{} \u{b7} {where_}{}", repo.name, if checkout.branch.is_empty() { String::new() } else { format!(" on {}", checkout.branch) }),
            (None, true) => where_.to_owned(),
            (None, false) => format!("{where_} on {}", checkout.branch),
        };
        let hidden_dot = if checkout.open { None } else { checkout.status.and_then(|word| status_colour(word, tokens)) };
        let git = checkout.git.clone().filter(|_| !checkout.clean);
        let empty = checkout.count == 0;
        // `.co.drop-target`: a carried pane would move here.
        let lit = Spots::peek(cx).lit() == Some(id);
        let fill = tokens.fill;
        let mono = cx.theme().mono_font_family.clone();
        let tokens = *tokens;
        let group = SharedString::from(format!("checkout-row-{id}"));
        let items = match solo {
            Some(repo) => menus::solo(repo, matches!(lead, Lead::Repo { chosen: true }), checkout, &rail.templates),
            None => menus::checkout(checkout, &rail.templates),
        };
        let (weak, heard) = (cx.weak_entity(), self.heard.clone());
        let plus_hover = self.plus_hover.clone();
        let menu = hot.then(|| self.start_menu(rail, &repo.name, checkout, &tokens, cx));
        let ring = picked || lit;
        let (solo_id, chosen) = (solo.map(|repo| repo.id), matches!(lead, Lead::Repo { chosen: true }));
        let tools = hot || picked;
        let (height, soft) = (if solo.is_some() { REPO_H } else { BRANCH_H }, tokens.fg_muted.blend(tokens.fg.opacity(0.6)));
        // What leads the line.
        let lead_el = match lead {
            Lead::Repo { chosen } => h_flex()
                .flex_1()
                .min_w_0()
                .gap(rem(8.0))
                .items_baseline()
                .child(
                    div()
                        .id(format!("checkout-{id}-name"))
                        .test_support()
                        .flex_none()
                        .max_w_full()
                        .truncate()
                        .text_size(rem(13.0))
                        .weighted(tokens.weight_semibold)
                        .text_color(if chosen { tokens.fg } else { soft })
                        .child(repo.name.clone()),
                )
                .child(div().flex_1().min_w_0().truncate().font_family(mono.clone()).text_size(rem(11.5)).text_color(tokens.fg_faint).child(branch.clone()))
                .into_any_element(),
            Lead::Branch => h_flex()
                .flex_1()
                .min_w_0()
                .gap(rem(6.0))
                .items_center()
                .child(div().flex_none().text_color(tokens.fg_faint).child(Glyph::Branch.at(tokens.icon_xs)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(mono.clone())
                        .text_size(rem(11.5))
                        .text_color(if empty { tokens.fg_faint } else { tokens.fg_muted })
                        .child(branch.clone()),
                )
                .into_any_element(),
        };
        // What closes it: the marks, drawn in the mono face and faint, not the changed colour, so a rail of
        // busy branches is not a column of orange.
        let end = h_flex()
            .flex_none()
            .gap(rem(6.0))
            .items_center()
            .font_family(mono.clone())
            .text_size(rem(11.5))
            .text_color(tokens.fg_faint)
            // `.shared`: several agents in one checkout, never found out later.
            .when_some(checkout.shared, |end, n| {
                end.child(
                    h_flex()
                        .id(format!("checkout-{id}-shared"))
                        .test_support()
                        .aria_label(format!("shared by {n} agents"))
                        .tooltip(tip(format!("shared by {n} agents")))
                        .gap(rem(tokens.sp_2))
                        .items_center()
                        .text_color(tokens.attention)
                        .child(Glyph::People.at(tokens.icon_xs + 1.0))
                        .child(n.to_string()),
                )
            })
            .when_some(git, |end, git| end.child(git))
            .when(solo.is_none() && empty, |end| end.child(div().id(format!("checkout-{id}-none")).test_support().aria_label("no panes").child("no panes")))
            .when_some(solo.and_then(|repo| repo.used.clone()).filter(|_| empty), |end, used| end.child(format!("\u{b7} {used}")))
            .when(!checkout.open && !empty, |end| {
                let words = format!("{} {} folded away", checkout.count, if checkout.count == 1 { "pane" } else { "panes" });
                end.child(
                    h_flex()
                        .id(format!("checkout-{id}-folded"))
                        .test_support()
                        .aria_label(words.clone())
                        .tooltip(tip(words))
                        .gap(rem(tokens.sp_4))
                        .items_center()
                        .children(hidden_dot.map(|colour| dot_of(colour, &tokens)))
                        .child(checkout.count.to_string()),
                )
            });
        // `.co .icon`: the buttons drawn over the end while the row is pointed at, picked or its menu is open.
        let button_group = group.clone();
        let buttons = h_flex()
            .absolute()
            .top_0()
            .bottom_0()
            .right(rem(tokens.sp_4))
            .items_center()
            .gap(rem(tokens.sp_2))
            .pl(rem(tokens.sp_4))
            .when(!tools, |buttons| buttons.invisible().group_hover(button_group, |style| style.visible()))
            .when_some(solo, |buttons, repo| {
                let repository = repo.id;
                buttons.child(
                    quiet_button(format!("repo-{repository}-sessions"), format!("Recent agent sessions in {}", repo.name), "Recent agent sessions", &tokens)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.ask(held, Call::OpenSessions { repository });
                        }))
                        .child(Glyph::Clock.at(tokens.icon_sm)),
                )
            })
            .when(checkout.count > 0, |buttons| {
                let words = if checkout.open { "Fold its panes away" } else { "Show its panes" };
                let label = format!("{} {} {}", if checkout.open { "Fold" } else { "Unfold" }, repo.name, checkout.name);
                buttons.child(
                    quiet_button(format!("checkout-{id}-fold"), label, words, &tokens)
                        .aria_expanded(checkout.open)
                        // The session moves the pick off a pane about to be folded away.
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.ask(held, Call::ToggleCheckout { checkout: id });
                        }))
                        .child(if checkout.open { Glyph::Down } else { Glyph::Right }.at(tokens.icon_xs)),
                )
            })
            .child(
                quiet_button(format!("checkout-{id}-start"), format!("Start a shell or an agent on {} {}", repo.name, checkout.name), "Start a shell or an agent", &tokens)
                    .aria_expanded(hot)
                    .when(hot, |button| button.bg(tokens.fill).text_color(tokens.accent))
                    .on_hover(move |hovered, _, _| plus_hover.set(*hovered))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.ask(held, Call::ToggleStart { checkout: id, from: StartFrom::Rail });
                    }))
                    .child(Glyph::Plus.at(tokens.icon_sm)),
            );
        div()
            .id(format!("checkout-row-{id}"))
            .relative()
            .when(!first, |row| row.mt(rem(if solo.is_some() { 10.0 } else { 4.0 })))
            .child(
                line(format!("checkout-{id}"), height, &tokens)
                    .group(group.clone())
                    .relative()
                    .w_full()
                    .px(rem(9.0))
                    .gap(rem(8.0))
                    .text_size(rem(tokens.fs_md))
                    .text_color(tokens.fg_muted)
                    .border_1()
                    // `.co.picked`: the ring that says which row holds the keys.
                    .border_color(if ring { tokens.ring } else { gpui_kit::transparent_black() })
                    .when(picked, |row| row.bg(tokens.bg_selected))
                    .when(ring, |row| row.child(mark(format!("checkout-{id}-ring"))))
                    // The row a carried pane would move to, for a test to read.
                    .when(lit, |row| row.bg(tokens.bg_selected).border_color(tokens.accent).child(mark(format!("checkout-{id}-target"))))
                    // Where the carried pane is let go, for the drag to find it.
                    .child(publish(Spot::Checkout(id)))
                    .aria_selected(picked)
                    .aria_label(tooltip.clone())
                    .tooltip(tip(tooltip))
                    .when(!picked, |row| row.hover(move |style| style.bg(fill)))
                    .on_click(cx.listener(move |this, _, _, _| {
                        // A repository's only checkout is both rows: the repository is chosen when it is not
                        // (choosing the chosen one folds it, so it is not asked again), and the checkout picked.
                        if let Some(repository) = solo_id.filter(|_| !chosen) {
                            this.ask(held, Call::ChooseRepository { repository });
                        }
                        this.ask(held, Call::RailPick { checkout: id, pane: None });
                    }))
                    .child(lead_el)
                    .child(div().flex_none().when(tools, |end| end.invisible()).group_hover(group, |style| style.invisible()).child(end))
                    .child(buttons)
                    .context_menu(move |menu, window, cx| popup::fill(menu, &items, &weak, &heard, tokens, window, cx)),
            )
            .children(menu)
    }
}

