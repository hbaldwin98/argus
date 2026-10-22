//! The reconcile dialog's frame and its body by stage (BOARD-RECONCILE-3): with nothing under way, the
//! ask; once asked, a waiting line over the ask; with a proposal, the lines at the left and the line
//! open at the right.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, Role, TestSupportExt as _, Window};

use super::ReconcileView;
use crate::kit::dialog::meta;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

impl Render for ReconcileView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.page(cx) else { return div().into_any_element() };
        let body = match page.stage {
            "none" => v_flex().child(self.ask(&page, &tokens, cx)).into_any_element(),
            "asked" => v_flex()
                .child(
                    meta(
                        format!("{} is reading the map, the feature and the tickets; its proposal shows here when it sends one.", page.by.clone().unwrap_or_default()),
                        &tokens,
                    )
                    .mb(rem(tokens.sp_12)),
                )
                .child(self.ask(&page, &tokens, cx))
                .into_any_element(),
            _ => h_flex().size_full().min_h_0().gap(rem(tokens.sp_20)).child(self.list(&page, &tokens, cx)).child(self.detail(&page, &tokens)).into_any_element(),
        };
        let splits = !matches!(page.stage, "none" | "asked");
        v_flex()
            .id("reconcile-dialog")
            .role(Role::Dialog)
            .test_support()
            .aria_label("Reconcile")
            .track_focus(&self.card)
            .w_full()
            .h(pixels(tokens.settings_h, window.rem_size()).min(window.viewport_size().height * tokens.sheet_max_share))
            .child(self.head(&page, &tokens, cx))
            .child(
                div()
                    .id("reconcile-body")
                    .flex_1()
                    .min_h_0()
                    .px(rem(tokens.sp_24))
                    .pt(rem(tokens.sp_16))
                    .pb(rem(tokens.sp_24))
                    // The split scrolls in its own two columns; the asks scroll as one.
                    .when(!splits, |body| body.overflow_y_scroll())
                    .child(body),
            )
            .child(self.foot(&page, &tokens, cx))
            .into_any_element()
    }
}

