//! The menu as it is drawn (`.menu`, `.mlabel`, `.mi`, `.msep`): a card of labels, rules and
//! choices, the highlighted one lit.
//!
//! It is drawn here rather than as the kit's popup because the board opens it from a key, at a
//! place it measured, and steps through it with keys of the Feature layer (the kit's popup is
//! opened by a press on an element and keeps its highlight to itself).

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use super::items::{Does, Entry, Item, Lead};
use crate::kit::icon_button::Node;
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// One choice: its lead, number and label, and at the right edge why it is off or the key that does it.
fn choice(at: usize, item: &Item, on: bool, tokens: &Tokens) -> Node {
    let mono = SharedString::from(tokens.font_mono);
    let (fill, off) = (tokens.fill, item.disabled);
    let lead = match item.lead {
        Lead::None => None,
        Lead::Here => Some(div().text_color(tokens.ok).child("\u{2713}")),
        Lead::Tick(true) => Some(div().text_color(tokens.ok).child("\u{2713}")),
        Lead::Tick(false) => Some(div().text_color(tokens.fg_faint).child(SharedString::from("\u{a0}"))),
    };
    h_flex()
        .id(("feature-menu-item", at))
        .test_support()
        .aria_label(SharedString::from(item.text.clone()))
        .aria_selected(on)
        .h(rem(tokens.control_h))
        .flex_shrink_0()
        .items_center()
        .gap(rem(tokens.sp_8))
        .px(rem(tokens.sp_8))
        .rounded(rem(tokens.r_sm))
        .whitespace_nowrap()
        .text_color(if off { tokens.fg_faint } else { tokens.fg })
        .when(!off, |row| row.cursor_pointer().hover(move |style| style.bg(fill)))
        .when(on, |row| row.bg(tokens.bg_selected))
        .children(lead)
        .children(item.number.map(|number| div().font_family(mono.clone()).text_color(tokens.fg_faint).child(SharedString::from(format!("#{number}")))))
        .child(div().flex_1().min_w_0().truncate().child(SharedString::from(item.text.clone())))
        .children(item.note.clone().map(|note| div().min_w_0().truncate().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(note))))
        .children(item.note.is_none().then_some(item.key).flatten().map(|key| keycap(key, mono.clone(), tokens)))
}

/// The card: `entries`, the highlighted one at `cursor`; `press` is told the entry pressed.
pub fn card(label: SharedString, entries: &[Entry], cursor: Option<usize>, press: impl Fn(usize, &Does, &mut gpui_kit::Window, &mut gpui_kit::App) + Clone + 'static, tokens: &Tokens) -> Node {
    v_flex()
        .id("feature-menu")
        .test_support()
        .role(gpui_kit::Role::Menu)
        .aria_label(label)
        .w(rem(tokens.fx_menu_w))
        .occlude()
        .p(rem(tokens.sp_4))
        .bg(tokens.bg_surface)
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_md))
        .shadow(tokens.shadow_pop())
        .text_size(rem(tokens.fs_md))
        .text_color(tokens.fg)
        .children(entries.iter().enumerate().map(|(at, entry)| match entry {
            Entry::Label(words) => div()
                .px(rem(tokens.sp_8))
                .pt(rem(tokens.sp_6))
                .pb(rem(tokens.sp_2))
                .text_size(rem(tokens.fs_sm))
                .weighted(tokens.weight_semibold)
                .text_color(tokens.fg_muted)
                .child(SharedString::from(words.clone()))
                .into_any_element(),
            Entry::Rule => div().h(rem(tokens.hairline)).mx(rem(tokens.sp_2)).my(rem(tokens.sp_4)).bg(tokens.line).into_any_element(),
            Entry::Item(item) => {
                let (does, press) = (item.does.clone(), press.clone());
                choice(at, item, cursor == Some(at), tokens)
                    .when(!item.disabled, |row| {
                        row.on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            press(at, &does, window, cx);
                        })
                    })
                    .into_any_element()
            }
        }))
}
