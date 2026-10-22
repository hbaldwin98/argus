//! The right column of the Code stage, shown while the outline is on and a file is read: its
//! header with the button that puts it away, what the file defines, and the changes in it
//! (`outline`, `code/stage.ts`), and the sash that sizes it.
//!
//! Which definition the cursor is in, which changed and what each change says is core's, sent in the
//! page (`CodeOutlineRow`, `CodeChangeRow`); a row sends the line it stands for.

use argus_desktop_core::code::outline::{badge, empty, CHANGES, CLOSE, HEADING};
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, relative, AnyElement, Context, FontWeight, SharedString, TestSupportExt as _};

use super::CodeStage;
use crate::kit::icon_button::{icon_button, Node};
use crate::kit::icons::Glyph;
use crate::kit::selected::selected;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl CodeStage {
    /// `.code-outline`: `width` wide and at most `at_most` of the stage, a hairline on its left that
    /// is also its sash, 8px in.
    pub(super) fn outline(&self, page: &CodePage, width: f32, at_most: f32, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let symbols = page.symbols.iter().enumerate().map(|(at, symbol)| {
            let line = symbol.line;
            self.row(("code-symbol", at), symbol.name.clone(), symbol.on, line, tokens, cx)
                .child(badge_box(("code-symbol-kind", at), badge(symbol.kind), Some(symbol.kind), tokens))
                .child(name(&symbol.name))
                .when(symbol.changed, |row| row.child(div().id(("code-symbol-changed", at)).test_support().aria_label("changed").flex_shrink_0().size(rem(tokens.dot_sm)).rounded_full().bg(tokens.changed)))
        }).collect::<Vec<_>>();
        let changes = page.changes_here.iter().enumerate().map(|(at, change)| {
            let line = change.line;
            self.row(("code-change", at), change.text.clone(), change.on, line, tokens, cx)
                .child(badge_box(("code-change-line", at), line.to_string(), None, tokens))
                .child(name(&change.text))
        }).collect::<Vec<_>>();
        v_flex()
            .id("code-outline")
            .test_support()
            .aria_label("Outline")
            .relative()
            .flex_shrink_0()
            .w(rem(width))
            .max_w(relative(at_most))
            .h_full()
            .min_h_0()
            .border_l_1()
            .border_color(tokens.line)
            .px(rem(tokens.sp_8))
            .pt(rem(tokens.sp_8))
            .pb(rem(tokens.sp_12))
            .gap(rem(tokens.hairline))
            .overflow_hidden()
            .child(
                h_flex().items_center().pl(rem(tokens.sp_8)).child(
                    div().flex_1().text_size(rem(tokens.fs_sm)).font_weight(FontWeight(tokens.weight_semibold)).text_color(tokens.fg_muted).child(HEADING),
                ).child(
                    icon_button("code-outline-close", CLOSE, false, tokens)
                        .tooltip(tip(CLOSE))
                        .child(Glyph::Close.at(tokens.icon_sm))
                        .on_click(cx.listener(|this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Outline }, cx))),
                ),
            )
            // The rows scroll under the header, as `.code-outline` does on its own.
            .child(
                v_flex()
                    .id("code-outline-rows")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .gap(rem(tokens.hairline))
                    .overflow_y_scroll()
                    .when(page.symbols.is_empty(), |rows| {
                        rows.child(div().id("code-outline-none").test_support().aria_label(empty(page)).typed(tokens.type_small).text_color(tokens.fg_muted).child(empty(page)))
                    })
                    .children(symbols)
                    .when(!page.changes_here.is_empty(), |rows| {
                        rows.child(
                            div()
                                .mt(rem(tokens.sp_12))
                                .px(rem(tokens.sp_8))
                                .pt(rem(tokens.sp_6))
                                .pb(rem(tokens.sp_2))
                                .text_size(rem(tokens.fs_sm))
                                .font_weight(FontWeight(tokens.weight_semibold))
                                .text_color(tokens.fg_muted)
                                .child(CHANGES),
                        )
                    })
                    .children(changes),
            )
            // The grab zone along the column's left edge.
            .child(div().absolute().top_0().bottom_0().left_0().w(rem(tokens.sp_6)).child(self.outline_split.clone()))
            .into_any_element()
    }

    /// `.orow`: a badge, a name and a dot, `--row-h` tall in the mono face; the one the cursor is in
    /// is the selected one.
    fn row(&self, id: (&'static str, usize), title: String, on: bool, line: u32, tokens: &Tokens, cx: &mut Context<Self>) -> Node {
        let fill = tokens.fill;
        let row = h_flex()
            .id(id)
            .test_support()
            .aria_label(SharedString::from(title))
            .aria_selected(on)
            .flex_shrink_0()
            .w_full()
            .h(rem(tokens.row_h))
            .px(rem(tokens.sp_8))
            .gap(rem(tokens.sp_6))
            .items_center()
            .rounded(rem(tokens.r_sm))
            .font_family(SharedString::from(tokens.font_mono))
            .text_size(rem(tokens.fs_sm))
            .text_color(tokens.fg)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Go { line } }, cx)));
        let row = selected(row, on, tokens);
        if on { row } else { row.hover(move |style| style.bg(fill)) }
    }
}

/// `.okind`: an 18px box on the fill, in the reference colour, bold and small.
fn badge_box(id: (&'static str, usize), text: String, kind: Option<&'static str>, tokens: &Tokens) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .flex_shrink_0()
        .size(rem(tokens.code_badge))
        .rounded(rem(tokens.r_xs))
        .bg(tokens.fill)
        .flex()
        .items_center()
        .justify_center()
        .text_size(rem(tokens.fs_xs))
        .font_weight(FontWeight(tokens.weight_bold))
        .text_color(tokens.ref_)
        .when_some(kind.filter(|kind| !kind.is_empty()), |badge, kind| badge.tooltip(tip(kind)))
        .child(SharedString::from(text))
}

fn name(text: &str) -> gpui_kit::Div {
    div().flex_1().min_w_0().truncate().child(SharedString::from(text.to_owned()))
}
