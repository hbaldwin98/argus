//! The Git stage's buttons in the title bar: Refresh while Changes is the tab, the way back beside
//! the work, and Close. The tabs and the files' count are `tabs`; the branch is the crumb's. The
//! stage has no band of its own: the file head sits directly under the title bar, as Code's does.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{DockGesture, Panel};
use gpui_kit::component::h_flex;
use gpui_kit::{prelude::*, AnyElement, App, Entity, WeakEntity, Window};

use super::GitStage;
use crate::connection::heard::Heard;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::review::entry::Tab;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The buttons for what is open, built each time the bar is drawn.
pub(super) fn tools(stage: WeakEntity<GitStage>, heard: &Entity<Heard>, _: &mut Window, cx: &mut App) -> AnyElement {
    let tokens = *Tokens::global(cx);
    let (tab, review, history, beside) = {
        let heard = heard.read(cx);
        (heard.git.tab(heard.open()), heard.review.is_some(), heard.history.is_some(), heard.changes_beside())
    };
    // Only what is open has a panel to go back as; beside the work already, the dock's head holds
    // the way to the stage.
    let back = (review || history) && !beside;
    let changes_on = tab == Tab::Changes;
    let close_label = if changes_on { "Close the review" } else { "Close the history" };
    let ask = |stage: WeakEntity<GitStage>, call: Call| {
        move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
            if let Some(stage) = stage.upgrade() {
                stage.update(cx, |this, cx| {
                    // A button takes focus when clicked, and the stage's keys are heard only on the stage itself.
                    window.focus(&this.focus, cx);
                    this.ask(call.clone(), cx)
                });
            }
        }
    };
    let close = stage.clone();
    h_flex()
        .id("git-tools")
        .flex_none()
        .gap(rem(tokens.sp_4))
        .items_center()
        .when(changes_on && review, |tools| {
            tools.child(icon_button("review-refresh", "Refresh", false, &tokens).tooltip(tip("Refresh")).child(Glyph::Refresh.at(14.0)).on_click(ask(stage.clone(), Call::ReviewRefresh)))
        })
        // Git on the stage goes back beside the work, to the dock or dialog it came from.
        .when(back, |tools| {
            tools.child(
                icon_button("git-collapse", "Back to the side pane", false, &tokens)
                    .tooltip(tip("Back to the side pane"))
                    .child(Glyph::DockRight.at(14.0))
                    .on_click(ask(stage.clone(), Call::Docks { gesture: DockGesture::Collapse { panel: Panel::Changes } })),
            )
        })
        .child(
            icon_button("git-close", close_label, false, &tokens)
                .tooltip(tip(close_label))
                .child(Glyph::Close.at(14.0))
                .on_click(move |_, window, cx| {
                    if let Some(stage) = close.upgrade() {
                        stage.update(cx, |this, cx| {
                            // The button is the title bar's, outside the stage, so a press does not give the stage the keys itself.
                            window.focus(&this.focus, cx);
                            this.close(cx)
                        });
                    }
                }),
        )
        .into_any_element()
}
