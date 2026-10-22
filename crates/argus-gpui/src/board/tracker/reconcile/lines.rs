//! The proposed lines of the reconcile dialog (BOARD-RECONCILE-6, -7, -8, -20): grouped under headings
//! in the order the session lists them, each a tick box, the key, what it does, why and a `failed` pill,
//! and the things the agent noticed and did not propose.
//!
//! The list is one scroll, whose children are the headings and the rows flat (the noticed block last), so
//! the line under the cursor has a child index to be brought into view by (`child_of`). Row order is
//! Rust's; nothing here sorts.

use argus_desktop_core::board::tracker::reconcile_dialog::{ProposedRow, ReconcileGesture, ReconcilePage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, relative, AnyElement, Context, MouseButton, Role, SharedString, TestSupportExt as _};

use super::ReconcileView;
use crate::kit::mixed::{mixed_box, Mark};
use crate::kit::selected::selected;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::{rem, Type};
use crate::theme::Tokens;

/// The child of the lines list the line `line` is: a heading is a child, a row is a child, in order.
pub fn child_of(page: &ReconcilePage, line: Option<usize>) -> Option<usize> {
    let line = line?;
    let mut at = 0;
    for group in &page.groups {
        at += 1;
        for row in &group.lines {
            if row.line == line {
                return Some(at);
            }
            at += 1;
        }
    }
    None
}

impl ReconcileView {
    /// `.label` in a dialog: sentence case, medium, with the count (or an aside) at the right.
    fn heading(&self, at: usize, words: &str, tone: gpui_kit::Hsla, aside: String, tokens: &Tokens) -> impl IntoElement {
        let label = Type::new(tokens.weight_medium, tokens.fs_md, 1.4);
        let words = SharedString::from(words.to_owned());
        h_flex()
            .id(("reconcile-heading", at))
            .test_support()
            .aria_label(words.clone())
            .gap(rem(tokens.sp_8))
            .items_center()
            .typed(label)
            .text_color(tokens.fg)
            .child(div().text_color(tone).child(words))
            .child(div().flex_1())
            .child(SharedString::from(aside))
    }

    fn line(&self, row: &ProposedRow, proposing: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let line = row.line;
        let words = SharedString::from(format!("Tick {} {}", row.key, row.does));
        let mark = if row.ticked { Mark::Checked } else { Mark::Clear };
        // The input is itself 24px (its box is drawn smaller), so the tick is easy to hit.
        let tick = div().flex_shrink_0().size(rem(tokens.sp_24)).flex().items_center().justify_center().child(
            mixed_box(("reconcile-tick", line), words, mark, tokens)
                .when(!proposing, |tick| tick.opacity(tokens.opacity_dim).cursor_default())
                .when(proposing, |tick| {
                    tick.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).on_click(cx.listener(move |this, _, _, cx| {
                        // A press on the box is the box's only: it does not also select the row.
                        cx.stop_propagation();
                        this.send(ReconcileGesture::Tick { line });
                    }))
                }),
        );
        selected(
            h_flex()
                .id(("reconcile-line", line))
                .role(Role::ListItem)
                .test_support()
                .aria_label(SharedString::from(format!("{} {}", row.key, row.does)))
                .aria_selected(row.on)
                .min_w_0()
                .gap(rem(tokens.sp_8))
                .items_center()
                .px(rem(tokens.sp_8))
                .py(rem(tokens.sp_6))
                .rounded(rem(tokens.r_sm))
                .text_size(rem(tokens.fs_sm))
                .text_color(tokens.fg)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, _| this.send(ReconcileGesture::Select { line })))
                .child(tick)
                .child(div().w(rem(tokens.sp_32 * 2.5)).flex_shrink_0().font_family(SharedString::from(tokens.font_mono)).text_color(tokens.ref_).child(SharedString::from(row.key.clone())))
                .child(div().flex_shrink_0().when(row.on, |does| does.weighted(tokens.weight_semibold)).child(SharedString::from(row.does.clone())))
                .child(div().flex_1().min_w_0().truncate().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(row.why.clone())))
                .children(row.failed.clone().map(|why| {
                    div()
                        .id(("reconcile-failed", line))
                        .test_support()
                        .aria_label("failed")
                        .flex_shrink_0()
                        .px(rem(tokens.sp_8))
                        .py(rem(tokens.hairline))
                        .rounded(rem(tokens.r_pill))
                        .bg(tokens.fill)
                        .text_size(rem(tokens.fs_xs))
                        .weighted(tokens.weight_semibold)
                        .text_color(tokens.danger)
                        .tooltip(tip(why))
                        .child("failed")
                })),
            row.on,
            tokens,
        )
    }

    fn noticed(&self, page: &ReconcilePage, tokens: &Tokens) -> impl IntoElement {
        v_flex()
            .id("reconcile-noticed")
            .gap(rem(tokens.sp_6))
            .pt(rem(tokens.sp_12))
            .border_t_1()
            .border_color(tokens.line)
            .child(self.heading(usize::MAX, "Noticed, not proposed", tokens.fg, "outside what you asked".to_owned(), tokens))
            .children(page.noticed.iter().map(|noticed| {
                h_flex()
                    .gap(rem(tokens.sp_8))
                    .px(rem(tokens.sp_8))
                    .py(rem(tokens.sp_6))
                    .text_size(rem(tokens.fs_sm))
                    .items_center()
                    .child(div().text_color(tokens.fg_faint).child("\u{b7}"))
                    .child(div().w(rem(tokens.sp_32 * 2.5)).flex_shrink_0().font_family(SharedString::from(tokens.font_mono)).text_color(tokens.ref_).child(SharedString::from(noticed.label.clone())))
                    .child(div().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(noticed.text.clone())))
            }))
            .child(div().typed(tokens.type_small).text_color(tokens.fg_muted).child("Tell the agent to include any of these and it adds a line. It never acts on them by itself."))
    }

    /// The lines list: the left column of a proposal.
    pub(super) fn list(&self, page: &ReconcilePage, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let proposing = page.stage == "proposed";
        let mut children: Vec<AnyElement> = Vec::new();
        for (at, group) in page.groups.iter().enumerate() {
            let tone = if group.into_argus { tokens.ref_ } else { tokens.attention };
            let heading = self.heading(at, &group.heading, tone, group.lines.len().to_string(), tokens);
            children.push(div().when(at > 0, |top| top.mt(rem(tokens.sp_8 + tokens.sp_2))).child(heading).into_any_element());
            children.extend(group.lines.iter().map(|row| self.line(row, proposing, tokens, cx).into_any_element()));
        }
        if !page.noticed.is_empty() {
            children.push(div().mt(rem(tokens.sp_8 + tokens.sp_2)).child(self.noticed(page, tokens)).into_any_element());
        }
        v_flex()
            .id("reconcile-lines")
            .role(Role::List)
            .test_support()
            .aria_label("Proposed lines")
            .w(relative(0.565))
            .flex_shrink_0()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.lines)
            .gap(rem(tokens.sp_6))
            .children(children)
            .into_any_element()
    }
}
