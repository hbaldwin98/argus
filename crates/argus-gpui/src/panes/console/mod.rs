//! The console as the window draws it: a tab per console the daemon holds,
//! the strip's buttons, and the console's sides, each a screen of its own
//! (two at once while it is split) headed by its console.
//!
//! Everything it draws is `Heard`'s last `ConsolePage`, and every gesture is
//! the `Call` the page sends: nothing on screen changes until the `Console`
//! event that answers it, so a refused select, close or spawn leaves the
//! console as it is. Where the console is (docked, the Console stage,
//! fullscreen) and which dock holds it are `layout`'s; this is what is in it.
//!
//! The keys are the session's: its `typing` says which side has them, and
//! GPUI's focus follows that, from where the keys already were in the
//! Workspace's pane or a console screen. A rail pick or a text field that has
//! them keeps them. While the daemon is gone the console is dimmed and
//! answers no press, as the sidebar does (`Heard::held`).

pub mod rename;
pub mod strip;

use argus_desktop_core::connection::page::{Call, Side};
use argus_desktop_core::keys::keymap::{Action, Place};
use argus_desktop_core::layout::docks::DocksPage;
use argus_desktop_core::panes::console::{ConsoleGesture, ConsolePage, ConsoleTab};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, Icon, IconName, Sizable as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    div, prelude::*, px, Anchor, App, ClickEvent, Context, Entity, FocusHandle, Hsla, MouseButton, Role, SharedString,
    Subscription, Window,
};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::caps;
use crate::panes::terminal::Terminal;
use crate::theme::Ledger;

use strip::{Does, Offer};

/// The height of the console's head and of a split side's.
pub const HEAD: f32 = 34.0;

pub struct ConsoleView {
    heard: Entity<Heard>,
    caller: Caller,
    first: Entity<Terminal>,
    second: Entity<Terminal>,
    /// The Workspace's pane, which has the keys when no console does.
    stage: FocusHandle,
    /// The screen the keys were last moved to, which is not the console's
    /// pane id: an unsplit or a trade of sides moves the keys between screens
    /// with the same pane typed.
    typed: Option<FocusHandle>,
    _observing: Subscription,
}

impl ConsoleView {
    pub fn new(heard: Entity<Heard>, caller: Caller, stage: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let side = |side, window: &mut Window, cx: &mut Context<Self>| {
            let (heard, caller) = (heard.clone(), caller.clone());
            cx.new(|cx| Terminal::new(heard, caller, side, window, cx))
        };
        let first = side(Side::First, window, cx);
        let second = side(Side::Second, window, cx);
        let observing = cx.observe_in(&heard, window, |this, _, window, cx| {
            this.follow_keys(window, cx);
            cx.notify();
        });
        let mut view = Self { heard, caller, first, second, stage, typed: None, _observing: observing };
        view.typed = view.keyed_handle(cx);
        view
    }

    /// The handle of the screen that has the console's keys, if a console does.
    pub fn keyed_handle(&self, cx: &App) -> Option<FocusHandle> {
        let console = &self.heard.read(cx).console;
        let typing = console.typing?;
        if Some(typing) == console.showing {
            Some(self.first.read(cx).focus_handle().clone())
        } else if Some(typing) == console.beside {
            Some(self.second.read(cx).focus_handle().clone())
        } else {
            None
        }
    }

    /// The session moved the keys: GPUI's focus goes with them, if it was in
    /// the panes (the Workspace's or a console screen). A rail pick, a field
    /// or a dialog that has the keys keeps them.
    fn follow_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keyed = self.keyed_handle(cx);
        if keyed == self.typed {
            return;
        }
        self.typed = keyed.clone();
        let (first, second) = (self.first.read(cx).focus_handle().clone(), self.second.read(cx).focus_handle().clone());
        let in_panes = window.focused(cx).is_none_or(|held| held == self.stage || held == first || held == second);
        if in_panes {
            window.focus(&keyed.unwrap_or_else(|| self.stage.clone()), cx);
        }
    }

    /// A gesture on the console, sent unless the daemon is gone.
    fn gesture(&self, gesture: ConsoleGesture, cx: &App) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::Console { gesture });
        }
    }

    fn tab(&self, tab: &ConsoleTab, page: &ConsolePage, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = tab.pane;
        let live = Some(pane) == page.showing || Some(pane) == page.beside;
        let offers = strip::tab_offers(tab, false, &page.tabs);
        let (caller, heard) = (self.caller.clone(), self.heard.clone());
        let ledger = *ledger;
        let (hover, selected, line, muted, foreground, faint) =
            (ledger.raised, ledger.selected, ledger.line_strong, ledger.muted, ledger.foreground, ledger.faint);
        let status = tab.status;
        let title = SharedString::from(tab.title.clone());
        h_flex()
            .id(SharedString::from(format!("console-tab-{pane}")))
            .role(Role::Tab)
            .test_support()
            .aria_selected(tab.on)
            .aria_label(title.clone())
            .flex_shrink_0()
            .h(px(26.0))
            .items_center()
            .rounded(px(4.0))
            .border_1()
            .border_color(if tab.on { line } else { gpui_kit::transparent_black() })
            .text_sm()
            .text_color(if tab.on || live { foreground } else { muted })
            .when(tab.on, |tab| tab.bg(selected))
            .when(!tab.on, |tab| tab.hover(move |style| style.bg(hover)))
            // The press is the tab's alone, as the page's `closesOnMiddle`
            // keeps it from autoscrolling or reaching what is under it.
            .on_mouse_down(MouseButton::Middle, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            // A click, as the page's `auxclick`: the press and the release both on the tab.
            .on_aux_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if event.is_middle_click() {
                    cx.stop_propagation();
                    this.gesture(ConsoleGesture::Close { pane }, cx)
                }
            }))
            .child(
                h_flex()
                    .id(SharedString::from(format!("console-tab-pick-{pane}")))
                    .role(Role::Button)
                    .test_support()
                    .aria_label(title.clone())
                    .h_full()
                    .pl_3()
                    .pr_1()
                    .gap_1p5()
                    .items_center()
                    .cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(format!("shell, {status}")).build(window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::Select { pane }, cx)))
                    .child(div().max_w(px(180.0)).overflow_hidden().whitespace_nowrap().child(title.clone()))
                    .when(strip::wears_dot(status), |pick| pick.child(dot(format!("console-tab-dot-{pane}"), status, &ledger))),
            )
            .child(
                div()
                    .id(SharedString::from(format!("console-tab-close-{pane}")))
                    .role(Role::Button)
                    .test_support()
                    .aria_label(format!("Close {}", tab.title))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(18.0))
                    .mr_1()
                    .rounded(px(3.0))
                    .text_color(faint)
                    .hover(move |style| style.text_color(foreground))
                    .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::Close { pane }, cx)))
                    .child(Icon::new(IconName::Close).xsmall()),
            )
            .context_menu(move |menu, _, cx| build(menu, &offers, &caller, &heard, ledger, cx))
    }

    fn head(&self, page: &ConsolePage, docks: &DocksPage, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<_> = page.tabs.iter().map(|tab| self.tab(tab, page, ledger, cx).into_any_element()).collect();
        let (offers, hide) = {
            let keymap = &self.heard.read(cx).keymap;
            (
                strip::strip_offers(page, docks, keymap),
                caps::titled("Hide the console", keymap, Action::ToggleConsole, Place::Anywhere),
            )
        };
        let (caller, heard, ledger_copy) = (self.caller.clone(), self.heard.clone(), *ledger);
        h_flex()
            .id("console-head")
            .test_support()
            .flex_shrink_0()
            .h(px(HEAD))
            .pl_3()
            .pr_2()
            .gap_1()
            .items_center()
            .bg(ledger.chrome)
            .border_b_1()
            .border_color(ledger.line)
            .child(h_flex().id("console-tabs").role(Role::TabList).min_w_0().gap_1().overflow_hidden().children(tabs))
            .when(page.can_spawn, |head| {
                head.child(
                    Button::new("console-new")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Plus)
                        .tooltip("New console")
                        .on_click(cx.listener(|this, _, _, cx| this.gesture(ConsoleGesture::Spawn, cx))),
                )
            })
            .child(div().flex_1())
            .child(
                // The kit's button has no accessible label of its own, so the wrapper carries it.
                div().id("console-menu-label").role(Role::Button).test_support().aria_label("Console menu").child(
                    Button::new("console-menu")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Ellipsis)
                        .tooltip("Split, move, fill the window")
                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, cx| build(menu, &offers, &caller, &heard, ledger_copy, cx)),
                ),
            )
            .child(
                // The tooltip's words are the button's name too, which the kit's button lacks.
                div().id("console-hide-label").role(Role::Button).test_support().aria_label(hide.clone()).child(
                    Button::new("console-hide")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .tooltip(hide)
                        .on_click(cx.listener(|this, _, _, cx| this.gesture(ConsoleGesture::Hide, cx))),
                ),
            )
    }

    /// A split side's header: its console's title and dot, the whole of it a
    /// button that gives that side the keys, and a close that takes it out of
    /// the split and leaves its console a tab. The side with the keys is marked.
    fn side_head(&self, tab: &ConsoleTab, page: &ConsolePage, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = tab.pane;
        let typing = page.typing == Some(pane);
        let offers = strip::tab_offers(tab, true, &page.tabs);
        let (caller, heard, ledger) = (self.caller.clone(), self.heard.clone(), *ledger);
        let status = tab.status;
        let (hover, foreground, muted, faint) = (ledger.raised, ledger.foreground, ledger.muted, ledger.faint);
        let title = SharedString::from(tab.title.clone());
        let tip = if typing { tab.title.clone() } else { format!("Type in {}", tab.title) };
        h_flex()
            .id(SharedString::from(format!("console-side-head-{pane}")))
            .test_support()
            .aria_selected(typing)
            .flex_shrink_0()
            .h(px(HEAD))
            .items_center()
            .bg(if typing { ledger.selected } else { ledger.chrome })
            .border_b_1()
            .border_color(if typing { ledger.accent } else { ledger.line })
            .text_sm()
            .text_color(if typing { foreground } else { muted })
            .when(!typing, |head| head.hover(move |style| style.bg(hover).text_color(foreground)))
            .child(
                h_flex()
                    .id(SharedString::from(format!("console-side-pick-{pane}")))
                    .role(Role::Button)
                    .test_support()
                    .aria_label(SharedString::from(tip.clone()))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px_3()
                    .gap_1p5()
                    .items_center()
                    .cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::Select { pane }, cx)))
                    .child(div().overflow_hidden().whitespace_nowrap().child(title))
                    .when(strip::wears_dot(status), |pick| pick.child(dot(format!("console-side-dot-{pane}"), status, &ledger))),
            )
            .child(
                div()
                    .id(SharedString::from(format!("console-side-close-{pane}")))
                    .role(Role::Button)
                    .test_support()
                    .aria_label(format!("Take {} out of the split", tab.title))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(18.0))
                    .mr_1()
                    .rounded(px(3.0))
                    .text_color(faint)
                    .hover(move |style| style.text_color(foreground))
                    .tooltip(|window, cx| Tooltip::new("Take out of the split").build(window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::CloseSide { pane }, cx)))
                    .child(Icon::new(IconName::Close).xsmall()),
            )
            .context_menu(move |menu, _, cx| build(menu, &offers, &caller, &heard, ledger, cx))
    }

    /// One side of the console: its screen, under its header while split.
    fn side(&self, id: &'static str, pane: u64, screen: Entity<Terminal>, page: &ConsolePage, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let head = page
            .beside
            .and_then(|_| page.tabs.iter().find(|tab| tab.pane == pane))
            .map(|tab| self.side_head(tab, page, ledger, cx).into_any_element());
        v_flex()
            .id(id)
            .test_support()
            .flex_1()
            .min_w_0()
            .h_full()
            .children(head)
            .child(div().flex_1().min_h_0().child(screen))
    }

    /// With no console on screen: what to press to get one.
    fn empty(&self, page: &ConsolePage, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let chosen = page.tabs.iter().find(|tab| tab.on).or(page.tabs.first()).map(|tab| tab.pane);
        let body = v_flex().id("console-empty").test_support().flex_1().min_h_0().items_center().justify_center().gap_2();
        match chosen {
            None => body
                .child(
                    div()
                        .text_sm()
                        .text_color(ledger.muted)
                        .child("No consoles yet: a shell in your home directory, in no checkout."),
                )
                .when(page.can_spawn, |body| {
                    body.child(
                        Button::new("console-start")
                            .label("Start a console")
                            .on_click(cx.listener(|this, _, _, cx| this.gesture(ConsoleGesture::Spawn, cx))),
                    )
                }),
            Some(pane) => body.child(
                Button::new("console-type")
                    .label("Type in the console")
                    .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::Select { pane }, cx))),
            ),
        }
    }
}

/// The dot a console wears: what it is doing, in the colour the page gives it.
fn dot(id: String, status: &str, ledger: &Ledger) -> impl IntoElement {
    let colour: Hsla = match status {
        "working" => ledger.accent,
        "waiting" | "needs-review" => ledger.warning,
        "failed" => ledger.danger,
        "done" => ledger.success,
        _ => ledger.faint,
    };
    div().id(SharedString::from(id)).test_support().size(px(8.0)).flex_shrink_0().rounded_full().bg(colour)
}

/// The keys an item names, each press a cap, the alternatives apart by a slash.
fn keycaps(keys: &[Vec<String>], ledger: &Ledger) -> impl IntoElement {
    let (line, muted) = (ledger.line_strong, ledger.muted);
    let mut caps = h_flex().gap_1().items_center().text_xs().text_color(muted);
    for (n, presses) in keys.iter().enumerate() {
        if n > 0 {
            caps = caps.child("/");
        }
        for press in presses {
            caps = caps.child(div().px_1().rounded(px(3.0)).border_1().border_color(line).child(SharedString::from(press.clone())));
        }
    }
    caps
}

/// The kit's menu for `offers`: each item sends its calls through `caller`,
/// and `Rename...` opens the dialog once the menu has let go of the keys, so
/// the dialog gives them back to what had them before the menu. While the
/// daemon is gone every item is disabled, and a click that still arrives
/// sends nothing.
fn build(mut menu: PopupMenu, offers: &[Offer], caller: &Caller, heard: &Entity<Heard>, ledger: Ledger, cx: &App) -> PopupMenu {
    let held = heard.read(cx).held();
    for offer in offers {
        menu = match offer {
            Offer::Heading(text) => menu.label(text.clone()),
            Offer::Separator => menu.separator(),
            Offer::Item { label, does, danger, here, keys } => {
                let (caller, heard, does) = (caller.clone(), heard.clone(), does.clone());
                let label = SharedString::from(label.clone());
                let item = if *danger || !keys.is_empty() {
                    let (shown, keys) = (label.clone(), keys.clone());
                    let colour = if *danger { ledger.danger } else { ledger.foreground };
                    PopupMenuItem::element(move |_, _| {
                        h_flex()
                            .w_full()
                            .gap_4()
                            .justify_between()
                            .text_color(colour)
                            .child(shown.clone())
                            .child(keycaps(&keys, &ledger))
                    })
                } else {
                    PopupMenuItem::new(label)
                };
                menu.item(item.checked(*here).disabled(*here || held).on_click(move |_, window, cx| {
                    if heard.read(cx).held() {
                        return;
                    }
                    match &does {
                        Does::Calls(calls) => calls.iter().for_each(|call| caller.call(call.clone())),
                        Does::Rename { pane, title } => {
                            let (pane, title, caller, heard) = (*pane, title.clone(), caller.clone(), heard.clone());
                            window.defer(cx, move |window, cx| rename::open(pane, title, caller, heard, window, cx));
                        }
                    }
                }))
            }
        };
    }
    menu
}

impl Render for ConsoleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let (page, docks, held) = {
            let heard = self.heard.read(cx);
            (heard.console.clone(), heard.docks.clone(), heard.held())
        };
        let head = self.head(&page, &docks, &ledger, cx);
        let body = match (page.showing, page.beside) {
            (Some(first), second) => {
                let first = self.side("console-first", first, self.first.clone(), &page, &ledger, cx);
                let second = second.map(|pane| {
                    self.side("console-second", pane, self.second.clone(), &page, &ledger, cx)
                });
                h_flex()
                    .id("console-sides")
                    .flex_1()
                    .min_h_0()
                    .child(first)
                    .when_some(second, |sides, second| {
                        sides.child(div().w(px(1.0)).h_full().flex_shrink_0().bg(ledger.line)).child(second)
                    })
                    .into_any_element()
            }
            (None, _) => self.empty(&page, &ledger, cx).into_any_element(),
        };
        v_flex()
            .id("console")
            .test_support()
            .aria_label("Console")
            .size_full()
            .min_h_0()
            .bg(ledger.background)
            .when(held, |console| console.opacity(0.5))
            .child(head)
            .child(body)
    }
}

#[cfg(test)]
mod tests;
