//! One row of the list (`.irow`): its number, title, tasks, questions and branch, and under them
//! the note that says why it stands where it does.
//!
//! A click peeks the feature, a double click opens it, and a right click peeks it and opens the
//! Move menu where the pointer is. The peeked row is the one Rust says is, marked with a ring; the
//! view keeps no cursor of its own.

use argus_desktop_core::board::index_words::{self, Bit};
use argus_desktop_core::connection::page::{Call, FeatureRow};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Entity, MouseButton, SharedString, TestSupportExt as _};

use crate::board::cells::{agent_pill, questions_cell, tasks_cell};
use crate::board::ctx::Ctx;
use crate::board::menus::Menu;
use crate::kit::area;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a row is drawn with besides itself.
pub struct Setting {
    /// Whether the list is wide enough for the Checkout column.
    pub checkout: bool,
    /// Where the row under the cursor is laid out, for the Move menu that opens beside it.
    pub anchor: std::rc::Rc<std::cell::Cell<gpui_kit::Bounds<gpui_kit::Pixels>>>,
}

/// The line under a row's title, built from `index_words::note`'s bits.
fn note(row: &FeatureRow, tokens: &Tokens) -> Option<impl IntoElement> {
    let bits = index_words::note(row);
    if bits.is_empty() {
        return None;
    }
    let mono = SharedString::from(tokens.font_mono);
    Some(h_flex().w_full().items_center().gap(rem(tokens.sp_4)).min_w_0().overflow_hidden().whitespace_nowrap().children(bits.into_iter().map(|bit| match bit {
        Bit::Text(text) => div().child(SharedString::from(text)).into_any_element(),
        Bit::Faint(text) => div().text_color(tokens.fg_faint).child(SharedString::from(text)).into_any_element(),
        Bit::Mono(text) => div().font_family(mono.clone()).child(SharedString::from(text)).into_any_element(),
        Bit::MonoFaint(text) => div().font_family(mono.clone()).text_color(tokens.fg_faint).child(SharedString::from(text)).into_any_element(),
        Bit::Agent(agent) => h_flex()
            .flex_1()
            .items_center()
            .gap(rem(tokens.sp_8))
            .min_w_0()
            .child(div().flex_1().min_w_0().truncate().child(SharedString::from(agent.title.clone())))
            .child(agent_pill(&row.slug, &agent, tokens))
            .into_any_element(),
    })))
}

pub fn row(row: &FeatureRow, setting: &Setting, ctx: &Ctx, menu: &Entity<Menu>, tokens: &Tokens) -> impl IntoElement {
    let quiet = index_words::quiet(row);
    let peeked = row.peeked;
    let gap = rem(tokens.sp_8);
    let mono = SharedString::from(tokens.font_mono);
    let (click, open, ring, fill) = (ctx.clone(), ctx.clone(), tokens.ring, tokens.fill);
    let (slug, opened, pressed, menu) = (row.slug.clone(), row.slug.clone(), row.slug.clone(), menu.clone());
    let reach = ctx.clone();
    v_flex()
        .id(SharedString::from(format!("feature-row-{}", row.slug)))
        .test_support()
        .aria_selected(peeked)
        .aria_label(SharedString::from(format!("#{} {}", row.number, row.title)))
        .relative()
        .w_full()
        .flex_shrink_0()
        .gap(rem(tokens.sp_8))
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_6))
        .border_1()
        .border_color(gpui_kit::transparent_black())
        .rounded(rem(tokens.r_md))
        .cursor_pointer()
        .when(peeked, |element| element.bg(tokens.bg_selected).border_color(ring))
        .when(!peeked, |element| element.hover(move |style| style.bg(fill)))
        .on_click(move |event, _, cx| {
            // A double click is a click first, as the page's is.
            if event.click_count() >= 2 {
                open.send(Call::OpenFeature { slug: Some(opened.clone()) }, cx);
            } else {
                click.send(Call::Peek { slug: slug.clone() }, cx);
            }
        })
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            reach.send(Call::Peek { slug: pressed.clone() }, cx);
            menu.update(cx, |menu, cx| menu.open_move(&pressed, event.position, false, window, cx));
        })
        .when(peeked, |element| element.child(area::measure(setting.anchor.clone())))
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap(gap)
                .child(
                    div()
                        .w(rem(tokens.fx_num_w))
                        .flex_shrink_0()
                        .font_family(mono.clone())
                        .text_size(rem(tokens.fs_sm))
                        .text_color(if peeked { tokens.accent } else { tokens.fg_faint })
                        .child(SharedString::from(format!("#{}", row.number))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(rem(tokens.fx_title_min))
                        .truncate()
                        .when(peeked, |title| title.weighted(tokens.weight_semibold))
                        .text_color(if quiet { tokens.fg_muted } else { tokens.fg })
                        .child(SharedString::from(row.title.clone())),
                )
                .child(div().w(rem(tokens.fx_tasks_w)).flex_shrink_0().child(tasks_cell(row, tokens)))
                .child(div().w(rem(tokens.fx_questions_w)).flex_shrink_0().children(questions_cell(row, tokens)))
                .when(setting.checkout, |line| {
                    line.child(
                        div()
                            .flex_shrink_0()
                            .w(rem(tokens.fx_checkout_max))
                            .truncate()
                            .font_family(mono)
                            .text_size(rem(tokens.fs_sm))
                            .text_color(if quiet { tokens.fg_faint } else { tokens.ref_ })
                            .children(row.branch.clone().map(SharedString::from)),
                    )
                }),
        )
        .children(note(row, tokens).map(|line| div().w_full().pl(rem(tokens.fx_num_w + tokens.sp_8)).typed(tokens.type_small).text_color(tokens.fg_muted).child(line)))
}
