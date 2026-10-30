//! The filter chips under the panel's header: All, what needs the person, what is at work.
//!
//! The counts are the ones the rail was told (`Rail::needs_you`, `Rail::working`). Which chip is on
//! is the window's own state (`Filter`), since it is a way of looking at the tree and not something
//! the daemon holds: a press changes the tree at once and sends nothing.

use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Context, Role, SharedString, TestSupportExt as _};

use super::filter::Filter;
use super::RailView;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn chips(&self, needs_you: usize, working: usize, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let chip = |id: &'static str, which: Filter, words: String, mark: Option<gpui_kit::AnyElement>, count: Option<usize>, cx: &mut Context<Self>| {
            let on = self.filter == which;
            let fill = tokens.fill;
            h_flex()
                .id(id)
                .role(Role::Button)
                .test_support()
                .aria_label(words)
                .aria_selected(on)
                .h(rem(24.0))
                .px(rem(9.0))
                .gap(rem(5.0))
                .items_center()
                .flex_none()
                .rounded(rem(12.0))
                .border_1()
                .border_color(if on { tokens.line_strong } else { tokens.line })
                .text_size(rem(tokens.fs_sm))
                .weighted(tokens.weight_medium)
                .text_color(if on { tokens.fg } else { tokens.fg_muted })
                .cursor_pointer()
                .when(on, |chip| chip.bg(tokens.bg_selected))
                .when(!on, |chip| chip.hover(move |style| style.bg(fill)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.filter = which;
                    cx.notify();
                }))
                .children(mark)
                .child(match count {
                    Some(count) => count.to_string(),
                    None => "All".to_owned(),
                })
        };
        let need_mark = div().size(rem(8.0)).flex_none().rounded_full().bg(tokens.attention).into_any_element();
        let work_mark = Glyph::Arc.at(9.0).text_color(tokens.accent).into_any_element();
        h_flex()
            .id("rail-chips")
            .test_support()
            .role(Role::Group)
            .aria_label(SharedString::from("Show"))
            .flex_none()
            .gap(rem(tokens.sp_6))
            .px(rem(10.0))
            .pt(rem(10.0))
            .pb(rem(tokens.sp_4))
            .child(chip("rail-filter-all", Filter::All, "All".into(), None, None, cx))
            .child(chip("rail-filter-need", Filter::NeedsYou, format!("{needs_you} need you"), Some(need_mark), Some(needs_you), cx))
            .child(chip("rail-filter-working", Filter::Working, format!("{working} working"), Some(work_mark), Some(working), cx))
    }
}
