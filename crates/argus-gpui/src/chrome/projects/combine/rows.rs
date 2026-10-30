//! The projects to fold in: one ticked row each, with what the project is, and the sentence that
//! names where their repositories go.
//!
//! A row is the control, the whole width of it: it draws the tick the page says and asks for the
//! opposite (`Tick`), so nothing is ticked until the session says so.

use argus_desktop_core::chrome::combine_outcome::{repositories, status_note, NOTHING_TO_FOLD, SCANS_A_FOLDER};
use argus_desktop_core::chrome::combine_page::{CombineGesture, CombinePage, CombineRow};
use argus_desktop_core::chrome::projects::Tone;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, Role, SharedString, TestSupportExt as _, Toggled};

use super::CombineView;
use crate::kit::caption::note;
use crate::kit::dialog::label;
use crate::kit::mixed::{mixed_box, Mark};
use crate::kit::selected::selected;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl CombineView {
    pub(super) fn rows(&self, page: &CombinePage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let into = page.into.as_ref().map_or("", |into| into.name.as_str());
        let heading = format!("Move these projects\u{2019} repositories into {into}:");
        v_flex()
            .id("cp-rows")
            .gap(rem(tokens.sp_6))
            .child(label(heading, tokens))
            .children(page.rows.is_empty().then(|| note("cp-none", NOTHING_TO_FOLD, tokens)))
            .children(page.rows.iter().enumerate().map(|(at, row)| self.row(at, row, tokens, cx)))
    }

    fn row(&self, at: usize, row: &CombineRow, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let project = row.id;
        let status = status_note(row).map(|tally| {
            let colour = match tally.tone {
                Tone::Loud => tokens.attention,
                Tone::Active => tokens.accent,
                Tone::Muted => tokens.fg_faint,
            };
            div().flex_shrink_0().typed(tokens.type_small).text_color(colour).child(SharedString::from(tally.words))
        });
        let mark = if row.ticked { Mark::Checked } else { Mark::Clear };
        selected(
            h_flex()
                .id(("cp-row", at))
                .role(Role::CheckBox)
                .test_support()
                .aria_label(SharedString::from(row.name.clone()))
                .aria_toggled(if row.ticked { Toggled::True } else { Toggled::False })
                .gap(rem(tokens.sp_8))
                .items_center()
                .min_h(rem(tokens.row_h))
                .px(rem(tokens.sp_8 - tokens.hairline))
                .rounded(rem(tokens.r_sm))
                .cursor_pointer()
                .when(!row.ticked, |row| row.hover(move |style| style.bg(tokens.fill)))
                .on_click(cx.listener(move |this, _, _, _| this.send(CombineGesture::Tick { project })))
                .child(mixed_box(("cp-box", at), "", mark, tokens))
                .child(div().min_w_0().truncate().text_color(tokens.fg).child(SharedString::from(row.name.clone())))
                .children(status)
                .children(row.scans_folder.then(|| div().flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child(SCANS_A_FOLDER)))
                .child(div().flex_1())
                .child(div().flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(repositories(row.repositories)))),
            row.ticked,
            tokens,
        )
    }
}

