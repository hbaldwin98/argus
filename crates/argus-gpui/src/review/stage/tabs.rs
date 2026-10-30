//! What the Git stage puts in the title bar: its Changes and History tabs and, with a review open,
//! how many files it lists and what they add and remove.
//!
//! The stage's own strip lost these when the bar took them (`chrome::header::tabs`); the stage
//! still owns what a press does, so the offer holds a handle on it. A press only asks: which tab
//! shows is the session's to say.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, App, Entity, SharedString, WeakEntity};

use super::GitStage;
use crate::chrome::header::slots::{Offer, Slot, Slots};
use crate::chrome::header::tabs::{Tab, Tabs};
use crate::connection::heard::Heard;
use crate::kit::counts::counts;
use crate::review::entry::Tab as Which;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Puts the Git tabs in the bar's slot for Git.
pub(super) fn offer(stage: WeakEntity<GitStage>, heard: Entity<Heard>, cx: &mut App) {
    Slots::of(cx).offer(
        Slot::Git,
        Offer::default().tabs(move |_, cx| {
            let tokens = *Tokens::global(cx);
            let (on, review) = {
                let heard = heard.read(cx);
                (heard.git.tab(heard.open()), heard.review.clone())
            };
            let press = |which: Which| {
                let stage = stage.clone();
                move |window: &mut gpui_kit::Window, cx: &mut App| {
                    if let Some(stage) = stage.upgrade() {
                        stage.update(cx, |this, cx| {
                            // A tab takes focus when clicked, and the tab's keys are heard only on the stage itself.
                            window.focus(&this.focus, cx);
                            this.show(which, cx)
                        });
                    }
                }
            };
            // What is still to stage; the files line beside the tabs counts the side on screen.
            let waiting = review.as_ref().and_then(|page| page.unstaged);
            let after = review.as_ref().filter(|page| on == Which::Changes && !page.files.is_empty()).map(|page| {
                let plural = if page.files.len() == 1 { "" } else { "s" };
                h_flex()
                    .id("review-count")
                    .test_support()
                    .aria_label(SharedString::from(format!("{} file{plural} \u{b7} +{} \u{2212}{}", page.files.len(), page.added, page.removed)))
                    .gap(rem(tokens.sp_4))
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_faint)
                    .child(SharedString::from(format!("{} file{plural} \u{b7}", page.files.len())))
                    .child(counts(page.added, page.removed, &tokens))
                    .into_any_element()
            });
            Some(
                Tabs::new(
                    "git-tabs",
                    "Git view",
                    vec![
                        Tab::new("git-tab-changes", "Changes", on == Which::Changes, press(Which::Changes)).count(waiting),
                        Tab::new("git-tab-history", "History", on == Which::History, press(Which::History)),
                    ],
                )
                .after(after),
            )
        }),
    );
}
