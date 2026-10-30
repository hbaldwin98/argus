//! The "N quiet repositories" row at the foot of the tree: the repositories with no panes, folded
//! into one line that opens to them (`project/Blend.dc.html`).
//!
//! Whether it is open is the window's own (`RailView::quiet_open`): it is a way of looking at the
//! list, so a press changes it at once and sends nothing. The repositories under it are the same
//! rows as any other, so choosing one sends the same call.

use argus_desktop_core::connection::page::{Rail, Repository};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Role};

use super::row::{line, Ctx};
use super::RailView;
use crate::kit::icons::Glyph;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn quiet(&self, quiet: &[&Repository], rail: &Rail, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if quiet.is_empty() || ctx.icons {
            return None;
        }
        let (open, fill) = (self.quiet_open, tokens.fill);
        let words = format!("{} quiet {}", quiet.len(), if quiet.len() == 1 { "repository" } else { "repositories" });
        let tokens = *tokens;
        let head = line("rail-quiet", 28.0, &tokens)
            .role(Role::Button)
            .aria_label(words.clone())
            .aria_expanded(open)
            .w_full()
            .px(rem(0.0))
            .gap(rem(8.0))
            .text_size(rem(12.5))
            .text_color(tokens.fg_faint)
            .hover(move |style| style.bg(fill).text_color(tokens.fg_muted))
            .on_click(cx.listener(|this, _, _, cx| {
                this.quiet_open = !this.quiet_open;
                cx.notify();
            }))
            .child(h_flex().w(rem(tokens.sp_16)).flex_none().justify_center().child(if open { Glyph::Down } else { Glyph::Right }.at(11.0)))
            .child(div().flex_1().min_w_0().truncate().child(words));
        let listed = open.then(|| quiet.iter().enumerate().map(|(at, repo)| self.repository(rail, repo, at == 0, ctx, &tokens, cx)).collect::<Vec<_>>());
        Some(
            v_flex()
                .id("rail-quiet-fold")
                .test_support()
                .flex_none()
                .mt(rem(tokens.sp_16))
                .child(div().mx(rem(10.0)).pt(rem(10.0)).border_t_1().border_color(tokens.line).child(head))
                .children(listed.into_iter().flatten()),
        )
    }
}
