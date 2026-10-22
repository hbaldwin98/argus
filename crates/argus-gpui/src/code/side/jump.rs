//! The files column while the go-to-file list is open: the paths the field matches, best first,
//! the one Enter would read lit (`jumpList`, `code/stage.ts`). Core ranks them and keeps the first
//! fifty; a press on one reads it, and a field with no match says so.

use argus_desktop_core::code::side::jump_none;
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Context, SharedString, TestSupportExt as _};

use super::super::stage::CodeStage;
use crate::kit::selected::selected;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl CodeStage {
    pub(super) fn jump(&self, page: &CodePage, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(jump) = &page.jump else { return div().into_any_element() };
        let fill = tokens.fill;
        let rows = jump.matches.iter().enumerate().map(|(at, path)| {
            let on = jump.at == Some(at);
            let click = path.clone();
            // `.trow`: a chevron slot that is empty here, the full path in mono, `--row-h` tall.
            let row = h_flex()
                .id(("code-jump", at))
                .test_support()
                .aria_label(SharedString::from(path.clone()))
                .aria_selected(on)
                .w_full()
                .h(rem(tokens.row_h))
                .flex_shrink_0()
                .items_center()
                .gap(rem(tokens.sp_6))
                .pl(rem(tokens.sp_6))
                .pr(rem(tokens.sp_8))
                .rounded(rem(tokens.r_sm))
                .text_size(rem(tokens.fs_md))
                .text_color(tokens.fg)
                .cursor_pointer()
                .when(!on, |row| row.hover(move |style| style.bg(fill)));
            selected(row, on, tokens)
                .on_click(cx.listener(move |this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Tree { path: click.clone() } }, cx)))
                .child(div().w(rem(tokens.icon_sm)).flex_shrink_0())
                .child(div().flex_1().min_w_0().truncate().font_family(SharedString::from(tokens.font_mono)).child(SharedString::from(path.clone())))
                .into_any_element()
        });
        v_flex()
            .id("code-list")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.jump_scroll)
            .gap(rem(tokens.hairline))
            .children(rows)
            .children(jump_none(jump).map(|none| {
                div().id("code-jump-none").test_support().aria_label(SharedString::from(none.clone())).typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(none)).into_any_element()
            }))
            .into_any_element()
    }
}
