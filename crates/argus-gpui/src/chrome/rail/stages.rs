//! The nav rows: which stage each lights, and the press that goes there.
//!
//! A press of a nav item: Git asks for the review of the checkout the window is on; every item that
//! goes to another stage first closes what the session has open of Git, in the same breath
//! (`review::entry`), so the stage is the session's to say and is never left standing under
//! another.

use argus_desktop_core::connection::page::Rail;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, px, Context, SharedString};

use super::RailView;
use super::row::line;
use crate::chrome::nav::Nav;
use crate::kit::keycap::keycap;
use crate::review::entry::checkout_here;
use crate::theme::Tokens;

impl RailView {
    /// A press of a nav item. Git asks for the review of the checkout the
    /// window is on; every item that goes to another stage first closes what
    /// the session has open of Git, in the same breath (`review::entry`), so
    /// the stage is the session's to say and is never left standing under
    /// another.
    pub(super) fn go(&mut self, nav: Nav, held: bool, cx: &mut Context<Self>) {
        if held {
            return;
        }
        let picked = self.picked.map(|picked| picked.checkout);
        let calls = match nav {
            Nav::Git => self.heard.update(cx, |heard, cx| {
                let (open, here) = (heard.open(), checkout_here(&heard.rail, picked));
                let calls = heard.git.enter(open, here);
                // A Git pressed with nothing to open puts the stage up on the window's word alone.
                cx.notify();
                calls
            }),
            _ if nav.leaves_git() => self.leave_git(cx),
            _ => Vec::new(),
        };
        for call in calls.into_iter().chain(nav.call()) {
            self.caller.call(call);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn nav(
        &self,
        rail: &Rail,
        git_up: bool,
        held: bool,
        keys: &[Vec<String>],
        mono: &SharedString,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fill = tokens.fill;
        // `.navs`: the keycaps show while the pointer is anywhere over the group.
        v_flex()
            .group("navs")
            .flex_shrink_0()
            .px(px(tokens.sp_8))
            .pb(px(tokens.sp_8))
            .gap(px(tokens.sp_2))
            .border_b_1()
            .border_color(tokens.line)
            .children(Nav::ALL.into_iter().enumerate().map(|(at, nav)| {
                let lit = nav.lit(rail.stage, git_up);
                let search = nav == Nav::Search;
                let caps = h_flex()
                    .gap(px(tokens.sp_4))
                    .children(keys.get(at).cloned().unwrap_or_default().into_iter().map(|key| keycap(key, mono.clone(), tokens)));
                let caps = if search { caps } else { caps.opacity(0.0).group_hover("navs", |style| style.opacity(1.0)) };
                let base = line(nav.id(), if search { 34.0 } else { 32.0 }, tokens).text_size(px(tokens.fs_md)).px(px(tokens.sp_8));
                let base = if search {
                    // `.search`: a field's outline, not a row's fill.
                    base.mb(px(tokens.sp_6))
                        .pl(px(tokens.sp_12))
                        .gap(px(tokens.sp_8))
                        .border_1()
                        .border_color(tokens.line_strong)
                        .rounded(px(tokens.r_md))
                        .text_color(tokens.fg_faint)
                        .hover(|style| style.text_color(tokens.fg_muted))
                } else {
                    base.gap(px(tokens.sp_12))
                        .text_color(tokens.fg)
                        .when(lit, |row| row.bg(tokens.bg_selected))
                        .when(!lit, |row| row.hover(move |style| style.bg(fill)))
                };
                let glyph = nav.glyph().at(16.0);
                let icon = if search {
                    glyph.into_any_element()
                } else {
                    let mark = div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(tokens.nav_icon_cell))
                        .flex_shrink_0()
                        .text_color(if lit { tokens.accent } else { tokens.fg_muted });
                    // `.nav .ic.new`: the one item whose mark is a filled disc.
                    let mark = if nav == Nav::New { mark.bg(fill).text_color(tokens.fg).rounded_full() } else { mark };
                    mark.child(glyph).into_any_element()
                };
                base.aria_selected(lit)
                    .aria_label(nav.label())
                    .on_click(cx.listener(move |this, _, _, cx| this.go(nav, held, cx)))
                    .child(icon)
                    .child(div().flex_1().min_w_0().truncate().child(nav.label()))
                    .child(caps)
            }))
    }
}
