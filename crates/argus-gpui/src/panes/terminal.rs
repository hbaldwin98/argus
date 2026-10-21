//! The Workspace's live pane on the stage: what it draws of the shown pane,
//! who has the keys, the keys it sends, and the room it reports.
//!
//! It draws `Heard`'s screen only while that screen is of the pane the rail
//! says is shown, so the old pane's rows never sit under the new one's
//! header; until the first frame of a newly picked pane arrives the stage is
//! empty (the page's skeleton after 250 ms is a later step). It repaints when
//! `Heard` says a frame changed the screen, never on a timer.
//!
//! Keys go to the session as the page sends them: every keystroke it can
//! name is a `Call::Key`, and what the leader, the paste chord and the
//! child's keyboard mode make of it is the session's. A key it does not
//! take (a bare modifier, an input method's) goes on to GPUI's input
//! handler (`keys/text`), which sends the committed text. Mouse reports,
//! selection and the wheel are not here yet.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use argus_desktop_core::connection::page::{Call, Side};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, prelude::*, px, App, Context, Entity, FocusHandle, KeyDownEvent, MouseButton, Pixels, Subscription, Task,
    Window,
};

use crate::connection::caller::Caller;
use crate::connection::heard::{Heard, Painted};
use crate::keys::page_key;
use crate::panes::element::TerminalElement;
use crate::panes::palette::Palette;
use crate::panes::screen::{Cursor, Screen};
use crate::theme::Ledger;

/// How long a size stays unreported after it last changed: a window being
/// dragged would otherwise reflow the pane at every step.
const SETTLE: Duration = Duration::from_millis(50);

pub struct Terminal {
    heard: Entity<Heard>,
    caller: Caller,
    focus: FocusHandle,
    palette: Palette,
    /// Stood in for a screen that is not of the pane on screen.
    blank: Arc<Screen>,
    /// The grid the element last said its size held.
    seen: Rc<Cell<Option<(u16, u16)>>>,
    pub(crate) cell: Rc<Cell<(Pixels, Pixels)>>,
    paints: Rc<Cell<u64>>,
    /// The room the element measured, waiting out [`SETTLE`].
    wanted: Option<(u16, u16)>,
    /// The room last reported to the session, which is told only a change.
    sent: Option<(u16, u16)>,
    settling: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Terminal {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            // A frame repaints; the rail moving the shown pane re-picks what is drawn.
            cx.subscribe(&heard, |_, _, _: &Painted, cx| cx.notify()),
            cx.observe(&heard, |_, _, cx| cx.notify()),
            // The cursor is hollow without the keys or the window.
            cx.on_focus_in(&focus, window, |_, _, cx| cx.notify()),
            cx.on_focus_out(&focus, window, |_, _, _, cx| cx.notify()),
            cx.observe_window_activation(window, |this, window, cx| {
                this.caller.call(Call::Focus { focused: window.is_window_active() });
                cx.notify();
            }),
        ];
        // The pane is what the person types into until they pick another place.
        window.focus(&focus, cx);
        Self {
            heard,
            caller,
            focus,
            palette: Palette::ledger(),
            blank: Arc::new(Screen::new()),
            seen: Rc::default(),
            cell: Rc::new(Cell::new((px(0.0), px(0.0)))),
            paints: Rc::default(),
            wanted: None,
            sent: None,
            settling: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// How many times the screen was painted.
    pub fn paints(&self) -> u64 {
        self.paints.get()
    }

    /// The element's size now holds `rows` by `cols`. Reported to the session
    /// once it has held still for [`SETTLE`], and only when it differs from
    /// the last report.
    pub fn room(&mut self, rows: u16, cols: u16, cx: &mut Context<Self>) {
        self.wanted = Some((rows, cols));
        self.settling = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |this, _| this.settle()).ok();
        }));
    }

    fn settle(&mut self) {
        if self.wanted == self.sent {
            return;
        }
        let Some((rows, cols)) = self.wanted else {
            return;
        };
        self.sent = self.wanted;
        self.caller.call(Call::Resize { side: Side::Workspace, rows, cols });
    }

    /// A key in the pane, to the session as the page sends it.
    fn press(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(key) = page_key::from_keystroke(&event.keystroke) else {
            return;
        };
        self.caller.call(Call::Key { key });
        // Handled here, so the platform does not type it again as text.
        cx.stop_propagation();
    }

    /// Text an input method committed.
    pub(crate) fn text(&mut self, text: &str) {
        if !text.is_empty() {
            self.caller.call(Call::Text { text: text.to_owned() });
        }
    }

    /// Text pasted through the platform's own paste, as `paste{text}`.
    pub(crate) fn paste(&mut self, text: String) {
        if !text.is_empty() {
            self.caller.call(Call::Paste { text });
        }
    }

    /// The cursor, and the size of one cell, for the input method's
    /// candidate window to sit by; nothing before the first paint measured
    /// a cell.
    pub(crate) fn cursor_cell(&self, cx: &App) -> Option<(Cursor, (Pixels, Pixels))> {
        let cell = self.cell.get();
        (cell.0 > px(0.0)).then(|| (self.heard.read(cx).screen.cursor(), cell))
    }
}

/// The screen to draw: `heard`'s when it is of the pane the rail says is
/// shown, else `blank`. Picking another pane moves `shown` before the new
/// pane's first frame arrives, and the old rows are not the new pane's.
fn drawn(heard: &Heard, blank: &Arc<Screen>) -> Arc<Screen> {
    let shown = heard.rail.shown.as_ref().map(|shown| shown.pane);
    if shown.is_some() && heard.screen.pane() == shown {
        heard.screen.clone()
    } else {
        blank.clone()
    }
}

impl Render for Terminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let screen = drawn(self.heard.read(cx), &self.blank);
        let focused = self.focus.is_focused(window) && window.is_window_active();
        let element = TerminalElement {
            screen,
            palette: self.palette.clone(),
            family: cx.theme().mono_font_family.clone(),
            focused,
            terminal: cx.entity(),
            focus: self.focus.clone(),
            seen: self.seen.clone(),
            cell: self.cell.clone(),
            paints: self.paints.clone(),
        };
        div()
            .id("terminal")
            .key_context("Terminal")
            .track_focus(&self.focus)
            .size_full()
            .bg(ledger.background)
            .p(px(8.0))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.press(event, cx)))
            // A press on the pane gives it the keys.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| window.focus(&this.focus, cx)),
            )
            .child(element)
    }
}

#[cfg(test)]
mod tests;
