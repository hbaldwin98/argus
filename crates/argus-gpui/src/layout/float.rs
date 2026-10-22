//! The dialogs: panels floating over the work area, each in a frame of its own that its head moves
//! and its corner resizes, and the one pressed comes in front.
//!
//! Rust keeps where each sits, how big, and in what order (`DocksPage::floating`, back to front);
//! this draws them there. A press on a frame sends `raise` unless it is the one in front. The head
//! and the grip press into the work area's one drag (`layout/work`, [`Held::Float`]), which follows
//! the pointer after the 5 px threshold, draws the box the pointer has made until it is let go,
//! and sends the box once on release, only if it changed. The rules of the box (the least size,
//! how much must stay in reach) are `argus-desktop-core`'s (`layout/floats`), so what is drawn
//! live is what is reported. While the daemon is gone a frame is dimmed and answers no press.
//!
//! The frame is its own: the kit's dialog does not move. It is a head (the panel's title, the
//! dock menu and a close or hide), the panel's body and the grip. Only a saved view floats, and
//! its body is `views/saved`, which `layout/panel` hands to the frame.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{DockGesture, DocksPage, FloatBounds, FloatPage, Panel, PresentPanel};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{h_flex, v_flex, Disableable as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    canvas, div, point, CursorStyle, prelude::*, px, Bounds, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Pixels, PathBuilder, Point,
    Role, SharedString, Subscription, WeakEntity, Window,
};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::extensions::bodies::Bodies;
use crate::kit::icons::Glyph;
use crate::kit::strip;
use crate::layout::bodies::Contents;
use crate::layout::dock_menu;
use crate::layout::panel;
use crate::layout::work::{Held, WorkView};
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem, zoom};
use crate::theme::Tokens;

/// What a press on a frame holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    /// The head: the frame follows the pointer.
    Move,
    /// The corner: the frame's size follows it.
    Size,
}

/// The box a pointer holding `grip` since `from` has made of `start`, as it is drawn in an area
/// `area` big (in the page's pixels, as the box is): the travel in whole pixels, which is the pointer's
/// divided by the `zoom`, then the core's rules.
pub fn held(grip: Grip, start: FloatBounds, from: Point<Pixels>, at: Point<Pixels>, area: (u32, u32), zoom: f32) -> FloatBounds {
    let (dx, dy) = (((at.x - from.x).as_f32() / zoom).round() as i32, ((at.y - from.y).as_f32() / zoom).round() as i32);
    let made = match grip {
        Grip::Move => start.moved(dx, dy),
        Grip::Size => start.resized(dx, dy),
    };
    made.kept_in(area.0, area.1)
}

/// The call that reports a box let go: only when it is not the box that was pressed.
pub fn released(panel: Panel, start: FloatBounds, now: FloatBounds) -> Option<Call> {
    (now != start).then_some(Call::Docks { gesture: DockGesture::Bounds { panel, bounds: now } })
}

pub struct FloatsView {
    heard: Entity<Heard>,
    caller: Caller,
    work: WeakEntity<WorkView>,
    _observing: Subscription,
}

impl FloatsView {
    pub fn new(heard: Entity<Heard>, caller: Caller, work: WeakEntity<WorkView>, cx: &mut Context<Self>) -> Self {
        let observing = cx.observe(&heard, |_, _, cx| cx.notify());
        Self { heard, caller, work, _observing: observing }
    }

    fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// The area frames are kept in, once measured; the window's before then.
    fn area(&self, window: &Window, cx: &Context<Self>) -> (u32, u32) {
        let measured = self.work.upgrade().map(|work| work.read(cx).area().size).filter(|size| size.width > px(0.0));
        let size = measured.unwrap_or_else(|| window.viewport_size());
        let zoom = zoom(window.rem_size());
        ((size.width.as_f32() / zoom).max(0.0) as u32, (size.height.as_f32() / zoom).max(0.0) as u32)
    }

    fn press(&self, at: Point<Pixels>, held: Held, cx: &mut Context<Self>) {
        self.work.update(cx, |work, cx| work.press(at, held, cx)).ok();
    }

    /// The frame's head: the panel's title, which moves it, its dock menu and its close.
    fn head(&self, float: &FloatPage, bounds: FloatBounds, docks: &DocksPage, held: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = float.panel.clone();
        let present: Option<PresentPanel> = docks.present.iter().find(|present| present.panel == panel).cloned();
        let closes = present.as_ref().is_some_and(|present| present.closes);
        let entries = dock_menu::moves(&panel, false, docks, present.as_ref());
        let (caller, heard, menu_tokens) = (self.caller.clone(), self.heard.clone(), *tokens);
        let pressed = panel.clone();
        let (gone, words) = (panel.clone(), if closes { format!("Close {}", float.title) } else { format!("Hide {}", float.title) });
        let saved_marks = crate::views::saved::head::marks(&panel, self.heard.read(cx).saved.as_deref(), tokens);
        let gesture = move |panel: Panel| if closes { DockGesture::Close { panel } } else { DockGesture::Hide { panel } };
        // A press on a button of the head is the button's, not the frame's move.
        let keep = |button: gpui_kit::Div| button.flex_shrink_0().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        strip::bar(tokens)
            .id(SharedString::from(format!("float-head-{panel}")))
            .test_support()
            .pl(rem(tokens.sp_12))
            .typed(tokens.type_small)
            .weighted(tokens.weight_semibold)
            .text_color(tokens.fg)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    let held = Held::Float { panel: pressed.clone(), grip: Grip::Move, start: bounds, from: event.position };
                    this.press(event.position, held, cx);
                }),
            )
            .child(div().min_w_0().truncate().child(float.title.clone()))
            .children(saved_marks)
            .child(div().flex_1())
            .child(keep(div()).child(
                strip::menu_button(SharedString::from(format!("float-menu-{panel}")), Glyph::Down, tokens.icon_menu, tokens, cx)
                    .tooltip("Move to another dock")
                    .disabled(held)
                    .dropdown_menu(move |menu, window, cx| dock_menu::build(menu, &entries, &caller, &heard, menu_tokens, window, cx)),
            ))
            .child(keep(div()).child(
                strip::menu_button(SharedString::from(format!("float-close-{gone}")), Glyph::Close, tokens.icon_xs, tokens, cx)
                    .tooltip(words.clone())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.ask(Call::Docks { gesture: gesture(gone.clone()) }, cx)
                    })),
            ))
    }

    /// The corner a frame is resized by: two diagonal strokes in the strong line colour.
    fn grip(&self, float: &FloatPage, bounds: FloatBounds, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = float.panel.clone();
        let (line, size) = (tokens.line_strong, tokens.icon_sm);
        div()
            .id(SharedString::from(format!("float-grip-{panel}")))
            .test_support()
            .absolute()
            .right_0()
            .bottom_0()
            .w(rem(size))
            .h(rem(size))
            .cursor(CursorStyle::ResizeUpLeftDownRight)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    let held = Held::Float { panel: panel.clone(), grip: Grip::Size, start: bounds, from: event.position };
                    this.press(event.position, held, cx);
                }),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |area: Bounds<Pixels>, _, window, _| {
                        let (rem_size, edge) = (window.rem_size(), pixels(1.0, window.rem_size()));
                        for inset in [4.0, 8.0] {
                            let inset = pixels(inset, rem_size);
                            let mut stroke = PathBuilder::stroke(edge);
                            stroke.move_to(point(area.right() - edge, area.bottom() - inset));
                            stroke.line_to(point(area.right() - inset, area.bottom() - edge));
                            if let Ok(path) = stroke.build() {
                                window.paint_path(path, line);
                            }
                        }
                    },
                )
                .size_full(),
            )
    }
}

impl Render for FloatsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let (docks, held) = {
            let heard = self.heard.read(cx);
            (heard.docks.clone(), heard.held())
        };
        let area = self.area(window, cx);
        let front = docks.floating.last().map(|float| float.panel.clone());
        // The layer takes no press of its own: only a frame does.
        let layer = div().id("floats").test_support().absolute().top_0().left_0().size_full().overflow_hidden();
        layer.children(docks.floating.iter().map(|float| {
            // A frame held by the pointer is drawn where the pointer has it, whatever Rust last said.
            let drawn = self.work.upgrade().and_then(|work| work.read(cx).floated(&float.panel));
            let bounds = drawn.unwrap_or_else(|| float.bounds.kept_in(area.0, area.1));
            let panel = float.panel.clone();
            let raising = panel.clone();
            let is_front = front.as_ref() == Some(&panel);
            let extension = Bodies::of(cx).view(&panel, &self.heard, &self.caller, window, cx);
            v_flex()
                .id(SharedString::from(format!("float-{panel}")))
                .role(Role::Dialog)
                .test_support()
                .aria_label(SharedString::from(float.title.clone()))
                .occlude()
                .absolute()
                .left(rem(bounds.x as f32))
                .top(rem(bounds.y as f32))
                .w(rem(bounds.width as f32))
                .h(rem(bounds.height as f32))
                .overflow_hidden()
                .rounded(rem(tokens.r_md))
                .border_1()
                .border_color(tokens.line_strong)
                .bg(tokens.see_surface)
                .shadow(tokens.shadow_pop())
                .when(held, |frame| frame.opacity(tokens.opacity_held))
                // Any press on the frame brings it in front, buttons and body included, before what
                // is pressed acts (the page listens on the frame itself).
                .capture_any_mouse_down(cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if event.button == MouseButton::Left && !is_front {
                        this.ask(Call::Docks { gesture: DockGesture::Raise { panel: raising.clone() } }, cx);
                    }
                }))
                .child(self.head(float, bounds, &docks, held, &tokens, cx))
                .child(
                    h_flex()
                        .id(SharedString::from(format!("float-body-{panel}")))
                        .flex_1()
                        .min_h_0()
                        .child(match (extension, Contents::peek(cx).panel(&float.panel)) {
                            (Some(view), _) => view.into_any_element(),
                            (None, Some(body)) => body.into_any_element(),
                            (None, None) => panel::content(&float.panel, float.title.clone(), &self.heard, &self.caller, &tokens, cx),
                        }),
                )
                .child(self.grip(float, bounds, &tokens, cx))
        }))
    }
}

#[cfg(test)]
mod tests;
