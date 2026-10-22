//! A checkout's row: its fold, branch, shared and git marks, the pick's ring, and the panes
//! hanging from it while it is open.

use argus_desktop_core::connection::page::{Call, Checkout};
use gpui_kit::component::{v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Hsla, Role};

use super::picked::Picked;
use super::RailView;
use super::row::{chevron, line};
use super::status::{dot_of, status_colour};
use crate::kit::icons::Glyph;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn checkout(
        &self,
        checkout: &Checkout,
        offstage: bool,
        held: bool,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = checkout.id;
        let picked = self.picked == Some(Picked { checkout: id, pane: None });
        let where_ = if checkout.kind == "main" { "main checkout" } else { "worktree" };
        let tooltip = if checkout.branch.is_empty() {
            where_.to_owned()
        } else {
            format!("{where_} on {}", checkout.branch)
        };
        let hidden_dot = if checkout.open { None } else { checkout.status.and_then(|word| status_colour(word, tokens)) };
        let git = checkout.git.clone().filter(|_| !checkout.clean);
        let fill = tokens.fill;
        let mono = cx.theme().mono_font_family.clone();
        v_flex()
            .child(
                // `.co`: a margin of 8 each side, the fold chevron in its 12px of left padding.
                div().mx(px(tokens.sp_8)).child(
                    line(format!("checkout-{id}"), tokens.control_h_sm, tokens)
                        .w_full()
                        .pl(px(tokens.sp_12 - 1.0))
                        .pr(px(tokens.sp_4 - 1.0))
                        .gap(px(tokens.sp_6))
                        .text_size(px(tokens.fs_xs))
                        .text_color(tokens.fg_faint)
                        .border_1()
                        // `.co.picked`: the ring that says which row holds the keys.
                        .border_color(if picked { tokens.accent } else { Hsla::transparent_black() })
                        .aria_selected(picked)
                        .aria_label(tooltip)
                        .hover(move |style| style.bg(fill).text_color(tokens.fg_muted))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pick(held, Picked { checkout: id, pane: None });
                            cx.notify();
                        }))
                        // The fold's box is 24 wide and sits in the row's padding, so a
                        // checkout's branch does not shift when it folds.
                        .child(if checkout.count > 0 {
                            chevron(format!("checkout-{id}-fold"), checkout.open, tokens.sp_24, tokens.fg_faint)
                                .role(Role::Button)
                                .rounded(px(tokens.r_sm))
                                .aria_label(if checkout.open { "Fold its panes away" } else { "Show its panes" })
                                .hover(move |style| style.bg(fill).text_color(tokens.fg))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    // The pick leaves a pane that is about to be folded away first.
                                    if !held && this.picked.is_some_and(|p| p.checkout == id && p.pane.is_some()) {
                                        this.picked = Some(Picked { checkout: id, pane: None });
                                    }
                                    this.ask(held, Call::ToggleCheckout { checkout: id });
                                    cx.notify();
                                }))
                                .into_any_element()
                        } else {
                            div().w(px(tokens.sp_24)).h_full().flex_shrink_0().into_any_element()
                        })
                        .child(div().ml(px(-tokens.sp_6)).child(Glyph::Branch.at(12.0)))
                        .child(div().flex_1().min_w_0().truncate().font_family(mono.clone()).child(checkout.name.clone()))
                        .when_some(checkout.shared, |row, n| {
                            row.child(
                                div()
                                    .id(format!("checkout-{id}-shared"))
                                    .test_support()
                                    .aria_label(format!("shared by {n} agents"))
                                    .child(format!("×{n}")),
                            )
                        })
                        .when_some(git, |row, git| {
                            row.child(div().font_family(mono.clone()).text_color(tokens.changed).child(git))
                        })
                        .when(!checkout.open, |row| {
                            row.when_some(hidden_dot, |row, colour| row.child(dot_of(colour, tokens))).child(
                                div()
                                    .id(format!("checkout-{id}-folded"))
                                    .test_support()
                                    .aria_label(format!(
                                        "{} {} folded away",
                                        checkout.count,
                                        if checkout.count == 1 { "pane" } else { "panes" }
                                    ))
                                    .font_family(mono)
                                    .child(checkout.count.to_string()),
                            )
                        }),
                ),
            )
            // A folded checkout draws no pane, whatever it was given. `.nest`: its
            // panes hang from a hairline under the checkout's own branch.
            .when(checkout.open, |col| {
                col.child(
                    v_flex()
                        .ml(px(tokens.pane_indent))
                        .mr(px(tokens.sp_8))
                        .mb(px(tokens.sp_4))
                        .pl(px(tokens.sp_4))
                        .border_l_1()
                        .border_color(tokens.line)
                        .children(checkout.panes.iter().map(|pane| self.pane(id, pane, offstage, held, tokens, cx))),
                )
            })
    }
}
