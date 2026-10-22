//! A pane's row: its status as a dot or a word, its title, and the close that appears under the
//! pointer.

use argus_desktop_core::connection::page::{Call, Pane};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Hsla, MouseButton, MouseDownEvent, MouseUpEvent, Role, SharedString};

use super::picked::Picked;
use super::RailView;
use super::row::line;
use super::status::pane_word;
use crate::kit::icons::Glyph;
use crate::layout::carry::{Carried, Carrier};
use crate::kit::pill::status_pill;
use crate::kit::status_dot::status_dot;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn pane(
        &self,
        checkout: u64,
        pane: &Pane,
        offstage: bool,
        held: bool,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = pane.id;
        let picked = self.picked == Some(Picked { checkout, pane: Some(id) });
        let word = pane_word(pane.status);
        let tooltip = format!(
            "{} · {}",
            pane.title,
            pane.note.clone().unwrap_or_else(|| format!("{}, {}", if pane.agent { "agent" } else { "shell" }, pane.status))
        );
        let fill = tokens.fill;
        // Off the Workspace the pane on screen is not the selection, the stage's own
        // nav item is: it keeps an edge and its text, not the box (`.offstage`).
        let boxed = pane.on && !offstage;
        let group = SharedString::from(format!("pane-row-{id}"));
        let ring = if picked {
            tokens.accent
        } else if boxed {
            // `--mark-sel` under Ledger: a hairline on the selection fill.
            tokens.line_strong
        } else {
            Hsla::transparent_black()
        };
        line(format!("pane-{id}"), 32.0, tokens)
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
            .aria_label(tooltip)
            .when(boxed, |row| row.bg(tokens.bg_selected))
            .when(!boxed, |row| row.hover(move |style| style.bg(fill)))
            .when(pane.on && offstage, |row| {
                row.child(div().absolute().left_0().top_0().bottom_0().w(px(tokens.pane_mark_w)).bg(tokens.line_strong))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                // The pane comes on screen and the keys stay on the rail until
                // Enter or a click on the stage (SHELL-RAIL-13). The rail has
                // them because its root tracks focus and GPUI focuses that
                // root on a press inside it, held or not: a held rail picks
                // nothing, so `press` answers no key.
                this.pick(held, Picked { checkout, pane: Some(id) });
                // A pane comes on screen where Git was, so Git goes first.
                if !held {
                    for call in this.leave_git(cx) {
                        this.caller.call(call);
                    }
                }
                this.ask(held, Call::Select { pane: id });
                cx.notify();
            }))
            // Carried onto the stage it opens there, on another checkout's row it moves there
            // (SHELL-RAIL-17). A press that never travels is the click above.
            .on_mouse_down(MouseButton::Left, {
                let (carrier, title) = (Carrier::of(cx), pane.title.clone());
                cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                    if !held {
                        carrier.press(event.position, Carried::Rail { pane: id, title: title.clone(), checkout: Some(checkout) }, cx);
                    }
                })
            })
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _: &MouseUpEvent, _, _| this.ask(held, Call::Close { pane: id })),
            )
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
                    .top(px((32.0 - tokens.icon_box) / 2.0 - 1.0))
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
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.ask(held, Call::Close { pane: id });
                    }))
                    .child(Glyph::Close.at(12.0)),
            )
    }
}
