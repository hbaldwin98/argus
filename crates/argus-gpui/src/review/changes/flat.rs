//! The Changes pane's flat list: a commit's files, or the jump field's matches best first, each
//! as the lists draw a file, and a press on one opens it.

use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, ReviewClick, ReviewFile, ReviewPage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, SharedString, Window};

use crate::kit::counts::counts;
use crate::kit::long_list::long_list;
use crate::kit::tip::tip;
use crate::kit::path::path_label;
use crate::review::changes::lists::status;
use crate::review::changes::rows::letter;
use crate::review::changes::side::{lit, order, Files, Opened};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl Files {
    pub(super) fn list(&self, page: &Rc<ReviewPage>, tokens: &Tokens, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = v_flex().id("review-files-list").test_support().flex_1().min_h_0();
        if let Some(jump) = page.jump.as_ref().filter(|jump| jump.matches.is_empty()) {
            return list.pt(rem(tokens.sp_4)).child(
                div()
                    .id("review-jump-none")
                    .test_support()
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .child(SharedString::from(format!("No file matches \u{201c}{}\u{201d}.", jump.query))),
            );
        }
        let rows: Vec<usize> = order(page).into_iter().filter(|i| *i < page.files.len()).collect();
        // The gap between rows is the foot of each, so every row is one height.
        let heights = vec![FILE_ROW + tokens.sp_2; rows.len()];
        let page = page.clone();
        list.child(
            long_list(cx.entity(), "review-files-rows", &heights, window.rem_size(), &self.scroll, move |_, at, _, cx| {
                let tokens = *Tokens::global(cx);
                let i = rows[at];
                div().pb(rem(tokens.sp_2)).child(file_row(i, &page.files[i], Some(i) == lit(&page), &tokens, cx)).into_any_element()
            })
            .size_full()
            .pt(rem(tokens.sp_4))
            .pb(rem(10.0)),
        )
    }
}

/// How tall a file's row is in the lists, in the page's pixels.
pub const FILE_ROW: f32 = 28.0;

/// `.row`, as the lists draw a file: its status, name, folder and counts.
fn file_row(i: usize, file: &ReviewFile, on: bool, tokens: &Tokens, cx: &mut Context<Files>) -> impl IntoElement {
    let (fill, selected_hover, fg) = (tokens.fill, tokens.selected_hover, tokens.fg);
    let tooltip = SharedString::from(file.path.clone());
    let (mark, colour, said) = status(file.kind, tokens);
    h_flex()
        .id(("review-file", i))
        .test_support()
        .aria_selected(on)
        .aria_label(tooltip.clone())
        .flex_shrink_0()
        .h(rem(FILE_ROW))
        .mx(rem(tokens.sp_6))
        .pl(rem(tokens.sp_8))
        .pr(rem(tokens.sp_6))
        .gap(rem(tokens.sp_8))
        .items_center()
        .rounded(rem(tokens.r_sm))
        .border_1()
        .border_color(if on { tokens.line_strong } else { gpui_kit::transparent_black() })
        .typed(tokens.type_body)
        .text_color(if on { tokens.fg } else { tokens.fg_muted })
        .cursor_pointer()
        .when(on, |row| row.bg(tokens.bg_selected).hover(move |style| style.bg(selected_hover)))
        .when(!on, |row| row.hover(move |style| style.bg(fill.opacity(0.6)).text_color(fg)))
        .tooltip(tip(tooltip))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.emit(Opened);
            this.ask(Call::ReviewClick { target: ReviewClick::File { file: i } }, cx)
        }))
        .child(letter(("review-file-status", i), mark, colour, said, tokens))
        .child(div().flex_1().min_w_0().overflow_hidden().child(path_label(("review-file-path", i), &file.name, file.dir.trim_end_matches('/'), tokens)))
        .child(div().flex_shrink_0().text_size(rem(11.0)).child(counts(file.added, file.removed, tokens)))
}

