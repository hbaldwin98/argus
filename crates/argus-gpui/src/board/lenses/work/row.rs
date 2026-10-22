//! One row of the Work outline (BOARD-WORK-3, -9): a mark, a label, the words, its ticket's key, a dot
//! when it changed since the last reconcile, and what stands at its end; a click puts the cursor on it
//! and a double-click opens it to be edited.
//!
//! What each part says is `marks`; this puts them in the row's four columns, indented 8 and 20 for each
//! level it sits under.

use argus_desktop_core::connection::page::{Call, WorkRow};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use super::marks::{aside, label_tone, mark_word, state_mark, text, Aside};
use crate::board::feature_view::OpenFeature;
use crate::kit::caption::meta;
use crate::kit::chips::{cdot, tkey};
use crate::kit::icons::Glyph;
use crate::kit::pill::status_pill;
use crate::kit::selected::selected;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What stands at the row's end.
fn at_end(row: &WorkRow, ix: usize, tokens: &Tokens) -> Option<AnyElement> {
    Some(match aside(row) {
        Aside::Nothing => return None,
        Aside::Waiting => status_pill(("wrow-waiting", ix), "needs you", "waiting", tokens).into_any_element(),
        Aside::WaitsOn(label) => meta(tokens)
            .id(("wrow-waits", ix))
            .test_support()
            .aria_label(SharedString::from(format!("waits on {label}")))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(rem(tokens.sp_4))
            .whitespace_nowrap()
            .child(Glyph::Lock.at(tokens.icon_xs))
            .child(SharedString::from(format!("waits on {label}")))
            .into_any_element(),
        Aside::State(word) => meta(tokens).id(("wrow-state", ix)).test_support().aria_label(SharedString::from(word.clone())).flex_shrink_0().child(SharedString::from(word)).into_any_element(),
        Aside::Subtasks(done, total) => meta(tokens).id(("wrow-sub", ix)).test_support().aria_label(SharedString::from(format!("{done}/{total}"))).flex_shrink_0().child(SharedString::from(format!("{done}/{total}"))).into_any_element(),
    })
}

/// The row at `ix` of the outline. `mixed` is whether a changed dot says anything here.
pub fn row(ix: usize, row: &WorkRow, mixed: bool, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let (mark, tone) = state_mark(row.state);
    let words = text(row.state, row.on);
    let (selecting, editing) = (feature.clone(), feature.clone());
    let at = row.at;
    let mut body = div()
        .min_w_0()
        .truncate()
        .when(words.strong, |text| text.weighted(tokens.weight_semibold))
        .when(words.muted, |text| text.text_color(tokens.fg_muted))
        .when(words.struck, |text| text.text_color(tokens.fg_faint).line_through())
        .child(SharedString::from(row.text.clone()));
    if words.strong {
        body = body.text_color(tokens.fg);
    }
    let line = h_flex()
        .id(("wrow", ix))
        .test_support()
        .aria_label(SharedString::from(format!("{} {}", row.label, row.text)))
        .aria_selected(row.on)
        .flex_shrink_0()
        .min_h(rem(tokens.control_h))
        .pl(rem(tokens.sp_8 + row.depth as f32 * tokens.row_indent))
        .pr(rem(tokens.sp_8))
        .gap(rem(tokens.sp_6))
        .items_center()
        .rounded(rem(tokens.r_sm))
        .cursor_pointer()
        .text_color(tokens.fg)
        .when(!row.on, |line| line.hover(|line| line.bg(tokens.fill)))
        // The state's words, said to a pointer over its mark.
        .child(div().id(("wrow-mark", ix)).flex_shrink_0().w(rem(tokens.wrow_mark_w)).text_color(tone.colour(tokens)).tooltip(tip(mark_word(row.state).to_owned())).child(mark))
        .child(div().flex_shrink_0().min_w(rem(tokens.wrow_label_min)).font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_xs)).text_color(label_tone(row).colour(tokens)).child(SharedString::from(row.label.clone())))
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(rem(tokens.sp_8))
                .child(body)
                .children(row.key.clone().map(|key| tkey(SharedString::from(key), tokens)))
                .when(mixed && row.changed, |cell| cell.child(cdot(tokens).id(("wrow-changed", ix)).test_support().aria_label("changed since the last reconcile"))),
        )
        .children(at_end(row, ix, tokens))
        .on_click(move |event, window, cx| {
            selecting.update(cx, |feature, cx| feature.send(Call::SelectWork { at }, cx));
            // The second press of a double-click opens what is under the cursor to be edited.
            if event.click_count() >= 2 {
                editing.update(cx, |feature, cx| feature.edit_cursor_row(window, cx));
            }
        });
    selected(line, row.on, tokens).into_any_element()
}
