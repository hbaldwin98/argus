//! What runs on the selected checkout: one button per pane, a dot, the title cut short and the
//! status word in the colour of how loud it is. A press goes to the pane (`.cs-running`).

use argus_desktop_core::checkouts::table::Running;
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Role, SharedString, TestSupportExt as _};

use super::super::table::Ctx;
use crate::kit::pill::status_word;
use crate::kit::status_dot::status_dot;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The colour a status word is said in beside a pane's title.
fn tone(status: &str, tokens: &Tokens) -> gpui_kit::Hsla {
    match status {
        "working" => tokens.accent,
        "waiting" | "needs-review" => tokens.attention,
        "failed" => tokens.danger,
        _ => tokens.fg_faint,
    }
}

/// The section: its heading, `Running here`, and a button per pane.
pub fn running(heading: &str, panes: &[Running], ctx: &Ctx, tokens: &Tokens) -> impl IntoElement {
    let fill = tokens.fill;
    v_flex()
        .id("side-running")
        .test_support()
        .child(
            div()
                .mb(rem(tokens.sp_6))
                .text_size(rem(tokens.fs_sm))
                .weighted(tokens.weight_semibold)
                .text_color(tokens.fg_muted)
                .id("side-running-heading")
                .test_support()
                .aria_label(SharedString::from(heading.to_owned()))
                .child(SharedString::from(heading.to_owned())),
        )
        .children(panes.iter().map(|pane| {
            let (caller, heard, id) = (ctx.caller.clone(), ctx.heard.clone(), pane.id);
            h_flex()
                .id(("side-pane", pane.id))
                .role(Role::Button)
                .test_support()
                .aria_label(SharedString::from(pane.title.clone()))
                .tooltip(tip(if pane.agent { "an agent" } else { "a shell" }))
                .w_full()
                .h(rem(tokens.ck_pane_h))
                .px(rem(tokens.sp_8))
                .gap(rem(tokens.sp_8))
                .items_center()
                .rounded(rem(tokens.r_sm))
                .text_size(rem(tokens.fs_sm))
                .text_color(tokens.fg)
                .cursor_pointer()
                .hover(move |style| style.bg(fill))
                .on_click(move |_, _, cx| {
                    if !heard.read(cx).held() {
                        caller.call(Call::GoToPane { pane: id });
                    }
                })
                .child(status_dot(format!("side-pane-{}", pane.id), pane.status, tokens))
                .child(div().flex_1().min_w_0().truncate().child(SharedString::from(pane.title.clone())))
                .child(div().text_size(rem(tokens.fs_xs)).text_color(tone(pane.status, tokens)).child(SharedString::from(status_word(pane.status).to_owned())))
        }))
}
