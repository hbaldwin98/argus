//! What the session under the cursor offers, drawn the way the sessions footer and the reader's head
//! both draw it (SESSIONS-13, TRANSCRIPT-11): go to its pane, resume it by the command named, or
//! say why it cannot be and offer only its transcript.
//!
//! Every button sends `SessionsAction` and changes nothing on screen: the dialog follows the
//! session's next page.

use argus_desktop_core::agents::sessions_dialog::SessionPreview;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::{div, prelude::*, AnyElement, App, Entity, SharedString, TestSupportExt as _};

use super::ask::ask;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::button::{button, Look};
use crate::kit::keycap::keycap;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The cap on a primary button: the on-accent colour, its edge at a share of it.
pub fn primary_cap(key: &'static str, tokens: &Tokens) -> gpui_kit::Div {
    keycap(key, SharedString::from(tokens.font_mono), tokens).text_color(tokens.fg_on_accent).border_color(tokens.fg_on_accent.opacity(tokens.share_edge))
}

/// The call a button of the offer sends.
fn act(action: Action) -> Call {
    Call::SessionsAction { action, row: None }
}

fn pressed(heard: &Entity<Heard>, caller: &Caller, action: Action) -> impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut App) + 'static {
    let (heard, caller) = (heard.clone(), caller.clone());
    move |_, _, cx| ask(&heard, &caller, act(action), cx)
}

/// The offer's buttons and words, in order. `full` is the sessions footer's, which names the command
/// a resume runs and offers the transcript beside the primary; the reader's head offers the primary
/// alone (its own button is the transcript already).
pub fn offer(preview: &SessionPreview, full: bool, heard: &Entity<Heard>, caller: &Caller, tokens: &Tokens) -> Vec<AnyElement> {
    let read = |tokens: &Tokens| {
        (full && preview.readable).then(|| {
            button("sessions-transcript", "Open full transcript", Look::Plain, true, tokens)
                .child("Open full transcript")
                .child(keycap("O", SharedString::from(tokens.font_mono), tokens))
                .on_click(pressed(heard, caller, Action::Transcript))
                .into_any_element()
        })
    };
    let meta = |words: &'static str, tokens: &Tokens| div().flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child(words);
    match preview.offer {
        "go" => read(tokens)
            .into_iter()
            .chain([button("sessions-go", "Go to its pane", Look::Primary, true, tokens)
                .child("Go to its pane")
                .child(primary_cap("Enter", tokens))
                .on_click(pressed(heard, caller, Action::Choose))
                .into_any_element()])
            .collect(),
        "resume" => {
            let command = SharedString::from(preview.command.clone().unwrap_or_default());
            let mut parts = Vec::new();
            if full {
                parts.push(meta("runs", tokens).into_any_element());
                parts.push(
                    div()
                        .id("sessions-command")
                        .test_support()
                        .aria_label(command.clone())
                        .min_w_0()
                        .max_w(rem(tokens.scommand_max_w))
                        .truncate()
                        .px(rem(tokens.sp_6))
                        .py(rem(tokens.sp_2))
                        .rounded(rem(tokens.r_sm))
                        .bg(tokens.bg_sunken)
                        .text_color(tokens.fg)
                        .font_family(SharedString::from(tokens.font_mono))
                        .tooltip(tip(command.clone()))
                        .child(command.clone())
                        .into_any_element(),
                );
            }
            parts.extend(read(tokens));
            parts.push(
                button("sessions-resume", "Resume in a new pane", Look::Primary, true, tokens)
                    .child("Resume in a new pane")
                    .child(primary_cap("R", tokens))
                    .tooltip(tip(command))
                    .on_click(pressed(heard, caller, Action::Resume))
                    .into_any_element(),
            );
            parts
        }
        _ => {
            let why = preview.why.as_ref().map(|why| {
                let words = SharedString::from(format!("Not resumable: {why}"));
                div()
                    .id("sessions-why")
                    .test_support()
                    .aria_label(words.clone())
                    .min_w_0()
                    .truncate()
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .child(words)
                    .into_any_element()
            });
            why.into_iter().chain(read(tokens)).collect()
        }
    }
}
