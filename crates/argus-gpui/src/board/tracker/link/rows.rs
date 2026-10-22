//! The tickets already linked, as the link dialog lists them (BOARD-LINK-7, -13): the tracker, the key in
//! the reference colour, what the ticket is to the feature, and Open and Unlink, the cursor row lit.
//!
//! A press on a row selects it, Open's and Unlink's included: neither stops the press, as the page's
//! do not.

use argus_desktop_core::board::tracker::link_dialog::{LinkGesture, LinkPage};
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Context, Role, SharedString, TestSupportExt as _};

use super::LinkView;
use crate::kit::button::{button, Look};
use crate::kit::dialog::{field_label, meta};
use crate::kit::selected::selected;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl LinkView {
    pub(super) fn linked(&self, page: &LinkPage, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let rows = page.linked.iter().enumerate().map(|(at, row)| {
            let (url, key) = (row.link.url.clone(), row.link.key.clone());
            let (asking, unlinking) = (self.caller.clone(), cx.entity());
            selected(
                h_flex()
                    .id(("link-row", at))
                    .role(Role::ListItem)
                    .test_support()
                    .aria_label(SharedString::from(format!("{} {}", row.link.tracker, row.link.key)))
                    .aria_selected(row.on)
                    .min_w_0()
                    .gap(rem(tokens.sp_8))
                    .items_center()
                    .px(rem(tokens.sp_8))
                    .py(rem(tokens.sp_6))
                    .rounded(rem(tokens.r_sm))
                    .text_size(rem(tokens.fs_sm))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, _| this.send(LinkGesture::Select { index: at })))
                    .child(div().w(rem(tokens.sp_32 * 2.0)).flex_shrink_0().text_color(tokens.fg_muted).child(SharedString::from(row.link.tracker.clone())))
                    .child(div().flex_shrink_0().font_family(SharedString::from(tokens.font_mono)).text_color(tokens.ref_).child(SharedString::from(row.link.key.clone())))
                    .child(div().flex_1().min_w_0().truncate().text_color(tokens.fg_muted).child(SharedString::from(row.link.role.clone())))
                    .child(
                        button(("link-open", at), SharedString::from(format!("Open {}", row.link.key)), Look::Plain, true, tokens)
                            .tooltip(tip(url.clone()))
                            .child("Open")
                            .on_click(move |_, _, _| asking.call(Call::OpenUrl { url: url.clone() })),
                    )
                    .child(
                        button(("link-unlink", at), SharedString::from(format!("Unlink {}", row.link.key)), Look::Plain, true, tokens)
                            .child("Unlink")
                            .on_click(move |_, _, cx| unlinking.update(cx, |this, _| this.send(LinkGesture::Unlink { key: key.clone() }))),
                    ),
                row.on,
                tokens,
            )
        });
        v_flex()
            .id("link-linked")
            .role(Role::Group)
            .test_support()
            .aria_label("Already linked")
            .gap(rem(tokens.sp_6))
            .child(field_label("Already linked", tokens))
            .children(page.linked.is_empty().then(|| meta("No ticket yet.", tokens)))
            .children(rows)
            .into_any_element()
    }
}

