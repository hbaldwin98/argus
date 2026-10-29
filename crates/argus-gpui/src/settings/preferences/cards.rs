//! The theme cards: one per theme, each a small picture of that theme in its own colours and its
//! name under it, the one drawn lit with a tick (SHELL-SETTINGS-5, 6).
//!
//! A card's colours are its theme's whatever the window is drawn in (the page scopes a preview with
//! `data-theme`), so each is read from that theme's token space. The names and their order are core's
//! (`settings/flavours`); a press chooses the theme and the card that is lit is the one the session
//! says is drawn, so a refused choice leaves the lit card where it was.

use argus_desktop_core::connection::page::SettingChange;
use argus_desktop_core::settings::flavours::FLAVOURS;
use argus_desktop_core::settings::remembered::Theme as Chosen;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, px, relative, AnyElement, Context, Role, SharedString, TestSupportExt as _};

use super::PreferencesView;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::{choice, derive};
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// The picture: the theme's stage with its sidebar and two lines, the first in its accent.
fn preview(theme: Chosen, tokens: &Tokens) -> impl IntoElement {
    let own = derive::tokens(choice::theme(theme, true));
    h_flex()
        .h(rem(tokens.preview_h))
        .items_stretch()
        .gap(rem(tokens.sp_4))
        .p(rem(tokens.sp_6))
        .rounded(rem(tokens.r_sm))
        .bg(own.base)
        .child(div().flex_shrink_0().w(rem(tokens.preview_side_w)).rounded(rem(tokens.preview_rounding)).bg(own.mantle))
        .child(
            v_flex()
                .flex_1()
                .gap(rem(tokens.sp_4))
                .child(div().h(rem(tokens.preview_line_h)).w(relative(0.6)).rounded(rem(tokens.preview_rounding)).bg(own.mauve))
                .child(div().h(rem(tokens.preview_line_h)).rounded(rem(tokens.preview_rounding)).bg(own.subtext)),
        )
}

/// The cards in rows of `theme_cards_per_row`, `drawn` lit.
pub fn cards(drawn: Chosen, tokens: &Tokens, rem_size: gpui_kit::Pixels, cx: &mut Context<PreferencesView>) -> AnyElement {
    let tokens = *tokens;
    let card = |at: usize, theme: Chosen, name: &'static str, cx: &mut Context<PreferencesView>| {
        let on = theme == drawn;
        v_flex()
            .id(("themecard", at))
            .role(Role::RadioButton)
            .test_support()
            .aria_label(SharedString::from(name))
            .aria_selected(on)
            .flex_1()
            .min_w_0()
            .gap(rem(tokens.sp_6))
            .p(rem(tokens.sp_6))
            .rounded(rem(tokens.r_md))
            .border_1()
            .border_color(if on { tokens.accent } else { tokens.line_strong })
            // `--edge-accent`: a ring of the accent beside the border, drawn inside it (an outer shadow would fill the card).
            .when(on, |card| card.shadow(vec![gpui_kit::BoxShadow {
                color: tokens.accent,
                offset: gpui_kit::point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: pixels(tokens.hairline, rem_size),
                inset: true,
            }]))
            .cursor_pointer()
            .when(!on, |card| card.hover(move |style| style.border_color(tokens.fg_faint).bg(tokens.fill)))
            .on_click(cx.listener(move |this, _, _, _| this.set(SettingChange::Theme { theme })))
            .child(preview(theme, &tokens))
            .child(
                h_flex()
                    .min_w_0()
                    .gap(rem(tokens.sp_6))
                    .items_center()
                    .text_size(rem(tokens.fs_sm))
                    .weighted(tokens.weight_medium)
                    .child(div().min_w_0().truncate().child(SharedString::from(name)))
                    .when(on, |name| name.child(div().text_color(tokens.ok).child(Glyph::Check.at(tokens.icon_xs)))),
            )
    };
    let all: Vec<_> = FLAVOURS.iter().enumerate().map(|(at, (theme, name))| card(at, *theme, name, cx).into_any_element()).collect();
    let per_row = tokens.theme_cards_per_row;
    let mut rows = Vec::new();
    let mut cells = all.into_iter();
    loop {
        let mut row: Vec<AnyElement> = cells.by_ref().take(per_row).collect();
        if row.is_empty() {
            break;
        }
        // A short last row keeps the width of a card in each cell it has, as a grid does: the filler has a card's padding and border so its share matches.
        while row.len() < per_row {
            row.push(div().flex_1().min_w_0().p(rem(tokens.sp_6)).border_1().into_any_element());
        }
        rows.push(h_flex().gap(rem(tokens.sp_8)).children(row).into_any_element());
    }
    v_flex().id("themecards").role(Role::RadioGroup).test_support().aria_label("Theme").gap(rem(tokens.sp_8)).py(rem(tokens.sp_8)).children(rows).into_any_element()
}
