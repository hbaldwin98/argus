//! The panes to go back to, under the empty stage's buttons: `Panes in <project>`, a card for
//! every pane in the project, the most recently changed first, whatever the rail is folded to
//! (SHELL-STAGE-13).
//!
//! Core sends them sorted (`chrome/rail`); a press is `select{pane}`, which the session answers by
//! putting the pane on screen, so the card changes nothing here.

use argus_desktop_core::connection::page::{Call, RunningPane};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, SharedString};

use crate::connection::caller::Caller;
use crate::kit::pill::{status_pill, status_word};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a card says under the pane's title: its note, else what kind of pane it is, then how long
/// since its status last changed.
pub fn note(pane: &RunningPane) -> String {
    let said = pane.note.clone().unwrap_or_else(|| if pane.agent { "agent" } else { "shell" }.to_owned());
    match &pane.used {
        Some(used) => format!("{said} \u{b7} {used} ago"),
        None => said,
    }
}

fn card(pane: &RunningPane, held: bool, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let (id, caller) = (pane.id, caller.clone());
    let (raised, text) = (tokens.bg_raised, tokens.fg);
    let (hover, press) = (raised.blend(text.opacity(tokens.share_card_hover)), raised.blend(text.opacity(tokens.share_card_press)));
    let title = SharedString::from(pane.title.clone());
    v_flex()
        .id(("running", pane.id))
        .role(gpui_kit::Role::Button)
        .test_support()
        .aria_label(title.clone())
        .flex_1()
        .min_w(rem(tokens.card_min))
        .gap(rem(tokens.sp_6))
        .p(rem(tokens.sp_12))
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_md))
        .bg(raised)
        .text_color(tokens.fg)
        .text_left()
        .cursor_pointer()
        .hover(move |style| style.border_color(tokens.line_strong).bg(hover))
        .active(move |style| style.bg(press))
        .on_click(move |_, _, _| {
            if !held {
                caller.call(Call::Select { pane: id });
            }
        })
        .child(
            h_flex()
                .gap(rem(tokens.sp_8))
                .justify_between()
                .items_center()
                .min_w_0()
                .text_size(rem(tokens.fs_xs))
                .text_color(tokens.fg_muted)
                .child(
                    h_flex()
                        .min_w_0()
                        .child(div().truncate().child(SharedString::from(format!("{} \u{b7} ", pane.repository))))
                        .child(div().truncate().font_family(SharedString::from(tokens.font_mono)).child(SharedString::from(pane.checkout.clone()))),
                )
                .child(status_pill(("running-status", pane.id), status_word(pane.status).to_owned(), pane.status, tokens)),
        )
        .child(div().truncate().weighted(tokens.weight_semibold).child(title))
        .child(div().truncate().text_size(rem(tokens.fs_xs)).text_color(tokens.fg_muted).child(SharedString::from(note(pane))))
}

/// The strip of cards for `panes`, or nothing with none to go back to.
pub fn running(project: &str, panes: &[RunningPane], held: bool, caller: &Caller, tokens: &Tokens) -> Option<impl IntoElement> {
    if panes.is_empty() {
        return None;
    }
    let head = format!("Panes in {project}");
    Some(
        v_flex()
            .id("running")
            .role(gpui_kit::Role::Group)
            .test_support()
            .aria_label(SharedString::from(head.clone()))
            .self_stretch()
            .mt(rem(tokens.sp_32))
            .text_left()
            .child(
                h_flex()
                    .items_center()
                    .gap(rem(tokens.sp_12))
                    .mb(rem(tokens.sp_12))
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .child(SharedString::from(head))
                    .child(div().flex_1().h(rem(tokens.hairline)).bg(tokens.line)),
            )
            .child(h_flex().flex_wrap().gap(rem(tokens.sp_12)).children(panes.iter().map(|pane| card(pane, held, caller, tokens)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(note: Option<&str>, agent: bool, used: Option<&str>) -> RunningPane {
        RunningPane {
            id: 1,
            agent,
            title: "t".into(),
            status: "idle",
            note: note.map(str::to_owned),
            repository: "argus".into(),
            checkout: "main".into(),
            used: used.map(str::to_owned),
        }
    }

    #[test]
    fn a_card_says_the_panes_note_else_its_kind_and_how_long_since_it_changed() {
        assert_eq!(note(&pane(Some("tests failing"), true, Some("3m"))), "tests failing \u{b7} 3m ago");
        assert_eq!(note(&pane(None, true, Some("2d"))), "agent \u{b7} 2d ago");
        assert_eq!(note(&pane(None, false, None)), "shell", "a daemon that gives no time says none");
    }
}
