//! The buttons and menus an extension's view offers its actions through: a view-wide action's
//! small button with its key cap, a row's text button (the key in its tooltip, none on the button),
//! the "..." that holds a row's actions when the view is narrow, and the calls they send
//! (EXTVIEW-6, -7, -9).
//!
//! What each action is, and which show where, is core's (`extensions/row_actions`); a click only
//! sends the gesture. An action on a row other than the selected one selects that row first, since
//! the click that took the action stopped before the row's own select.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::extensions::row_actions::tooltip;
use argus_desktop_core::extensions::views::{ExtTable, ExtTableAction, ExtensionGesture};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Anchor, App, Entity, Role, SharedString};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::button::{button, Look};
use crate::kit::strip;
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// A gesture in `panel`'s view.
pub fn gesture(panel: &argus_desktop_core::layout::docks::Panel, gesture: ExtensionGesture) -> Call {
    Call::Extension { panel: panel.clone(), gesture }
}

/// The calls that take `action` on `row`: the row's select first when it is not the selected one.
pub fn taking(view: &ExtTable, action: &str, row: Option<&str>) -> Vec<Call> {
    let mut calls = Vec::new();
    if let Some(row) = row.filter(|row| view.selected.as_deref() != Some(*row)) {
        calls.push(gesture(&view.panel, ExtensionGesture::Select { key: row.to_owned() }));
    }
    calls.push(gesture(&view.panel, ExtensionGesture::Act { action: action.to_owned() }));
    calls
}

fn send(caller: &Caller, calls: Vec<Call>) {
    for call in calls {
        caller.call(call);
    }
}

/// A view-wide action as a small button with its key's cap, in the page's meta row or an ended
/// console's strip.
pub fn wide_button(view: &ExtTable, action: &ExtTableAction, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let (caller, calls) = (caller.clone(), taking(view, &action.id, None));
    button(SharedString::from(format!("ext-act-{}-{}", view.panel, action.id)), action.label.clone(), Look::Plain, true, tokens)
        .child(SharedString::from(action.label.clone()))
        .children(action.key.clone().map(|key| keycap(key, SharedString::from(tokens.font_mono), tokens)))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            send(&caller, calls.clone());
        })
}

/// `.row-act`: a row's action as text, quiet until the pointer is on it. The key is in the tooltip,
/// since the view's menu lists every key once.
pub fn row_button(view: &ExtTable, action: &ExtTableAction, row: &str, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let (caller, calls) = (caller.clone(), taking(view, &action.id, Some(row)));
    let (line, fg) = (tokens.line, tokens.fg);
    let said = tooltip(action);
    div()
        .id(SharedString::from(format!("ext-row-act-{}-{row}-{}", view.panel, action.id)))
        .role(Role::Button)
        .test_support()
        .aria_label(SharedString::from(format!("{} {row}", action.label)))
        .flex_shrink_0()
        .h(rem(tokens.control_h_sm))
        .px(rem(tokens.sp_8))
        .flex()
        .items_center()
        .rounded(rem(tokens.r_sm))
        .typed(tokens.type_label)
        .text_size(rem(tokens.fs_xs))
        .text_color(tokens.fg_muted)
        .cursor_pointer()
        .hover(move |style| style.bg(line).text_color(fg))
        .when_some(said, |button, said| button.tooltip(tip(said)))
        .child(SharedString::from(action.label.clone()))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            send(&caller, calls.clone());
        })
}

/// The "..." that holds a row's actions while the view is narrow: its menu lists them with their
/// keys as hints, and a choice takes the action on that row.
pub fn row_more(view: &ExtTable, actions: &[&ExtTableAction], row: &str, caller: &Caller, heard: &Entity<Heard>, tokens: &Tokens, cx: &App) -> impl IntoElement {
    let items: Vec<MenuItem<Vec<Call>>> = actions
        .iter()
        .map(|action| MenuItem::choice(action.label.clone(), taking(view, &action.id, Some(row))).keyed(action.key.iter().map(|key| vec![key.clone()]).collect()))
        .collect();
    let (caller, held, menu_tokens) = (caller.clone(), heard.clone(), *tokens);
    // The kit's menu button, as the dock's tools are: it opens the menu under itself. Its own
    // press must not reach the row, so the box around it takes it.
    div()
        .id(SharedString::from(format!("ext-row-more-box-{}-{row}", view.panel)))
        .role(Role::Button)
        .test_support()
        .aria_label("Row actions")
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(
            strip::menu_button(SharedString::from(format!("ext-row-more-{}-{row}", view.panel)), Glyph::More, 14.0, tokens, cx)
                .tooltip("Actions")
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| {
                    let (caller, held) = (caller.clone(), held.clone());
                    popup::fill(menu, &items, menu_tokens, move |cx: &App| held.read(cx).held(), move |calls, _, _| send(&caller, calls.clone()), window, cx)
                }),
        )
}
