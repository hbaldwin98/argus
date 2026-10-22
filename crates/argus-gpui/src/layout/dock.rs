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

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{Dock, DockGesture, DockPage, DockTab, DocksPage, Panel, PresentPanel};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{v_flex, Disableable as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    canvas, div, prelude::*, px, AnyElement, AnyView, App, Bounds, ClickEvent, Context, Entity, MouseButton, MouseDownEvent,
    Pixels, Role, SharedString, Subscription, WeakEntity, Window,
};

use crate::chrome::strip;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::icons::Glyph;
use crate::layout::dock_menu;
use crate::layout::panel;
use crate::layout::work::{Held, WorkView};
use crate::style::Typed as _;
use crate::theme::Tokens;

/// What a side dock leaves the stage of the width at least: `--dock-floor` in the page.
pub const FLOOR_WIDTH: f32 = 240.0;
/// What the bottom dock leaves the stage of the height at least: the page's `100% - 120px`.
pub const FLOOR_HEIGHT: f32 = 120.0;

pub struct DockView {
    dock: Dock,
    heard: Entity<Heard>,
    caller: Caller,
    work: WeakEntity<WorkView>,
    /// What the console panel draws, which is one view wherever it is docked.
    console: AnyView,
    /// Where the panel was last laid out: the console's strip is its head, and
    /// a press in the top of it carries the panel as a head does.
    panel: Rc<Cell<Bounds<Pixels>>>,
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
        Self { dock, heard, caller, work, console, panel: Rc::default(), _observing: observing }
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
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let panel = tab.panel.clone();
        let present = present_of(docks, &panel);
        let closes = present.as_ref().is_some_and(|present| present.closes);
        let stage = present.as_ref().is_some_and(|present| present.stage);
        let entries = dock_menu::tab(&panel, &tab.title, docks, present.as_ref());
        let (caller, heard) = (self.caller.clone(), self.heard.clone());
        let (held_panel, held_title) = (panel.clone(), tab.title.clone());
        let (closing, middle) = (panel.clone(), panel.clone());
        let close_label = format!("Close {}", tab.title);
        strip::tab(SharedString::from(format!("dock-tab-{panel}")), on, closes, tab.title.clone(), tokens)
            // A title truncates at the tab's width; the tooltip has all of it (SHELL-DOCK-3).
            .tooltip({
                let title = tab.title.clone();
                move |window, cx| Tooltip::new(title.clone()).build(window, cx)
            })
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
            .child(strip::title(tab.title.clone()))
            .when(closes, |tab| {
                tab.child(
                    strip::close_x(SharedString::from(format!("dock-tab-close-{closing}")), close_label, tokens)
                        // The tab's own press, which would begin a drag.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask(Call::Docks { gesture: DockGesture::Close { panel: closing.clone() } }, cx)
                        })),
                )
            })
            .context_menu(move |menu, _, cx| dock_menu::build(menu, &entries, &caller, &heard, cx))
    }

    /// The "Move to another dock" button: the dock the panel sits in, which
    /// opens the menu of docks.
    fn menu_button(&self, panel: &Panel, docks: &DocksPage, held: bool, tokens: &Tokens, cx: &App) -> impl IntoElement {
        let present = present_of(docks, panel);
        let entries = dock_menu::moves(panel, true, docks, present.as_ref());
        let (caller, heard) = (self.caller.clone(), self.heard.clone());
        let glyph = match self.dock {
            Dock::Left => Glyph::DockLeft,
            Dock::Right => Glyph::DockRight,
            Dock::Bottom => Glyph::DockBottom,
        };
        div()
            .flex_shrink_0()
            // The head's press, which would begin a drag of the panel.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                strip::menu_button(SharedString::from(format!("dock-menu-{}", self.name())), glyph, 15.0, tokens, cx)
                    .tooltip("Move to another dock")
                    .disabled(held)
                    .dropdown_menu(move |menu, _, cx| dock_menu::build(menu, &entries, &caller, &heard, cx)),
            )
    }

    fn strip(&self, page: &DockPage, shown: &Panel, docks: &DocksPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<AnyElement> =
            page.tabs.iter().map(|tab| self.tab(tab, &tab.panel == shown, docks, tokens, cx).into_any_element()).collect();
        let held = self.heard.read(cx).held();
        strip::bar(tokens)
            .id(SharedString::from(format!("dock-{}-tabs", self.name())))
            .role(Role::TabList)
            .test_support()
            .children(tabs)
            .child(div().flex_1())
            .child(self.menu_button(shown, docks, held, tokens, cx))
    }

    /// A single panel's head: its name, which drags it, and its dock button.
    /// The console has none: its own strip is its head, with the dock menu in it.
    fn head(&self, shown: &Panel, title: &str, docks: &DocksPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let stage = present_of(docks, shown).is_some_and(|present| present.stage);
        let (panel, held_title) = (shown.clone(), title.to_owned());
        let held = self.heard.read(cx).held();
        strip::bar(tokens)
            .id(SharedString::from(format!("dock-{}-head", self.name())))
            .test_support()
            .pl(px(tokens.sp_12))
            .typed(tokens.type_small)
            .weighted(tokens.weight_medium)
            .text_color(tokens.fg_muted)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    // A head is dragged, never clicked.
                    let held = Held::Panel { panel: panel.clone(), title: held_title.clone(), stage, click: false };
                    this.press(event.position, held, cx);
                }),
            )
            .child(div().flex_1().truncate().child(title.to_owned()))
            .child(self.menu_button(shown, docks, held, tokens, cx))
    }

    /// The grab zone along the edge the dock is sized by, with the page's
    /// grip (`.dock-sash::after`) in it: a pill in the accent colour, hidden
    /// until the pointer is on the zone or the sash is held.
    ///
    /// The page's zone is 24px wide about the hairline; here it is the 12px
    /// inside the dock's edge, since the stage beside it is drawn over what
    /// reaches past.
    fn sash(&self, held: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (accent, dock) = (tokens.accent, self.dock);
        let name = SharedString::from(format!("dock-{}-sash", self.name()));
        let (across, along) = (px(tokens.sp_4), px(tokens.sp_32));
        let grip = div()
            .absolute()
            .rounded(px(tokens.r_pill))
            .bg(accent)
            .opacity(if held { 1.0 } else { 0.0 })
            .group_hover(name.clone(), |style| style.opacity(1.0));
        let grip = match dock {
            Dock::Left => grip.right_0().top_1_2().mt(-along / 2.0).w(across).h(along),
            Dock::Right => grip.left_0().top_1_2().mt(-along / 2.0).w(across).h(along),
            Dock::Bottom => grip.top_0().left_1_2().ml(-along / 2.0).w(along).h(across),
        };
        let zone = div()
            .id(name.clone())
            .group(name)
            .role(Role::Splitter)
            .test_support()
            .absolute()
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
            )
            .child(grip);
        let reach = px(tokens.sp_12);
        match self.dock {
            Dock::Left => zone.top_0().bottom_0().right_0().w(reach).cursor_col_resize(),
            Dock::Right => zone.top_0().bottom_0().left_0().w(reach).cursor_col_resize(),
            Dock::Bottom => zone.left_0().right_0().top_0().h(reach).cursor_row_resize(),
        }
    }
}

/// Records the area its parent was laid out in.
fn measure(into: Rc<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    canvas(move |bounds, _, _| into.set(bounds), |_, _, _, _| {}).absolute().size_full()
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
        let tokens = *Tokens::global(cx);
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
            .bg(tokens.bg_sunken)
            .border_color(tokens.line)
            .when(held, |dock| dock.opacity(tokens.opacity_held));
        let root = match self.dock {
            Dock::Left => root.w(size).min_w_0().h_full().border_r_1(),
            Dock::Right => root.w(size).min_w_0().h_full().border_l_1(),
            Dock::Bottom => root.h(size).min_h_0().w_full().border_t_1(),
        };
        let strip = (page.tabs.len() > 1).then(|| self.strip(&page, &shown, &docks, &tokens, cx).into_any_element());
        let head = (page.tabs.len() <= 1 && shown != Panel::Console)
            .then(|| self.head(&shown, &title, &docks, &tokens, cx).into_any_element());
        root.children(strip)
            .children(head)
            .child(
                div()
                    .id(SharedString::from(format!("dock-{name}-panel")))
                    .test_support()
                    .relative()
                    .aria_label(title.clone())
                    .flex_1()
                    .min_h_0()
                    .child(measure(self.panel.clone()))
                    .when(shown == Panel::Console, |panel| {
                        let (carried, title) = (shown.clone(), title.clone());
                        let stage = present_of(&docks, &shown).is_some_and(|present| present.stage);
                        // The console's strip is its head. Its tabs and buttons take their own
                        // presses; one that reaches here and is in the strip's height is the head's.
                        panel.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                if event.position.y - this.panel.get().origin.y < px(tokens.strip_h) {
                                    let held = Held::Panel { panel: carried.clone(), title: title.clone(), stage, click: false };
                                    this.press(event.position, held, cx);
                                }
                            }),
                        )
                    })
                    .child(if shown == Panel::Console {
                        self.console.clone().into_any_element()
                    } else {
                        panel::body(title, &tokens).into_any_element()
                    }),
            )
            .child(self.sash(sizing, &tokens, cx))
            .into_any_element()
    }
}
