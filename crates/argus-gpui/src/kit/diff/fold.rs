//! The lines between hunks the diff did not carry, and the buttons that ask for them.

use argus_desktop_core::connection::page::DiffRow;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString, TestSupportExt as _};

use super::shape;
use super::{DiffEvent, DiffView};
use crate::theme::Tokens;

impl DiffView {
    /// The lines between hunks the diff did not carry, and the buttons that
    /// ask for them. The cursor may rest on one, so it takes a click too.
    pub(super) fn fold(&self, ix: usize, row: &DiffRow, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let number = row.row;
        let button = |id: &'static str, label: String, all: bool, cx: &mut Context<Self>| {
            div()
                .id((id, ix))
                .test_support()
                .aria_label(SharedString::from(label.clone()))
                // `.fbtn`: the reference colour in the interface face, lifting to `--fill`.
                .px(px(tokens.sp_4))
                .h(px(tokens.fold_btn_h))
                .flex()
                .items_center()
                .rounded(px(tokens.r_xs))
                .font_family(SharedString::from(tokens.font_ui))
                .text_size(px(tokens.fs_sm))
                .text_color(tokens.ref_)
                .cursor_pointer()
                .hover({
                    let fill = tokens.fill;
                    move |style| style.bg(fill)
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    // The row under it would take the click as a move first.
                    cx.stop_propagation();
                    cx.emit(DiffEvent::Fold { row: number, all });
                }))
                .child(SharedString::from(label))
        };
        self.wide(h_flex())
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(px(shape::FOLD))
            .px(px(tokens.sp_12))
            .gap(px(tokens.sp_8))
            .items_center()
            .bg(tokens.bg_chrome)
            .text_color(tokens.fg_muted)
            .font_family(SharedString::from(tokens.font_ui))
            .text_size(px(tokens.fs_sm))
            .border_t_1()
            .border_b_1()
            .border_dashed()
            .border_color(if row.cursor { tokens.line_strong } else { tokens.line })
            .child(SharedString::from(format!("\u{22ef} {}", row.text)))
            .when(self.interactive, |fold| {
                fold.children(match row.step {
                    Some(step) => vec![
                        button("diff-fold", format!("Show {step}"), false, cx),
                        button("diff-fold-all", "Show all".to_owned(), true, cx),
                    ],
                    None => vec![button("diff-fold", "Show".to_owned(), true, cx)],
                })
            })
            .when(self.interactive, |fold| fold.cursor_text())
            .when(self.interactive, |fold| {
                fold.on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                    cx.emit(DiffEvent::Row { row: number, extend: event.modifiers().shift });
                }))
            })
            .into_any_element()
    }
}
