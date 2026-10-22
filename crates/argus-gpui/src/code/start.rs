//! What fills the Code stage's file area while no file is read (`start`, `code/stage.ts`): the
//! files this version changed and the ones opened lately, each a row that reads it, and a hint
//! about the field; or, while the search's cursor is on a match, a card that offers to open it.
//!
//! The lists and their words are core's (`code::start`); the files opened lately are the window's
//! memory (`Heard::opened`), kept beside what the session sends.

use argus_desktop_core::chrome::strip::Part;
use argus_desktop_core::code::stage::{CodeClick, CodeGesture};
use argus_desktop_core::code::start::{Selected, Start, FileLine, HINT};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, FontWeight, SharedString, TestSupportExt as _};

use crate::code::stage::CodeStage;
use crate::kit::button::{button, Look};
use crate::kit::flow::{flow, Piece};
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl CodeStage {
    /// A file in a list: its name, then its folder faint in mono, and how much it changed.
    fn file_line(&self, id: (&'static str, usize), line: &FileLine, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let path = line.path.clone();
        let fill = tokens.fill;
        h_flex()
            .id(id)
            .test_support()
            .aria_label(SharedString::from(line.path.clone()))
            .w_full()
            .h(rem(tokens.cs_row_h))
            .flex_shrink_0()
            .items_center()
            .gap(rem(tokens.sp_12))
            .px(rem(tokens.sp_12))
            .rounded(rem(tokens.r_md))
            .text_color(tokens.fg)
            .cursor_pointer()
            .hover(move |style| style.bg(fill))
            .on_click(cx.listener(move |this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Tree { path: path.clone() } }, cx)))
            .child(div().flex_shrink_0().text_color(tokens.fg_muted).child(Glyph::FileSm.at(tokens.icon_sm)))
            .child(div().flex_shrink_0().child(SharedString::from(line.name.clone())))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(SharedString::from(tokens.font_mono))
                    .text_size(rem(tokens.fs_sm))
                    .text_color(tokens.fg_faint)
                    .child(SharedString::from(line.dir.clone())),
            )
            .when_some(line.counts.filter(|(added, removed)| *added > 0 || *removed > 0), |row, (added, removed)| {
                row.child(
                    h_flex()
                        .gap(rem(tokens.sp_2))
                        .font_family(SharedString::from(tokens.font_mono))
                        .text_size(rem(tokens.fs_xs))
                        .when(added > 0, |counts| counts.child(div().text_color(tokens.ok).child(format!("+{added}"))))
                        .when(removed > 0, |counts| counts.child(div().text_color(tokens.danger).child(format!("\u{2212}{removed}")))),
                )
            })
    }

    fn section(&self, heading: String, tokens: &Tokens) -> gpui_kit::Div {
        v_flex().max_w(rem(tokens.cs_sec_max_w)).child(div().mb(rem(tokens.sp_8)).text_size(rem(tokens.fs_lg)).font_weight(FontWeight(tokens.weight_bold)).child(SharedString::from(heading)))
    }

    /// The start panel for `said`.
    pub(crate) fn start(&self, said: &Start, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = SharedString::from(tokens.font_mono);
        let changed = if said.changed.is_empty() {
            div()
                .id("code-nothing")
                .test_support()
                .aria_label(SharedString::from(said.nothing.iter().map(|part| match part { Part::Text(t) | Part::Mono(t) => t.as_str() }).collect::<String>()))
                .text_color(tokens.fg_muted)
                .children(said.nothing.iter().map(|part| match part {
                    Part::Text(text) => div().child(SharedString::from(text.clone())).into_any_element(),
                    Part::Mono(text) => div().font_family(mono.clone()).child(SharedString::from(text.clone())).into_any_element(),
                }))
                .flex()
                .into_any_element()
        } else {
            v_flex()
                .gap(rem(tokens.hairline * 2.0))
                .children(said.changed.iter().enumerate().map(|(at, line)| self.file_line(("code-changed", at), line, tokens, cx).into_any_element()))
                .when_some(said.more.clone(), |list, more| {
                    list.child(div().id("code-more").test_support().aria_label(SharedString::from(more.clone())).typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(more)))
                })
                .into_any_element()
        };
        let lately = (!said.lately.is_empty()).then(|| {
            self.section("Opened lately".into(), tokens)
                .child(v_flex().gap(rem(tokens.hairline * 2.0)).children(said.lately.iter().enumerate().map(|(at, line)| self.file_line(("code-lately", at), line, tokens, cx).into_any_element())))
        });
        let (before, key, after) = HINT;
        // `.code-start`: 44px in, 32 between its sections.
        v_flex()
            .id("code-start")
            .test_support()
            .aria_label("Nothing open")
            .size_full()
            .overflow_y_scroll()
            .p(rem(tokens.sp_44))
            .gap(rem(tokens.sp_32))
            .typed(tokens.type_body)
            .text_color(tokens.fg)
            .child(self.section(said.heading.clone(), tokens).child(changed))
            .children(lately)
            .child(
                flow(vec![Piece::Run(before.to_owned()), Piece::Part(keycap(key, mono, tokens).into_any_element()), Piece::Run(after.to_owned())])
                    .id("code-hint")
                    .test_support()
                    .aria_label(SharedString::from(format!("{before}{key}{after}")))
                    .max_w(rem(tokens.cs_hint_max_w))
                    .text_size(rem(tokens.fs_sm))
                    .line_height(rem(tokens.fs_sm * 1.6))
                    .text_color(tokens.fg_faint),
            )
    }

    /// The card that offers the match the search's cursor is on, in the place of the start panel.
    pub(crate) fn match_card(&self, found: &Selected, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let index = found.index;
        let place = SharedString::from(found.place.clone());
        v_flex()
            .id("code-match")
            .test_support()
            .aria_label("Open the selected match")
            .size_full()
            .items_center()
            .justify_center()
            .gap(rem(tokens.sp_16))
            .p(rem(tokens.sp_24))
            .text_center()
            .child(div().text_color(tokens.fg_faint).child(Glyph::File.at(tokens.icon_lg)))
            .child(div().typed(tokens.type_title).text_color(tokens.fg).child("Open the selected match"))
            .child(
                flow(vec![
                    Piece::Part(div().font_family(SharedString::from(tokens.font_mono)).child(place).into_any_element()),
                    Piece::Text(" is the one under the cursor in the results.".into()),
                ])
                .justify_center()
                .text_color(tokens.fg_muted),
            )
            .child(
                button("code-open-match", "Open it", Look::Primary, false, tokens)
                    .child("Open it")
                    .child(keycap("Enter", SharedString::from(tokens.font_mono), tokens))
                    .on_click(cx.listener(move |this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Match { index } }, cx))),
            )
    }
}
