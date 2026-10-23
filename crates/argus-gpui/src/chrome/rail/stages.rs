//! The nav rows: which stage each lights, and the press that goes there.
//!
//! A press of a nav item: Git asks for the review of the checkout the window is on; every item that
//! goes to another stage first closes what the session has open of Git, in the same breath
//! (`review::entry`), so the stage is the session's to say and is never left standing under
//! another.
//!
//! The built-in items are a fixed list (`Nav::ALL`), the Browsers item last of them; the
//! Extensions and Views folds that follow are `tail`'s. The Browsers item's count, fold and sub-rows
//! (SHELL-BROWSER-14) are `browsers`'. A narrow rail draws each item as a tile of its icon, and the
//! folds not at all.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, MouseButton, MouseDownEvent, SharedString};

use super::row::{line, Ctx};
use super::slice::Slice;
use super::RailView;
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{DockGesture, Panel};
use crate::chrome::nav::Nav;
use crate::keys::over::run_over;
use crate::layout::carry::Carrier;
use crate::layout::work::Absent;
use crate::kit::keycap::keycap;
use crate::kit::tip::tip;
use crate::review::entry::checkout_here;
use crate::theme::scale::rem;
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
        let picked = self.slice.rail.cursor.pick.map(|pick| pick.checkout);
        let calls = match nav {
            // Git beside the work, open, comes in sight where it sits: its dock opens on it.
            Nav::Git if self.heard.read(cx).changes_beside() => vec![Call::Docks { gesture: DockGesture::Show { panel: Panel::Changes } }],
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

    /// A press of Git that may travel: the changes' panel as it is while a review is open, or one
    /// its drop opens. History open, or no checkout to review, it is Git's click only.
    fn carry_git(&self, event: &MouseDownEvent, held: bool, cx: &mut Context<Self>) {
        let heard = self.heard.read(cx);
        let open = heard.open();
        if open.review {
            self.carry_panel(event, &Panel::Changes, "Changes", held, cx);
        } else if let (false, false, Some(checkout)) = (held, open.history, checkout_here(&heard.rail, self.slice.rail.cursor.pick.map(|pick| pick.checkout))) {
            Carrier::of(cx).press_absent(event.position, Panel::Changes, "Changes".into(), true, Some(Absent::Review { checkout }), cx);
        }
    }

    pub(super) fn nav(&self, slice: &Slice, ctx: Ctx, mono: &SharedString, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (fill, held) = (tokens.fill, ctx.held);
        let rail = &slice.rail;
        // `.navs`: the keycaps show while the pointer is anywhere over the group.
        v_flex()
            .group("navs")
            .flex_shrink_0()
            .px(rem(tokens.sp_8))
            .pb(rem(tokens.sp_8))
            .gap(rem(tokens.sp_2))
            .when(ctx.icons, |navs| navs.items_center())
            .border_b_1()
            .border_color(tokens.line)
            .children(Nav::ALL.into_iter().enumerate().map(|(at, nav)| {
                let lit = nav.lit(rail.stage, slice.git_up);
                let search = nav == Nav::Search;
                let caps = h_flex()
                    .gap(rem(tokens.sp_4))
                    .children(slice.keys.get(at).cloned().unwrap_or_default().into_iter().map(|key| keycap(key, mono.clone(), tokens)));
                let caps = if search { caps } else { caps.opacity(0.0).group_hover("navs", |style| style.opacity(1.0)) };
                let base = line(nav.id(), if search { tokens.search_h } else { tokens.nav_h }, tokens).text_size(rem(tokens.fs_md));
                // A narrow rail: a tile of the icon, the words in the tooltip.
                let base = if ctx.icons {
                    base.w(rem(tokens.rail_tile)).h(rem(tokens.rail_tile)).justify_center().tooltip(tip(nav.label()))
                } else {
                    base.px(rem(tokens.sp_8))
                };
                let base = if search {
                    // `.search`: a field's outline, not a row's fill.
                    let base = base.text_color(tokens.fg_faint);
                    if ctx.icons {
                        base.hover(move |style| style.bg(fill).text_color(tokens.fg))
                    } else {
                        base.mb(rem(tokens.sp_6))
                            .pl(rem(tokens.sp_12))
                            .gap(rem(tokens.sp_8))
                            .border_1()
                            .border_color(tokens.line_strong)
                            .rounded(rem(tokens.r_md))
                            .hover(|style| style.text_color(tokens.fg_muted))
                    }
                } else {
                    base.when(!ctx.icons, |row| row.gap(rem(tokens.sp_12)))
                        .text_color(tokens.fg)
                        .when(lit, |row| row.bg(tokens.bg_selected))
                        .when(!lit, |row| row.hover(move |style| style.bg(fill)))
                };
                let glyph = nav.glyph().at(tokens.icon_glyph);
                let icon = if search {
                    glyph.into_any_element()
                } else {
                    let mark = div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(rem(tokens.nav_icon_cell))
                        .flex_shrink_0()
                        .text_color(if lit { tokens.accent } else { tokens.fg_muted });
                    // `.nav .ic.new`: the one item whose mark is a filled disc.
                    let mark = if nav == Nav::New { mark.bg(fill).text_color(tokens.fg).rounded_full() } else { mark };
                    mark.child(glyph).into_any_element()
                };
                let browsers = nav == Nav::Browsers && !ctx.icons;
                let row = base
                    .aria_selected(lit)
                    .aria_label(nav.label())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        // Search is the palette, the window's own as Ctrl K is: it opens it and the palette tells the
                        // session, which never opens it on the window's behalf.
                        if nav == Nav::Search {
                            if !held {
                                run_over(Action::Palette, window, cx);
                            }
                            return;
                        }
                        this.go(nav, held, cx)
                    }))
                    // A press that travels carries the changes beside an agent, opened for the
                    // checkout here when no review is.
                    .when(nav == Nav::Git, |row| {
                        row.on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| this.carry_git(event, held, cx)))
                    })
                    // A press that travels carries every browser, as their panel, beside an agent; with
                    // none open, a cell for the next.
                    .when(nav == Nav::Browsers, |row| {
                        let none = slice.subs.is_empty();
                        row.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                if !none {
                                    this.carry_panel(event, &Panel::Browsers, nav.label(), held, cx);
                                } else if !held {
                                    Carrier::of(cx).press_absent(event.position, Panel::Browsers, nav.label().into(), false, Some(Absent::Empty), cx);
                                }
                            }),
                        )
                    })
                    .child(icon)
                    .when(!ctx.icons, |row| row.child(div().flex_1().min_w_0().truncate().child(nav.label())))
                    .when(browsers, |row| row.child(self.browsers_tail(slice, tokens, cx)))
                    .when(!ctx.icons, |row| row.child(caps));
                // `.nav-nest`: the open browsers, under the item while it is unfolded.
                if browsers && self.browsers_listed && !slice.subs.is_empty() {
                    v_flex().gap(rem(tokens.sp_2)).child(row).child(self.browsers_list(slice, ctx, tokens, cx)).into_any_element()
                } else {
                    row.into_any_element()
                }
            }))
            // The Extensions and Views folds, where they have anything to list.
            .children((!ctx.icons).then(|| self.tail(slice, ctx, tokens, cx)))
    }
}
