//! The files column while the search is open: the files-to-include globs, a line saying how the
//! search went, and the hits grouped by file (`searchPanel`, `hitFile`, `code/stage.ts`).
//!
//! What was found, the order, which match the cursor is on and every sentence are core's; the hits
//! are drawn only while they answer the query and options as they stand (CODE-52). A press on a
//! file folds it, a press on a hit reads the match where it is, and the glob is searched with on
//! Enter, as the query is.

use argus_desktop_core::code::side::{glob_key, hit_title, summary, GLOB_HINT};
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodeHit, CodeHitFile, CodePage, CodeSearch};
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Place;
use gpui_kit::component::h_flex;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, AnyElement, Context, Entity, FontWeight, Focusable as _, Hsla, SharedString, Subscription, TestSupportExt as _, Window};

use super::super::stage::CodeStage;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::crop::{crop, Look};
use crate::kit::field::Field;
use crate::keys::layer::{action_for, Verdict};
use crate::kit::icons::Glyph;
use crate::kit::selected::selected;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The glob field's subscription to its edits and its place in the key walk.
pub(in crate::code) fn wire(glob: &Field, heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut Context<CodeStage>) -> (Subscription, Registered) {
    let input = glob.input().clone();
    let typing = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
        if !matches!(event, InputEvent::Change) {
            return;
        }
        let text = input.read(cx).value();
        if text != this.known_glob {
            this.known_glob = text.clone();
            this.send(CodeGesture::Glob { glob: text.to_string() }, cx);
        }
    });
    let (heard, caller) = (heard.clone(), caller.clone());
    let keys = Fields::of(cx).register(window, input.focus_handle(cx), move |press, _, cx| {
        let Some(gesture) = action_for(&[Place::Jump], press.page).and_then(glob_key) else { return Verdict::Held };
        if !heard.read(cx).held() {
            caller.call(Call::Code { gesture });
        }
        Verdict::Taken
    });
    (typing, keys)
}

/// A hit's text, the matched stretches lit, scrolled to the first and faded where it is cut.
/// `ground` is what lies behind the row, which the fade runs into.
fn hit_text(hit: &CodeHit, ground: Hsla, tokens: &Tokens) -> impl IntoElement {
    let mut text = String::new();
    let mut lit = Vec::new();
    for (part, matched) in &hit.parts {
        if *matched && !part.is_empty() {
            lit.push(text.len()..text.len() + part.len());
        }
        text.push_str(part);
    }
    let look = Look {
        family: SharedString::from(tokens.font_mono),
        size: tokens.fs_sm,
        line_height: tokens.code_line_h,
        colour: if hit.on { tokens.fg } else { tokens.fg_muted },
        mark_colour: tokens.fg,
        mark_background: tokens.attention.opacity(tokens.share_hit),
        ground,
        lead: tokens.hline_lead,
        fade: tokens.hline_fade,
    };
    crop(SharedString::from(text), lit, look).flex_1().min_w_0().h(rem(tokens.code_line_h))
}

impl CodeStage {
    /// `.hfile`: a file of the results, folded or open, with how many matches it holds.
    fn hit_file(&self, at: usize, file: &CodeHitFile, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let path = file.path.clone();
        let fill = tokens.fill;
        h_flex()
            .id(("code-hit-file", at))
            .test_support()
            .aria_label(SharedString::from(file.path.clone()))
            .aria_expanded(file.open)
            .tooltip(tip(file.path.clone()))
            .w_full()
            .h(rem(tokens.row_h_sm))
            .flex_shrink_0()
            .items_center()
            .gap(rem(tokens.sp_6))
            .pl(rem(tokens.sp_6))
            .pr(rem(tokens.sp_8))
            .rounded(rem(tokens.r_sm))
            .text_size(rem(tokens.fs_md))
            .text_color(tokens.fg)
            .cursor_pointer()
            .hover(move |style| style.bg(fill))
            .on_click(cx.listener(move |this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::HitFile { path: path.clone() } }, cx)))
            .child(div().flex_shrink_0().w(rem(tokens.icon_sm)).flex().justify_center().text_color(tokens.fg_faint).child(if file.open { Glyph::Down } else { Glyph::Right }.at(tokens.icon_sm)))
            // The file's own name first, which is what is scanned for; its folder trails it and gives way.
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(rem(tokens.sp_8))
                    .child(div().flex_shrink_0().font_weight(FontWeight(tokens.weight_semibold)).child(SharedString::from(file.name.clone())))
                    .child(div().min_w_0().truncate().text_size(rem(tokens.fs_sm)).text_color(tokens.fg_faint).child(SharedString::from(file.dir.clone()))),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px(rem(tokens.sp_6))
                    .rounded(rem(tokens.r_pill))
                    .bg(tokens.fill)
                    .text_size(rem(tokens.fs_xs))
                    .font_weight(FontWeight(tokens.weight_semibold))
                    .text_color(tokens.fg_muted)
                    .child(file.count.to_string()),
            )
            .into_any_element()
    }

    /// `.hline`: one match, its line number and its text with the hit lit.
    fn hit_line(&self, hit: &CodeHit, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let index = hit.index;
        let fill = tokens.fill;
        let row = h_flex()
            .id(("code-hit", index))
            .test_support()
            .aria_label(SharedString::from(hit_title(hit)))
            .aria_selected(hit.on)
            .tooltip(tip(hit_title(hit)))
            .w_full()
            .min_h(rem(tokens.row_h_sm))
            .flex_shrink_0()
            .items_start()
            .gap(rem(tokens.sp_6))
            .pl(rem(tokens.hline_indent))
            .pr(rem(tokens.sp_8))
            .py(rem(tokens.sp_4))
            .rounded(rem(tokens.r_sm))
            .font_family(SharedString::from(tokens.font_mono))
            .text_size(rem(tokens.fs_sm))
            .line_height(rem(tokens.code_line_h))
            .text_color(if hit.on { tokens.fg } else { tokens.fg_muted })
            .cursor_pointer()
            .when(!hit.on, |row| row.hover(move |style| style.bg(fill)));
        selected(row, hit.on, tokens)
            .on_click(cx.listener(move |this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Match { index } }, cx)))
            .child(div().flex_shrink_0().w(rem(tokens.hline_no_w)).flex().justify_end().text_color(tokens.fg_faint).child(hit.line.to_string()))
            .child(hit_text(hit, if hit.on { tokens.bg_selected } else { tokens.bg_chrome }, tokens))
            .into_any_element()
    }

    fn hits(&self, search: &CodeSearch, tokens: &Tokens, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if !search.current {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (at, file) in search.files.iter().enumerate() {
            out.push(self.hit_file(at, file, tokens, cx));
            if file.open {
                out.extend(file.hits.iter().map(|hit| self.hit_line(hit, tokens, cx)));
            }
        }
        out
    }

    pub(super) fn search(&self, page: &CodePage, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(search) = &page.search else { return div().into_any_element() };
        let said = summary(search);
        let glob = div()
            .id("code-glob-box")
            .test_support()
            .flex_shrink_0()
            .child(crate::kit::finder::field_input(self.glob.input(), "code-glob", GLOB_HINT, tokens));
        v_flex()
            .flex_1()
            .min_h_0()
            .gap(rem(tokens.sp_6))
            .child(glob)
            .child(
                div()
                    .id("code-summary")
                    .test_support()
                    .aria_label(SharedString::from(said.text.clone()))
                    .flex_shrink_0()
                    .px(rem(tokens.sp_4))
                    .text_size(rem(tokens.fs_sm))
                    .text_color(if said.danger { tokens.danger } else { tokens.fg_muted })
                    .child(SharedString::from(said.text)),
            )
            .child(
                v_flex()
                    .id("code-list")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.hits_scroll)
                    .gap(rem(tokens.hairline))
                    .children(self.hits(search, tokens, cx)),
            )
            .into_any_element()
    }
}

