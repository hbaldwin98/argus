//! A cell's header: the pane's status dot, its name in the body colour and its task muted (core's
//! halves of the title), and the button that takes it out of the grid. A panel's cell has the same
//! header with the panel's title as its name and no status: it is carried and closed as a pane's is,
//! and its close sends the panel back to its dock.
//!
//! The header is the handle a pane is carried by (SHELL-GRID-11): a press on it is followed by the
//! work area's drag, which lifts it after the shared threshold, lights the zone it would land on,
//! and lets Escape put it back. The close does not start one. The head has no fill of its own, only a
//! hairline under it; which cell has the keys is said by the ring round the cell (`keyed`), not by
//! the head. The Workspace's own cell keeps the close button's room empty, so the heads' ends line up.

use argus_desktop_core::connection::page::{Call, Side};
use argus_desktop_core::layout::docks::{DocksPage, Panel};
use argus_desktop_core::panes::shown::workspace_grid::Occupant;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, MouseButton, MouseDownEvent, SharedString};

use super::SplitView;
use crate::kit::status_dot::status_dot;
use crate::kit::strip;
use crate::kit::tip::tip;
use crate::layout::carry::{Carried, Carrier};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The words on a header's close.
pub const TAKE_OUT: &str = "Take out of the grid";

/// What a header names: what its cell holds, by the title it is shown by (whole, for the close's
/// words and the carried pane), its two halves as the head sets them, and a pane's status.
pub(super) struct Named {
    pub held: Occupant,
    pub title: String,
    pub name: String,
    pub task: Option<String>,
    pub status: Option<&'static str>,
}

/// A panel's title as its dock's tab or its dialog says it; one never docked is named by its kind.
pub(super) fn panel_title(docks: &DocksPage, panel: &Panel) -> String {
    let tabs = [&docks.left, &docks.right, &docks.bottom].into_iter().flat_map(|dock| dock.tabs.iter().map(|tab| (&tab.panel, &tab.title)));
    let floating = docks.floating.iter().map(|float| (&float.panel, &float.title));
    match tabs.chain(floating).find(|(held, _)| *held == panel) {
        Some((_, title)) => title.clone(),
        None => match panel {
            Panel::Console => "Console".into(),
            Panel::Views => "Views".into(),
            Panel::Browsers => "Browsers".into(),
            Panel::Changes => "Changes".into(),
            Panel::Extension(_) | Panel::Saved(_) => panel.to_string(),
        },
    }
}

impl SplitView {
    pub(super) fn head(&self, n: u8, named: Named, keyed: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *tokens;
        let title = SharedString::from(named.title.clone());
        let (name, task) = (named.name.clone(), named.task.clone());
        let carried = Carried::Cell { slot: n, held: named.held, title: named.title };
        let held = self.heard.read(cx).held();
        h_flex()
            .id(SharedString::from(format!("cell-head-{n}")))
            .test_support()
            .aria_selected(keyed)
            .relative()
            .flex_shrink_0()
            .h(rem(tokens.pane_head_h))
            .items_center()
            .gap(rem(tokens.sp_6))
            .pl(rem(tokens.sp_12))
            .border_b_1()
            .border_color(tokens.line)
            .text_size(rem(tokens.fs_xs))
            .text_color(tokens.fg_faint)
            .cursor_grab()
            .tooltip(tip("Drag onto another pane to move this one"))
            // A press that is never a drag leaves the header as it was: it has no click of its own.
            .on_mouse_down(MouseButton::Left, {
                let carrier = Carrier::of(cx);
                cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                    if !held {
                        carrier.press(event.position, carried.clone(), cx);
                    }
                })
            })
            .children(named.status.map(|status| status_dot(format!("cell-{n}"), status, &tokens)))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(rem(tokens.sp_6))
                    .child(
                        div()
                            .id(SharedString::from(format!("cell-name-{n}")))
                            .test_support()
                            .aria_label(SharedString::from(name.clone()))
                            .flex_shrink_0()
                            .weighted(tokens.weight_semibold)
                            .text_color(tokens.fg)
                            .child(name),
                    )
                    .children(task.map(|task| {
                        strip::title(format!("\u{b7} {task}")).id(SharedString::from(format!("cell-task-{n}"))).test_support().aria_label(SharedString::from(task))
                    })),
            )
            .child(if n == 0 {
                div().w(rem(tokens.control_h_sm)).flex_shrink_0().into_any_element()
            } else {
                strip::close_x(SharedString::from(format!("cell-close-{n}")), format!("Take {title} out of the grid"), &tokens)
                    .tooltip(tip(TAKE_OUT))
                    // A press on the close must not start a move.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !this.heard.read(cx).held() {
                            this.caller.call(Call::CloseBeside { side: Side::Beside(n - 1) });
                        }
                    }))
                    .into_any_element()
            })
    }
}
