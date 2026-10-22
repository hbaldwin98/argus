//! A checkout's row: its fold, branch, shared and git marks, the + that starts something on it,
//! the pick's ring, its right-click menu, and the panes hanging from it while it is open.
//!
//! A narrow rail draws none of the row, only the panes under it (SHELL-RAIL-33).

use argus_desktop_core::chrome::cursor::{Pick, StartFrom};
use argus_desktop_core::connection::page::{Call, Checkout, Rail};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Hsla, Role, SharedString};

use super::menus;
use super::popup;
use super::row::{chevron, line, quiet_button, Ctx};
use super::status::{dot_of, status_colour};
use super::RailView;
use crate::kit::icons::Glyph;
use crate::kit::spots::{publish, Spot, Spots};
use crate::kit::tip::tip;
use crate::theme::Tokens;

/// A zero-size marker a test finds by id; it paints nothing.
fn mark(id: String) -> impl IntoElement {
    div().id(id).test_support().size_0()
}

impl RailView {
    pub(super) fn checkout(&self, rail: &Rail, repository: &str, checkout: &Checkout, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let id = checkout.id;
        // A narrow rail has no room for the row; its panes keep their tiles.
        let row = (!ctx.icons).then(|| self.checkout_row(rail, repository, checkout, ctx, tokens, cx));
        v_flex()
            .children(row)
            // A folded checkout draws no pane, whatever it was given. `.nest`: its
            // panes hang from a hairline under the checkout's own branch.
            .when(checkout.open, |col| {
                col.child(
                    v_flex()
                        .when(!ctx.icons, |nest| {
                            nest.ml(px(tokens.pane_indent))
                                .mr(px(tokens.sp_8))
                                .mb(px(tokens.sp_4))
                                .pl(px(tokens.sp_4))
                                .border_l_1()
                                .border_color(tokens.line)
                        })
                        .children(checkout.panes.iter().map(|pane| self.pane(rail, id, pane, ctx, tokens, cx))),
                )
            })
    }

    fn checkout_row(&self, rail: &Rail, repository: &str, checkout: &Checkout, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, held) = (checkout.id, ctx.held);
        let picked = rail.cursor.pick == Some(Pick { checkout: id, pane: None });
        let hot = rail.cursor.start.is_some_and(|menu| menu.checkout == id && menu.from == StartFrom::Rail);
        let where_ = if checkout.kind == "main" { "main checkout" } else { "worktree" };
        let tooltip = if checkout.branch.is_empty() { where_.to_owned() } else { format!("{where_} on {}", checkout.branch) };
        let hidden_dot = if checkout.open { None } else { checkout.status.and_then(|word| status_colour(word, tokens)) };
        let git = checkout.git.clone().filter(|_| !checkout.clean);
        // `.co.drop-target`: a carried pane would move here.
        let lit = Spots::peek(cx).lit() == Some(id);
        let fill = tokens.fill;
        let mono = cx.theme().mono_font_family.clone();
        let tokens = *tokens;
        let group = SharedString::from(format!("checkout-row-{id}"));
        let items = menus::checkout(checkout, &rail.templates);
        let (weak, heard) = (cx.weak_entity(), self.heard.clone());
        let plus_hover = self.plus_hover.clone();
        let menu = hot.then(|| self.start_menu(rail, repository, checkout, &tokens, cx));
        let ring = picked || lit;
        div()
            .id(format!("checkout-row-{id}"))
            .relative()
            // `.co`: a margin of 8 each side, the fold chevron in its 12px of left padding.
            .mx(px(tokens.sp_8))
            .child(
                line(format!("checkout-{id}"), tokens.control_h_sm, &tokens)
                    .group(group.clone())
                    .w_full()
                    .pl(px(tokens.sp_12 - 1.0))
                    .pr(px(tokens.sp_4 - 1.0))
                    .gap(px(tokens.sp_6))
                    .text_size(px(tokens.fs_xs))
                    .text_color(tokens.fg_faint)
                    .border_1()
                    // `.co.picked`: the ring that says which row holds the keys.
                    .border_color(if ring { tokens.ring } else { Hsla::transparent_black() })
                    .when(ring, |row| row.child(mark(format!("checkout-{id}-ring"))))
                    // The row a carried pane would move to, for a test to read.
                    .when(lit, |row| row.bg(tokens.bg_selected).border_color(tokens.accent).child(mark(format!("checkout-{id}-target"))))
                    // Where the carried pane is let go, for the drag to find it.
                    .child(publish(Spot::Checkout(id)))
                    .aria_selected(picked)
                    .aria_label(tooltip.clone())
                    .tooltip(tip(tooltip))
                    .hover(move |style| style.bg(fill).text_color(tokens.fg_muted))
                    .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::RailPick { checkout: id, pane: None })))
                    // The fold's box is 24 wide and sits in the row's padding, so a
                    // checkout's branch does not shift when it folds.
                    .child(if checkout.count > 0 {
                        let words = if checkout.open { "Fold its panes away" } else { "Show its panes" };
                        let label = format!("{} {repository} {}", if checkout.open { "Fold" } else { "Unfold" }, checkout.name);
                        chevron(format!("checkout-{id}-fold"), checkout.open, tokens.sp_24, tokens.fg_faint, tokens.icon_xs)
                            .role(Role::Button)
                            .rounded(px(tokens.r_sm))
                            .aria_label(label)
                            .tooltip(tip(words))
                            .hover(move |style| style.bg(fill).text_color(tokens.fg))
                            // The session moves the pick off a pane about to be folded away.
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.ask(held, Call::ToggleCheckout { checkout: id });
                            }))
                            .into_any_element()
                    } else {
                        div().w(px(tokens.sp_24)).h_full().flex_shrink_0().into_any_element()
                    })
                    .child(div().ml(px(-tokens.sp_6)).child(Glyph::Branch.at(tokens.icon_xs)))
                    .child(div().flex_1().min_w_0().truncate().font_family(mono.clone()).child(checkout.name.clone()))
                    // `.shared`: several agents in one checkout, never found out later.
                    .when_some(checkout.shared, |row, n| {
                        row.child(
                            h_flex()
                                .id(format!("checkout-{id}-shared"))
                                .test_support()
                                .aria_label(format!("shared by {n} agents"))
                                .tooltip(tip(format!("shared by {n} agents")))
                                .gap(px(tokens.sp_2))
                                .items_center()
                                .font_family(mono.clone())
                                .text_color(tokens.attention)
                                .child(Glyph::People.at(tokens.icon_xs))
                                .child(n.to_string()),
                        )
                    })
                    .when_some(git, |row, git| row.child(div().font_family(mono.clone()).text_color(tokens.changed).child(git)))
                    .when(!checkout.open, |row| {
                        let words = format!("{} {} folded away", checkout.count, if checkout.count == 1 { "pane" } else { "panes" });
                        row.child(
                            h_flex()
                                .id(format!("checkout-{id}-folded"))
                                .test_support()
                                .aria_label(words.clone())
                                .tooltip(tip(words))
                                .gap(px(tokens.sp_4))
                                .items_center()
                                .font_family(mono)
                                .children(hidden_dot.map(|colour| dot_of(colour, &tokens)))
                                .child(checkout.count.to_string()),
                        )
                    })
                    // `.co .icon`: the + that starts a shell or an agent here, drawn while the row is
                    // pointed at, picked or its menu is open.
                    .child(
                        quiet_button(format!("checkout-{id}-start"), format!("Start a shell or an agent on {repository} {}", checkout.name), "Start a shell or an agent", &tokens)
                            .aria_expanded(hot)
                            .ml_auto()
                            .when(hot, |button| button.bg(tokens.fill).text_color(tokens.accent))
                            .when(!(hot || picked), |button| button.invisible().group_hover(group, |style| style.visible()))
                            .on_hover(move |hovered, _, _| plus_hover.set(*hovered))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.ask(held, Call::ToggleStart { checkout: id, from: StartFrom::Rail });
                            }))
                            .child(Glyph::Plus.at(tokens.icon_sm)),
                    )
                    .context_menu(move |menu, _, cx| popup::fill(menu, &items, &weak, &heard, tokens, cx)),
            )
            .children(menu)
    }
}
