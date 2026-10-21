//! One dock drawn: its tab strip while it holds more than one panel, the
//! panel it shows under a head of its own while it holds one, and the sash
//! that sizes it.
//!
//! Everything it draws is `Heard`'s last `DocksPage`: which tabs, which one is
//! shown, how big. A press only becomes a `Call` through `WorkView`, which owns
//! the drag (`pointer::drag`): a tab picked is a click on release and a drag
//! once it travels, a sash pressed is a size on release. The size drawn is
//! Rust's except while the sash is held, so a refused or clamped size leaves
//! the dock as Rust says it is.
//!
//! While the daemon is gone the dock is dimmed and answers no press, as the
//! sidebar does (`Heard::held`).

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{Dock, DockGesture, DockPage, DockTab, DocksPage, Panel, PresentPanel};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, Disableable as _, Icon, IconName, Sizable as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    div, prelude::*, px, AnyElement, AnyView, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Role,
    SharedString, Subscription, WeakEntity, Window,
};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::layout::dock_menu;
use crate::layout::panel;
use crate::layout::work::{Held, WorkView};
use crate::theme::Ledger;

/// The height of a tab strip or a single panel's head.
pub const STRIP: f32 = 34.0;
/// What a side dock leaves the stage of the width at least: `--dock-floor` in the page.
pub const FLOOR_WIDTH: f32 = 240.0;
/// What the bottom dock leaves the stage of the height at least: the page's `100% - 120px`.
pub const FLOOR_HEIGHT: f32 = 120.0;
/// How thick the sash's grab zone is, inside the dock's own edge.
const SASH: f32 = 6.0;

pub struct DockView {
    dock: Dock,
    heard: Entity<Heard>,
    caller: Caller,
    work: WeakEntity<WorkView>,
    /// What the console panel draws, which is one view wherever it is docked.
    console: AnyView,
    _observing: Subscription,
}

impl DockView {
    pub fn new(
        dock: Dock,
        heard: Entity<Heard>,
        caller: Caller,
        work: WeakEntity<WorkView>,
        console: AnyView,
        cx: &mut Context<Self>,
    ) -> Self {
        let observing = cx.observe(&heard, |_, _, cx| cx.notify());
        Self { dock, heard, caller, work, console, _observing: observing }
    }

    fn name(&self) -> &'static str {
        name(self.dock)
    }

    fn page<'a>(&self, docks: &'a DocksPage) -> &'a DockPage {
        match self.dock {
            Dock::Left => &docks.left,
            Dock::Right => &docks.right,
            Dock::Bottom => &docks.bottom,
        }
    }

    /// The size the dock draws: Rust's, or the sash's while it is held.
    fn drawn(&self, page: &DockPage, cx: &Context<Self>) -> u32 {
        self.work.upgrade().and_then(|work| work.read(cx).sizing(self.dock)).unwrap_or(page.size)
    }

    fn press(&self, at: gpui_kit::Point<gpui_kit::Pixels>, held: Held, cx: &mut Context<Self>) {
        self.work.update(cx, |work, cx| work.press(at, held, cx)).ok();
    }

    fn ask(&self, call: Call, cx: &mut Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    fn tab(
        &self,
        tab: &DockTab,
        on: bool,
        docks: &DocksPage,
        ledger: &Ledger,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let panel = tab.panel.clone();
        let present = present_of(docks, &panel);
        let closes = present.as_ref().is_some_and(|present| present.closes);
        let stage = present.as_ref().is_some_and(|present| present.stage);
        let entries = dock_menu::tab(&panel, &tab.title, docks, present.as_ref());
        let (caller, heard) = (self.caller.clone(), self.heard.clone());
        let (hover, selected, line, muted, foreground, faint) =
            (ledger.raised, ledger.selected, ledger.line_strong, ledger.muted, ledger.foreground, ledger.faint);
        let (held_panel, held_title) = (panel.clone(), tab.title.clone());
        let (closing, middle) = (panel.clone(), panel.clone());
        let close_label = format!("Close {}", tab.title);
        h_flex()
            .id(SharedString::from(format!("dock-tab-{panel}")))
            .role(Role::Tab)
            .test_support()
            .aria_selected(on)
            .aria_label(tab.title.clone())
            .flex_shrink_0()
            .h(px(26.0))
            .pl_3()
            .pr_1()
            .gap_1()
            .items_center()
            .rounded(px(4.0))
            .border_1()
            .border_color(if on { line } else { gpui_kit::transparent_black() })
            .text_sm()
            .text_color(if on { foreground } else { muted })
            .when(on, |tab| tab.bg(selected))
            .when(!on, |tab| tab.hover(move |style| style.bg(hover)))
            .cursor_pointer()
            // A title truncates at the tab's width; the tooltip has all of it (SHELL-DOCK-3).
            .tooltip({
                let title = tab.title.clone();
                move |window, cx| Tooltip::new(title.clone()).build(window, cx)
            })
            .when(!closes, |tab| tab.pr_3())
            // A tab picked is a click on release and a drag once it travels.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    let held = Held::Panel { panel: held_panel.clone(), title: held_title.clone(), stage, click: true };
                    this.press(event.position, held, cx);
                }),
            )
            .when(closes, |tab| {
                // The press is the tab's alone, as the page's `closesOnMiddle`
                // keeps it from autoscrolling or reaching what is under it.
                tab.on_mouse_down(MouseButton::Middle, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                // A click, as the page's `auxclick`: the press and the release both on the tab.
                .on_aux_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    if event.is_middle_click() {
                        cx.stop_propagation();
                        this.ask(Call::Docks { gesture: DockGesture::Close { panel: middle.clone() } }, cx)
                    }
                }))
            })
            .child(div().max_w(px(180.0)).overflow_hidden().whitespace_nowrap().child(tab.title.clone()))
            .when(closes, |tab| {
                tab.child(
                    div()
                        .id(SharedString::from(format!("dock-tab-close-{closing}")))
                        .role(Role::Button)
                        .test_support()
                        .aria_label(close_label)
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(18.0))
                        .rounded(px(3.0))
                        .text_color(faint)
                        .hover(move |style| style.text_color(foreground))
                        // The tab's own press, which would begin a drag.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask(Call::Docks { gesture: DockGesture::Close { panel: closing.clone() } }, cx)
                        }))
                        .child(Icon::new(IconName::Close).xsmall()),
                )
            })
            .context_menu(move |menu, _, cx| dock_menu::build(menu, &entries, &caller, &heard, cx))
    }

    /// The "Move to another dock" button: the dock the panel sits in, which
    /// opens the menu of docks.
    fn menu_button(&self, panel: &Panel, docks: &DocksPage, held: bool) -> impl IntoElement {
        let present = present_of(docks, panel);
        let entries = dock_menu::moves(panel, true, docks, present.as_ref());
        let (caller, heard) = (self.caller.clone(), self.heard.clone());
        let icon = match self.dock {
            Dock::Left => IconName::PanelLeft,
            Dock::Right => IconName::PanelRight,
            Dock::Bottom => IconName::PanelBottom,
        };
        div()
            .flex_shrink_0()
            // The head's press, which would begin a drag of the panel.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new(SharedString::from(format!("dock-menu-{}", self.name())))
                    .ghost()
                    .xsmall()
                    .icon(icon)
                    .tooltip("Move to another dock")
                    .disabled(held)
                    .dropdown_menu(move |menu, _, cx| dock_menu::build(menu, &entries, &caller, &heard, cx)),
            )
    }

    fn strip(&self, page: &DockPage, shown: &Panel, docks: &DocksPage, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<AnyElement> =
            page.tabs.iter().map(|tab| self.tab(tab, &tab.panel == shown, docks, ledger, cx).into_any_element()).collect();
        h_flex()
            .id(SharedString::from(format!("dock-{}-tabs", self.name())))
            .role(Role::TabList)
            .test_support()
            .flex_shrink_0()
            .h(px(STRIP))
            .px_2()
            .gap_1()
            .items_center()
            .bg(ledger.chrome)
            .border_b_1()
            .border_color(ledger.line)
            .children(tabs)
            .child(div().flex_1())
            .child(self.menu_button(shown, docks, self.heard.read(cx).held()))
    }

    /// A single panel's head: its name, which drags it, and its dock button.
    fn head(&self, shown: &Panel, title: &str, docks: &DocksPage, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let stage = present_of(docks, shown).is_some_and(|present| present.stage);
        let (panel, held_title) = (shown.clone(), title.to_owned());
        h_flex()
            .id(SharedString::from(format!("dock-{}-head", self.name())))
            .test_support()
            .flex_shrink_0()
            .h(px(STRIP))
            .pl_3()
            .pr_2()
            .gap_2()
            .items_center()
            .bg(ledger.chrome)
            .border_b_1()
            .border_color(ledger.line)
            .text_sm()
            .text_color(ledger.muted)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    // A head is dragged, never clicked.
                    let held = Held::Panel { panel: panel.clone(), title: held_title.clone(), stage, click: false };
                    this.press(event.position, held, cx);
                }),
            )
            .child(div().flex_1().overflow_hidden().whitespace_nowrap().child(title.to_owned()))
            .child(self.menu_button(shown, docks, self.heard.read(cx).held()))
    }

    /// The grab zone along the edge the dock is sized by.
    fn sash(&self, held: bool, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let (accent, dock) = (ledger.accent, self.dock);
        let zone = div()
            .id(SharedString::from(format!("dock-{}-sash", self.name())))
            .role(Role::Splitter)
            .test_support()
            .absolute()
            .when(held, |zone| zone.bg(accent))
            .hover(move |style| style.bg(accent.opacity(0.4)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    // The head or tab under the sash's zone must not take the press too.
                    cx.stop_propagation();
                    if this.heard.read(cx).held() {
                        return;
                    }
                    let drawn = {
                        let docks = &this.heard.read(cx).docks;
                        this.drawn(this.page(docks), cx) as f32
                    };
                    this.work.update(cx, |work, cx| work.press_sash(dock, drawn, event.position, cx)).ok();
                }),
            );
        match self.dock {
            Dock::Left => zone.top_0().bottom_0().right_0().w(px(SASH)).cursor_col_resize(),
            Dock::Right => zone.top_0().bottom_0().left_0().w(px(SASH)).cursor_col_resize(),
            Dock::Bottom => zone.left_0().right_0().top_0().h(px(SASH)).cursor_row_resize(),
        }
    }
}

pub fn name(dock: Dock) -> &'static str {
    match dock {
        Dock::Left => "left",
        Dock::Right => "right",
        Dock::Bottom => "bottom",
    }
}

fn present_of(docks: &DocksPage, panel: &Panel) -> Option<PresentPanel> {
    docks.present.iter().find(|present| &present.panel == panel).cloned()
}

impl Render for DockView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let (docks, held) = {
            let heard = self.heard.read(cx);
            (heard.docks.clone(), heard.held())
        };
        let page = self.page(&docks).clone();
        // A dock that shows nothing, or a panel that is on the stage instead,
        // is closed on screen and takes no room.
        let Some(shown) = page.shown.clone().filter(|shown| self.heard.read(cx).on_screen_in_dock(shown)) else {
            return div().into_any_element();
        };
        let title = page.tabs.iter().find(|tab| tab.panel == shown).map_or_else(|| shown.to_string(), |tab| tab.title.clone());
        let size = px(self.drawn(&page, cx) as f32);
        let sizing = self.work.upgrade().is_some_and(|work| work.read(cx).sizing(self.dock).is_some());
        let name = self.name();
        let root = v_flex()
            .id(SharedString::from(format!("dock-{name}")))
            .test_support()
            .relative()
            // It gives way to the stage's floor (`work`), as the page's
            // `max-width: calc(100% - var(--dock-floor))` caps it.
            .flex_shrink(1.0)
            .bg(ledger.background)
            .border_color(ledger.line)
            .when(held, |dock| dock.opacity(0.5));
        let root = match self.dock {
            Dock::Left => root.w(size).min_w_0().h_full().border_r_1(),
            Dock::Right => root.w(size).min_w_0().h_full().border_l_1(),
            Dock::Bottom => root.h(size).min_h_0().w_full().border_t_1(),
        };
        let strip = (page.tabs.len() > 1).then(|| self.strip(&page, &shown, &docks, &ledger, cx).into_any_element());
        let head = (page.tabs.len() <= 1).then(|| self.head(&shown, &title, &docks, &ledger, cx).into_any_element());
        root.children(strip)
            .children(head)
            .child(
                div()
                    .id(SharedString::from(format!("dock-{name}-panel")))
                    .test_support()
                    .aria_label(title.clone())
                    .flex_1()
                    .min_h_0()
                    .child(if shown == Panel::Console {
                        self.console.clone().into_any_element()
                    } else {
                        panel::body(title, &ledger).into_any_element()
                    }),
            )
            .child(self.sash(sizing, &ledger, cx))
            .into_any_element()
    }
}
