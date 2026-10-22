//! The question the Checkouts stage asks before something cannot be taken back: removing a
//! worktree, or deleting a branch git refused to delete because it holds commits nothing else has
//! (`ck/stage.ts`, `confirmDialog`).
//!
//! The session holds the question (`Checkouts::confirm`), and the dialog is up while it does:
//! drawn once per question and taken away when the table says there is none. `Keep it` has the
//! keys, and it, the close button, Escape and a press on the scrim all answer no; only the red
//! button answers yes. A question is answered once: the dialog closes on the answer, and a table
//! heard again before the session has cleared it does not put it back up.
//!
//! As the page does, a question that replaces another without the table passing through none is
//! not drawn again, so the old title and body stay (CHECKOUTS-50). The session never does that
//! today.

use std::rc::Rc;

use argus_desktop_core::checkouts::stage::Confirm;
use argus_desktop_core::checkouts::table::LastCommit;
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, FocusHandle, Global, SharedString, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::kit::alert;
use crate::kit::dialog::head;
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// What the window knows of the question beyond what the table says.
#[derive(Default)]
struct Asked {
    /// The question up is this stage's, so a table with none does not close another's.
    up: bool,
    /// It has been answered, and the session has not yet said so: the table heard in between must
    /// not put it up again.
    answered: bool,
}

impl Global for Asked {}

fn asked(cx: &App) -> bool {
    cx.try_global::<Asked>().is_some_and(|asked| asked.answered)
}

/// Answers the question, once.
fn answer(yes: bool, caller: &Caller, window: &mut Window, cx: &mut App) {
    if asked(cx) {
        return;
    }
    cx.default_global::<Asked>().answered = true;
    caller.call(Call::AnswerCheckouts { yes });
    Overlays::get(cx).close(Overlay::Removal, window, cx);
    cx.default_global::<Asked>().up = false;
}

/// Puts the question up or takes it away to match what the table holds.
pub fn follow(confirm: Option<&Confirm>, caller: &Caller, window: &mut Window, cx: &mut App) {
    let host = Overlays::get(cx);
    let Some(confirm) = confirm else {
        let was = std::mem::take(cx.default_global::<Asked>());
        if was.up && host.is_open(Overlay::Removal) {
            host.close(Overlay::Removal, window, cx);
        }
        return;
    };
    if host.is_open(Overlay::Removal) || asked(cx) {
        return;
    }
    cx.default_global::<Asked>().up = true;
    let confirm = confirm.clone();
    let keep: FocusHandle = cx.focus_handle();
    let no = caller.clone();
    let (asking, answering) = (caller.clone(), caller.clone());
    host.open(
        Spec::new(Overlay::Removal).dismissed(Dismiss::Answers(Rc::new(move |window, cx| answer(false, &no, window, cx)))),
        move |dialog: Dialog, window, cx, first| {
            if first {
                window.focus(&keep, cx);
            }
            let tokens = *Tokens::global(cx);
            let (no, yes, close, ok) = (asking.clone(), answering.clone(), asking.clone(), asking.clone());
            dialog
                .w(pixels(tokens.dialog_w_sm, window.rem_size()))
                .p_0()
                .margin_top((window.viewport_size().height - pixels(height(&confirm, &tokens), window.rem_size())) / 2.0)
                // Enter on `Keep it` is a press on it.
                .on_ok(move |_, window, cx| {
                    answer(false, &ok, window, cx);
                    true
                })
                .child(body(
                    &confirm,
                    &keep,
                    move |window, cx| answer(false, &no, window, cx),
                    move |window, cx| answer(true, &yes, window, cx),
                    move |window, cx| answer(false, &close, window, cx),
                    &tokens,
                ))
        },
        window,
        cx,
    );
}

/// How tall the dialog is in the page's pixels, estimated from what it holds: the page centres its
/// confirm dialogs and the kit's dialog sits near the top unless told how far down to go.
fn height(confirm: &Confirm, tokens: &Tokens) -> f32 {
    let line = tokens.type_body.size * 1.4;
    let head = tokens.sp_24 + tokens.sp_32.max(line) + tokens.sp_16;
    let foot = 2.0 * tokens.sp_16 + tokens.control_h + tokens.hairline;
    let inside = match confirm {
        Confirm::Remove { .. } => line,
        Confirm::Force { commits, .. } => {
            let callout = 2.0 * tokens.sp_8 + line;
            let small = tokens.fs_sm * 1.4;
            let list = (commits.len() as f32 * small + 2.0 * tokens.sp_8 + 2.0 * tokens.hairline).min(tokens.ck_commits_max_h);
            callout + if commits.is_empty() { 0.0 } else { tokens.sp_12 + list }
        }
    };
    head + inside + tokens.sp_24 + foot
}

/// The title: the question with the name or the branch in the mono face, and its words.
fn title(confirm: &Confirm, tokens: &Tokens) -> (SharedString, impl IntoElement) {
    let mono = |name: &str| div().font_family(SharedString::from(tokens.font_mono)).child(SharedString::from(name.to_owned()));
    match confirm {
        Confirm::Remove { name, .. } => (SharedString::from(format!("Remove {name}?")), h_flex().child("Remove\u{a0}").child(mono(name)).child("?")),
        Confirm::Force { branch, .. } => (SharedString::from(format!("Delete {branch} anyway?")), h_flex().child("Delete\u{a0}").child(mono(branch)).child("\u{a0}anyway?")),
    }
}

/// One unmerged commit: its hash, its summary and how long ago.
fn commit(index: usize, commit: &LastCommit, tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .id(("force-commit", index))
        .test_support()
        .aria_label(SharedString::from(format!("{} {} \u{b7} {}", commit.short, commit.summary, commit.ago)))
        .gap(rem(tokens.sp_4))
        .whitespace_nowrap()
        .child(div().font_family(SharedString::from(tokens.font_mono)).text_color(tokens.fg_faint).child(SharedString::from(commit.short.clone())))
        .child(div().text_color(tokens.fg_muted).child(SharedString::from(commit.summary.clone())))
        .child(div().text_color(tokens.fg_faint).child(SharedString::from(format!(" \u{b7} {}", commit.ago))))
}

/// The alert: the bin tile and the question, what it gives up, and the two answers.
fn body(
    confirm: &Confirm,
    keep: &FocusHandle,
    keep_it: impl Fn(&mut Window, &mut App) + 'static,
    yes: impl Fn(&mut Window, &mut App) + 'static,
    close: impl Fn(&mut Window, &mut App) + 'static,
    tokens: &Tokens,
) -> impl IntoElement {
    let (words, shown) = title(confirm, tokens);
    let heading = h_flex().gap(rem(tokens.sp_12)).items_center().child(alert::tile(tokens)).child(div().id("confirm-title").test_support().aria_label(words).min_w_0().child(shown));
    let inside = match confirm {
        Confirm::Remove { .. } => {
            let words = "Its panes end; its directory and branch are deleted.";
            div().id("confirm-body").test_support().aria_label(words).child(words).into_any_element()
        }
        Confirm::Force { commits, .. } => v_flex()
            .gap(rem(tokens.sp_12))
            .child(alert::callout("confirm-warning", "It has commits no other branch has, and nothing is pushed.", tokens))
            .when(!commits.is_empty(), |inside| {
                inside.child(
                    v_flex()
                        .id("confirm-commits")
                        .max_h(rem(tokens.ck_commits_max_h))
                        .overflow_y_scroll()
                        .rounded(rem(tokens.r_sm))
                        .border_1()
                        .border_color(tokens.line_strong)
                        .bg(tokens.bg_surface)
                        .px(rem(tokens.sp_12))
                        .py(rem(tokens.sp_8))
                        .text_size(rem(tokens.fs_sm))
                        .children(commits.iter().enumerate().map(|(i, c)| commit(i, c, tokens))),
                )
            })
            .into_any_element(),
    };
    let (keep_words, yes_words) = match confirm {
        Confirm::Remove { .. } => ("Keep it", "Remove"),
        Confirm::Force { .. } => ("Keep the branch", "Delete anyway"),
    };
    v_flex()
        .id("checkouts-confirm")
        .child(head("confirm", heading, close, None, tokens))
        .child(v_flex().gap(rem(tokens.sp_12)).px(rem(tokens.sp_24)).pb(rem(tokens.sp_24)).typed(tokens.type_body).text_color(tokens.fg_muted).child(inside))
        .child(alert::answers("confirm", keep_words, yes_words, keep, keep_it, yes, tokens))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::themes::ledger;

    #[test]
    fn a_question_that_lists_commits_is_taller_than_a_plain_one() {
        let tokens = ledger::tokens();
        let plain = height(&Confirm::Remove { name: "a".into(), branch: "a".into() }, &tokens);
        let commits = |n| Confirm::Force { branch: "a".into(), commits: (0..n).map(|_| LastCommit { short: "x".into(), summary: "y".into(), ago: "1h".into() }).collect() };
        assert!(height(&commits(1), &tokens) > plain);
        assert!(height(&commits(3), &tokens) > height(&commits(1), &tokens));
        assert_eq!(height(&commits(500), &tokens), height(&commits(100), &tokens), "the list scrolls at its largest");
    }
}
