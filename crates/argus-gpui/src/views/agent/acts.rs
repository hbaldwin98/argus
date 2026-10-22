//! What a view's head offers to press: the buttons the agent put in the view, then the `...` that
//! opens its menu (refresh, pop out or put back, close), in the panel on the title row and as a
//! page in the stage's strip (`viewActions`, `views/agent.ts`).
//!
//! Every item is a request: the view stays until the daemon says it closed, and a refresh is
//! pending until the agent shows it again.

use std::collections::HashSet;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, PageView};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::{div, prelude::*, AnyElement, Anchor, App, Context, Entity, MouseButton, Role, SharedString, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::kit::strip::menu_button;
use crate::layout::dock_menu;
use crate::theme::Tokens;
use crate::views::actions::{action_button, PressWith};
use crate::views::blocks::pen::Pen;
use crate::views::ctx::ViewCtx;

/// Why Refresh cannot be chosen again.
pub const ASKED: &str = "Already asked";

/// What `view`'s blocks and buttons are drawn with: whose view it is, what is pending in it, what
/// changed, and whether the daemon takes a press.
pub fn context(view: &PageView, can_press: bool) -> ViewCtx {
    ViewCtx {
        diagrams: Rc::new(view.diagrams.clone()),
        pressed: Rc::new(view.pressed.clone()),
        can_press,
        changed: Rc::new(view.changed.iter().cloned().collect::<HashSet<_>>()),
        ..ViewCtx::agent(view.pane, &view.view.id)
    }
}

/// The `...` menu: the view's name, Refresh, the other place it can be (a page when it is in its
/// dock, its dock when it is a page), and Close in the danger colour.
pub fn menu(view: &PageView, popped: bool) -> Vec<MenuItem<Call>> {
    let (pane, id) = (view.pane, view.view.id.clone());
    let place = if popped {
        MenuItem::choice("Put back in its dock", Call::OpenView { pane, id: id.clone() })
    } else {
        MenuItem::choice("Pop out as a page", Call::PopOutView { pane, id: id.clone() })
    };
    vec![
        MenuItem::heading(view.view.title.clone()),
        MenuItem::choice("Refresh", Call::RefreshView { pane, id: id.clone() }).refused_when(view.refreshing, ASKED),
        place,
        MenuItem::Rule,
        MenuItem::choice("Close this view for every window", Call::CloseView { pane, id }).danger(),
    ]
}

/// `More: refresh, pop out, close`, the menu opening under it, right-aligned with the button.
pub fn more(view: &PageView, popped: bool, caller: &Caller, heard: &Entity<Heard>, tokens: &Tokens, cx: &App) -> impl IntoElement {
    let (items, caller, heard, tokens) = (menu(view, popped), caller.clone(), heard.clone(), *tokens);
    let words = SharedString::from(format!("More about {}", view.view.title));
    // The kit's button has no accessible label of its own, so the wrapper carries it.
    div()
        .id("view-more")
        .role(Role::Button)
        .test_support()
        .aria_label(words)
        .flex_shrink_0()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            menu_button("view-more-button", Glyph::More, tokens.icon_sm, &tokens, cx)
                .tooltip("More: refresh, pop out, close")
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu: PopupMenu, window: &mut Window, cx: &mut Context<PopupMenu>| {
                    dock_menu::build(menu, &items, &caller, &heard, tokens, window, cx)
                }),
        )
}

/// The agent's buttons and then the `...`, drawn with `pen`.
pub fn acts(view: &PageView, popped: bool, can_press: bool, pen: &Pen, heard: &Entity<Heard>) -> Vec<AnyElement> {
    let ctx = context(view, can_press);
    let mut all: Vec<AnyElement> = view
        .view
        .actions
        .iter()
        .map(|action| action_button(action, &ctx, Rc::new(PressWith::default), false, pen))
        .collect();
    all.push(more(view, popped, pen.draw.caller, heard, pen.draw.tokens, &*pen.cx).into_any_element());
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::menu::item::lines;
    use argus_protocol::AgentView;

    fn view(refreshing: bool) -> PageView {
        PageView {
            pane: 3,
            from: String::new(),
            at: 0,
            refreshing,
            pressed: vec!["measure".into()],
            subject: None,
            view: AgentView { id: "trim".into(), title: "Trim context".into(), ..Default::default() },
            diagrams: Vec::new(),
            changed: vec!["2".into()],
            unseen: false,
        }
    }

    #[test]
    fn the_menu_offers_refresh_the_other_place_and_a_close_in_danger() {
        assert_eq!(lines(&menu(&view(false), false)), ["# Trim context", "Refresh", "Pop out as a page", "---", "!Close this view for every window"]);
        assert_eq!(lines(&menu(&view(false), true))[2], "Put back in its dock");
    }

    #[test]
    fn a_refresh_already_asked_cannot_be_asked_again_and_says_why() {
        let asked = menu(&view(true), false);
        let refresh = asked[1].as_choice().unwrap();
        assert_eq!(refresh.reason.as_deref(), Some(ASKED));
        assert!(menu(&view(false), false)[1].as_choice().unwrap().reason.is_none());
    }

    #[test]
    fn each_choice_asks_for_this_view_by_its_pane_and_id() {
        let items = menu(&view(false), false);
        let calls: Vec<String> = items.iter().filter_map(|item| item.as_choice()).map(|choice| format!("{:?}", choice.does)).collect();
        assert_eq!(calls.len(), 3);
        assert!(calls[0].starts_with("RefreshView") && calls[1].starts_with("PopOutView") && calls[2].starts_with("CloseView"));
        assert!(calls.iter().all(|call| call.contains("pane: 3") && call.contains("\"trim\"")));
        assert!(matches!(menu(&view(false), true)[2].as_choice().unwrap().does, Call::OpenView { pane: 3, .. }), "put back opens it in its dock");
    }

    #[test]
    fn the_context_carries_what_is_pending_and_what_changed() {
        let ctx = context(&view(false), true);
        assert!(ctx.pressed.contains(&"measure".to_owned()) && ctx.can_press && ctx.changed.contains("2"));
        assert_eq!((ctx.pane, ctx.id.as_str()), (3, "trim"));
    }
}
