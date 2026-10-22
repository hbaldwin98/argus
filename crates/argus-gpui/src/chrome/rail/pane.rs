//! A pane's row: its status as a dot or a word, its title, the close that appears under the
//! pointer, its right-click menu, and a middle click closing it.
//!
//! A narrow rail draws a tile instead: the initials of the pane's task with its status as a dot on
//! the corner, the name in the tooltip (`pane_mark`, SHELL-RAIL-16, 33).

use argus_desktop_core::chrome::cursor::Pick;
use argus_desktop_core::chrome::rail::pane_mark;
use argus_desktop_core::connection::page::{Call, Pane, Rail};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Hsla, MouseButton, MouseDownEvent, MouseUpEvent, Role, SharedString};

use super::menus;
use super::popup;
use super::row::{line, Ctx, Line};
use super::status::pane_word;
use super::RailView;
use crate::kit::area;
use crate::kit::icons::Glyph;
use crate::kit::pill::status_pill;
use crate::kit::status_dot::status_dot;
use crate::kit::tip::tip;
use crate::layout::carry::{Carried, Carrier};
use crate::theme::scale::Mark;
use crate::theme::Tokens;

/// The tooltip a pane's row says: its title, then its note or what it is.
fn tooltip(pane: &Pane) -> String {
    format!("{} \u{b7} {}", pane.title, pane.note.clone().unwrap_or_else(|| format!("{}, {}", if pane.agent { "agent" } else { "shell" }, pane.status)))
}

impl RailView {
    pub(super) fn pane(&self, rail: &Rail, checkout: u64, pane: &Pane, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let picked = rail.cursor.pick == Some(Pick { checkout, pane: Some(pane.id) });
        let items = menus::pane(checkout, pane);
        let (weak, heard) = (cx.weak_entity(), self.heard.clone());
        let tokens = *tokens;
        let row = if ctx.icons { self.tile(checkout, pane, picked, ctx, &tokens, cx).into_any_element() } else { self.row(checkout, pane, picked, ctx, &tokens, cx).into_any_element() };
        div().id(format!("pane-row-{}", pane.id)).test_support().child(row).context_menu(move |menu, _, cx| popup::fill(menu, &items, &weak, &heard, tokens, cx))
    }

    /// What every pane row does with a press: it is carried onto the stage or another checkout's row
    /// once it travels (SHELL-RAIL-17), shown when it does not, and closed by a middle click.
    fn presses(&self, row: Line, checkout: u64, pane: &Pane, held: bool, cx: &mut Context<Self>) -> Line {
        let (id, title) = (pane.id, pane.title.clone());
        let carrier = Carrier::of(cx);
        row.on_click(cx.listener(move |this, _, _, cx| this.show(checkout, id, held, cx)))
            .on_mouse_down(MouseButton::Left, {
                cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                    if !held {
                        carrier.press(event.position, Carried::Rail { pane: id, title: title.clone(), checkout: Some(checkout) }, cx);
                    }
                })
            })
            .on_mouse_up(MouseButton::Middle, cx.listener(move |this, _: &MouseUpEvent, _, _| this.ask(held, Call::Close { pane: id })))
    }

    fn row(&self, checkout: u64, pane: &Pane, picked: bool, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, held) = (pane.id, ctx.held);
        let word = pane_word(pane.status);
        let fill = tokens.fill;
        // Off the Workspace the pane on screen is not the selection, the stage's own
        // nav item is: it keeps an edge and its text, not the box (`.offstage`).
        let boxed = pane.on && !ctx.offstage;
        let group = SharedString::from(format!("pane-row-{id}"));
        // `--mark-sel`: a bar down the leading edge, or a hairline all round, or the fill alone.
        let hairline = boxed && tokens.mark_sel == Mark::Hairline;
        let ring = if picked {
            tokens.ring
        } else if hairline {
            tokens.mark_sel_edge
        } else {
            Hsla::transparent_black()
        };
        let edge = if pane.on && ctx.offstage {
            Some(tokens.pane_mark_w)
        } else if boxed && tokens.mark_sel == Mark::Bar {
            Some(tokens.mark_bar_w)
        } else {
            None
        };
        let shown = pane.on.then(|| area::measure(self.shown_row.clone()));
        let row = line(format!("pane-{id}"), tokens.pane_row_h, tokens)
            .group(group.clone())
            .relative()
            .w_full()
            // One pixel less each side: the ring is a border here, an outline in the page.
            .pl(px(tokens.sp_12 - 1.0))
            .pr(px(tokens.sp_8 - 1.0))
            .gap(px(tokens.sp_12))
            .text_size(px(tokens.fs_md))
            .text_color(if pane.on || pane.agent { tokens.fg } else { tokens.fg_muted })
            .border_1()
            .border_color(ring)
            .aria_selected(pane.on)
            .aria_label(tooltip(pane))
            .tooltip(tip(tooltip(pane)))
            .when(boxed, |row| row.bg(tokens.bg_selected))
            .when(!boxed, |row| row.hover(move |style| style.bg(fill)))
            .when_some(edge, |row, width| row.child(div().absolute().left_0().top_0().bottom_0().w(px(width)).bg(tokens.mark_sel_edge)))
            .children(shown);
        self.presses(row, checkout, pane, held, cx)
            // One status encoding per row: the pill when it has a word, else the dot.
            .when(word.is_none(), |row| row.child(status_dot(format!("pane-{id}"), pane.status, tokens)))
            .child(div().flex_1().min_w_0().truncate().child(pane.title.clone()))
            .when_some(word, |row, word| {
                // `.pane-end`: gives way to the close button while the row is pointed at.
                row.child(div().flex_shrink_0().group_hover(group.clone(), |style| style.invisible()).child(status_pill(format!("pane-{id}-word"), word, pane.status, tokens)))
            })
            // `.pane-row .close`: over the status word, only under the pointer.
            .child(
                div()
                    .id(format!("pane-{id}-close"))
                    .test_support()
                    .role(Role::Button)
                    .aria_label(format!("Close {}", pane.title))
                    .absolute()
                    .right(px(tokens.sp_8))
                    .top(px((tokens.pane_row_h - tokens.icon_box) / 2.0 - 1.0))
                    .size(px(tokens.icon_box))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(tokens.r_sm))
                    .text_color(tokens.fg_muted)
                    .cursor_pointer()
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .hover(move |style| style.bg(fill).text_color(tokens.fg))
                    .tooltip(tip("Close"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.ask(held, Call::Close { pane: id });
                    }))
                    .child(Glyph::Close.at(tokens.icon_xs)),
            )
    }

    /// `.pane` in a folded rail: a tile of the task's initials, its status a dot on the corner.
    fn tile(&self, checkout: u64, pane: &Pane, picked: bool, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, held) = (pane.id, ctx.held);
        let fill = tokens.fill;
        let boxed = pane.on && !ctx.offstage;
        let shown = pane.on.then(|| area::measure(self.shown_row.clone()));
        // The corner dot: filled by how loud the status is; an idle or ended pane is a ring.
        let (dot, ring) = match pane.status {
            "working" => (tokens.accent, tokens.bg_chrome),
            "waiting" | "needs-review" => (tokens.attention, tokens.bg_chrome),
            "failed" => (tokens.danger, tokens.bg_chrome),
            "done" => (tokens.ok, tokens.bg_chrome),
            "idle" | "exited" => (Hsla::transparent_black(), tokens.fg_faint),
            _ => (tokens.fg_faint, tokens.bg_chrome),
        };
        let tile = line(format!("pane-{id}"), tokens.rail_tile, tokens)
            .group(SharedString::from(format!("pane-row-{id}")))
            .relative()
            .w(px(tokens.rail_tile))
            .justify_center()
            .rounded(px(tokens.r_md))
            .border_1()
            .border_color(if picked { tokens.ring } else { tokens.line })
            .font_family(SharedString::from(tokens.font_mono))
            .text_size(px(tokens.fs_sm))
            .text_color(if pane.on || pane.agent { tokens.fg } else { tokens.fg_muted })
            .aria_selected(pane.on)
            .aria_label(tooltip(pane))
            .tooltip(tip(tooltip(pane)))
            .when(boxed, |tile| tile.bg(tokens.bg_selected))
            .when(!boxed, |tile| tile.hover(move |style| style.bg(fill)))
            .children(shown);
        let dot = div()
            .id(format!("pane-{id}-dot"))
            .test_support()
            .absolute()
            .top(px(-tokens.sp_2))
            .right(px(-tokens.sp_2))
            .size(px(tokens.sp_12))
            .rounded_full()
            .border_2()
            .border_color(ring)
            .bg(dot);
        v_flex().items_center().mb(px(tokens.sp_4)).child(self.presses(tile, checkout, pane, held, cx).child(pane_mark(&pane.title)).child(dot))
    }
}

