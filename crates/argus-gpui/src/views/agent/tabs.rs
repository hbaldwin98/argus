//! The views panel's tab strip: a tab per view, newest first, the chosen one lit and kept in view,
//! then the dock button and Hide (`tabs`, `views/agent.ts`).
//!
//! A tab says a refresh is asked with the spinner and a change not yet seen with a dot. A press on a
//! tab, its close or a middle click is a request; the view is chosen or gone only when the daemon
//! says so. The strip's empty room is the panel's head: a press there is the panel being carried,
//! which the dock turns into its drag.

use argus_desktop_core::connection::page::{Call, ViewsPage};
use argus_desktop_core::layout::docks::{Dock, DocksPage, Panel};
use argus_desktop_core::views::head;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{h_flex, Disableable as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, App, ClickEvent, Context, Entity, MouseButton, Role, ScrollHandle, SharedString, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::icons::Glyph;
use crate::kit::spinner::spinner;
use crate::kit::strip;
use crate::kit::tip::tip;
use crate::layout::dock_menu;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The dock button's glyph: the dock the panel sits in, else the chevron.
fn glyph(docks: &DocksPage) -> Glyph {
    match dock_menu::dock_of(docks, &Panel::Views) {
        Some(Dock::Left) => Glyph::DockLeft,
        Some(Dock::Right) => Glyph::DockRight,
        Some(Dock::Bottom) => Glyph::DockBottom,
        None => Glyph::Down,
    }
}

/// `.vnew`: a change in the view the person has not opened since.
fn unseen(tokens: &Tokens) -> AnyElement {
    div()
        .id("view-unseen")
        .role(Role::Group)
        .test_support()
        .aria_label("changed")
        .flex_shrink_0()
        .size(rem(tokens.sp_6))
        .rounded(rem(tokens.r_pill))
        .bg(tokens.changed)
        .into_any_element()
}

/// The tab of the view at `at` in `page`, lit when it is the one the panel shows.
fn tab(page: &ViewsPage, at: usize, on: bool, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let view = &page.views[at];
    let key = head::key(view);
    let (pane, id) = (view.pane, view.view.id.clone());
    let group = SharedString::from(format!("view-tab-{key}"));
    let (open, close, middle) = (Call::OpenView { pane, id: id.clone() }, Call::CloseView { pane, id: id.clone() }, Call::CloseView { pane, id });
    let (opening, closing, clicking) = (caller.clone(), caller.clone(), caller.clone());
    let words = format!("Close {}", view.view.title);
    let (spin, dot) = (view.refreshing.then(|| spinner(&format!("view-tab-{key}"), tokens)), view.unseen.then(|| unseen(tokens)));
    strip::tab(group.clone(), on, true, view.view.title.clone(), tokens)
        .group(group.clone())
        // The close is not there for a tab at rest, so its title has the room (`.x` is `display: none`).
        .when(!on, |tab| tab.pr(rem(tokens.sp_12)).group_hover(group.clone(), |style| style.pr(rem(tokens.sp_4))))
        // Tabs shrink before the strip scrolls, so the first is never a sliver.
        .flex_shrink(1.0)
        .min_w(rem(tokens.sp_32 * 3.0))
        .max_w(rem(tokens.vtab_max))
        .overflow_hidden()
        .tooltip(tip(head::tab_tip(view)))
        // The tab's own press: the strip's head would begin a drag of the panel.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, _| opening.call(open.clone()))
        // As the page's `closesOnMiddle`: the press is the tab's alone, the click closes.
        .on_mouse_down(MouseButton::Middle, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_aux_click(move |event: &ClickEvent, _, cx| {
            if event.is_middle_click() {
                cx.stop_propagation();
                clicking.call(middle.clone());
            }
        })
        .child(
            h_flex()
                .min_w_0()
                .flex_1()
                .items_center()
                .gap(rem(tokens.sp_6))
                .children(spin)
                .children(dot)
                .child(strip::title(view.view.title.clone())),
        )
        .child(
            strip::close_x(SharedString::from(format!("view-tab-close-{key}")), words, tokens)
                .when(!on, |close| close.hidden().group_hover(group.clone(), |style| style.flex()))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    closing.call(close.clone());
                }),
        )
}

/// What the strip is drawn from: `chosen` is the index of the view the panel shows, `scroll` the
/// handle of the tabs' own scrolling box, which brings that one into view.
pub struct Strip<'a> {
    pub page: &'a ViewsPage,
    pub chosen: usize,
    pub scroll: &'a ScrollHandle,
    pub docks: &'a DocksPage,
    pub held: bool,
    pub caller: &'a Caller,
    pub heard: &'a Entity<Heard>,
}

/// The strip over a view.
pub fn strip(from: Strip, tokens: &Tokens, cx: &App) -> impl IntoElement {
    let Strip { page, chosen, scroll, docks, held, caller, heard } = from;
    let tabs: Vec<AnyElement> = (0..page.views.len()).map(|at| tab(page, at, at == chosen, caller, tokens).into_any_element()).collect();
    let present = docks.present.iter().find(|present| present.panel == Panel::Views).cloned();
    let entries = dock_menu::moves(&Panel::Views, true, docks, present.as_ref());
    let (menu_caller, menu_heard, menu_tokens) = (caller.clone(), heard.clone(), *tokens);
    let hiding = caller.clone();
    strip::bar(tokens)
        .id("views-tabs")
        .role(Role::TabList)
        .test_support()
        .aria_label("Views")
        .child(
            h_flex()
                .id("views-tabs-scroll")
                .flex_1()
                .min_w_0()
                .gap(rem(tokens.sp_4))
                .overflow_x_scroll()
                .track_scroll(scroll)
                .children(tabs),
        )
        .child(
            div()
                .flex_shrink_0()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    strip::menu_button("views-dock-menu", glyph(docks), 15.0, tokens, cx)
                        .tooltip("Move to another dock")
                        .disabled(held)
                        .dropdown_menu(move |menu, window: &mut Window, cx: &mut Context<_>| {
                            dock_menu::build(menu, &entries, &menu_caller, &menu_heard, menu_tokens, window, cx)
                        }),
                ),
        )
        .child(
            div().flex_shrink_0().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
                strip::menu_button("views-hide", Glyph::Close, 12.0, tokens, cx)
                    .tooltip("Hide")
                    .disabled(held)
                    .on_click(move |_: &ClickEvent, _, _| hiding.call(Call::HideViews)),
            ),
        )
}
