//! The files column while the go-to-file list is open: the paths the field matches, best first,
//! the one Enter would read lit (`jumpList`, `code/stage.ts`). Core ranks them and keeps the first
//! fifty; a press on one reads it, and a field with no match says so. Each result is two lines, as
//! the board has them: the name first with the typed letters lit, the folder under it faint and
//! cut from its left (`typed`, `kit::path`).

use argus_desktop_core::code::side::jump_none;
use argus_desktop_core::code::file::path_parts;
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Context, FontWeight, SharedString, TestSupportExt as _};

use super::super::stage::CodeStage;
use super::typed;
use crate::kit::icons::Glyph;
use crate::kit::row::end_text;
use crate::kit::path::left_cut;
use crate::kit::selected::selected;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// A name with the letters the field lit set apart: heavier, in the text colour, underlined in the
/// accent.
fn name(path: &str, typed: &str, tokens: &Tokens) -> AnyElement {
    let (name, _) = path_parts(path);
    h_flex()
        .flex_none()
        .whitespace_nowrap()
        .text_size(rem(tokens.fs_md))
        .children(typed::lit(name, typed).into_iter().map(|(text, lit)| {
            let part = div().text_color(if lit { tokens.fg } else { tokens.fg_muted.blend(tokens.fg.opacity(0.6)) });
            if lit { part.font_weight(FontWeight(tokens.weight_semibold)).underline().text_decoration_color(tokens.accent) } else { part }.child(SharedString::from(text))
        }))
        .into_any_element()
}

impl CodeStage {
    pub(super) fn jump(&self, page: &CodePage, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(jump) = &page.jump else { return div().into_any_element() };
        let fill = tokens.fill;
        let rows = jump.matches.iter().enumerate().map(|(at, path)| {
            let on = jump.at == Some(at);
            let click = path.clone();
            let (_, dir) = path_parts(path);
            let changed = page.changed_files.iter().any(|file| &file.path == path);
            // `.hit`: the file glyph, then the name over the folder, `--row-h` and a half tall.
            let row = h_flex()
                .id(("code-jump", at))
                .test_support()
                .aria_label(SharedString::from(path.clone()))
                .aria_selected(on)
                .w_full()
                .min_h(rem(tokens.sp_44))
                .flex_shrink_0()
                .items_start()
                .gap(rem(tokens.sp_8))
                .px(rem(tokens.sp_8))
                .py(rem(tokens.sp_6))
                .rounded(rem(tokens.r_sm))
                .cursor_pointer()
                .when(!on, |row| row.hover(move |style| style.bg(fill)));
            selected(row, on, tokens)
                .on_click(cx.listener(move |this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Tree { path: click.clone() } }, cx)))
                .child(div().flex_none().mt(rem(tokens.sp_2)).text_color(tokens.fg_faint).child(Glyph::FileSm.at(tokens.icon_xs)))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(h_flex().child(name(path, &jump.query, tokens)).child(div().flex_1()).children(changed.then(|| end_text("M", tokens.changed, tokens))))
                        .children((!dir.is_empty()).then(|| h_flex().min_w_0().child(left_cut("code-jump-dir", SharedString::from(dir.to_owned()), tokens)))),
                )
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
