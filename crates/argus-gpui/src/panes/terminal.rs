//! A live pane's screen: the Workspace's on the stage, or one of the
//! console's two sides. What it draws of the pane its side shows, who has
//! the keys, the keys it sends, and the room it reports.
//!
//! It draws `Heard`'s screen of its side only while that screen is of the pane
//! the side shows (the rail's shown pane, or the console's), so the old
//! pane's rows never sit under the new one's header; until the first frame of
//! a newly picked pane arrives it is empty (the page's skeleton after 250 ms
//! is a later step). It repaints when `Heard` says a frame changed its side's
//! screen, never on a timer.
//!
//! The keys are the session's: a `Call::Key` goes to whichever pane it says
//! has them, whatever screen GPUI's focus is on, so the console's screens
//! take GPUI's focus only when the console says they are typed in. A press
//! on a console screen that is not typed in asks for the keys
//! (`console{select}`) and waits for the answer, as the page's does.
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
use argus_desktop_core::panes::console::ConsoleGesture;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::TestSupportExt as _;
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
    side: Side,
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
    /// A settled size was not sent because a sash was held; sent when it is let go.
    waiting: bool,
    settling: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Terminal {
    pub fn new(heard: Entity<Heard>, caller: Caller, side: Side, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let workspace = side == Side::Workspace;
        let subscriptions = vec![
            // A frame repaints; the rail moving the shown pane re-picks what is drawn.
            cx.subscribe(&heard, move |_, _, painted: &Painted, cx| {
                if painted.0 == side {
                    cx.notify();
                }
            }),
            cx.observe(&heard, |this, _, cx| {
                // A sash let go: the size it settled on is the one to report.
                if this.waiting {
                    this.settle(cx);
                }
                cx.notify();
            }),
            // The cursor is hollow without the keys or the window.
            cx.on_focus_in(&focus, window, |_, _, cx| cx.notify()),
            cx.on_focus_out(&focus, window, |_, _, _, cx| cx.notify()),
            cx.observe_window_activation(window, move |this, window, cx| {
                // One report for the window, not one per screen.
                if workspace {
                    this.caller.call(Call::Focus { focused: window.is_window_active() });
                }
                cx.notify();
            }),
        ];
        // The pane is what the person types into until they pick another place.
        if workspace {
            window.focus(&focus, cx);
        }
        Self {
            side,
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
            waiting: false,
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
            this.update(cx, |this, cx| this.settle(cx)).ok();
        }));
    }

    /// Sends the settled room, unless a sash is held: the stage is then being
    /// resized by a gesture and reports when it is let go (see `observe` in `new`).
    fn settle(&mut self, cx: &Context<Self>) {
        if self.heard.read(cx).resizing {
            self.waiting = true;
            return;
        }
        self.waiting = false;
        if self.wanted == self.sent {
            return;
        }
        let Some((rows, cols)) = self.wanted else {
            return;
        };
        self.sent = self.wanted;
        self.caller.call(Call::Resize { side: self.side, rows, cols });
    }

    /// A press on the screen. The Workspace's takes the keys. A console's
    /// takes them when the console says they are its own, and otherwise
    /// asks for them and waits: the focus moves when the `Console` event
    /// says so (`panes/console`), not on the press, and nothing is asked
    /// while the daemon is gone.
    fn pressed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let heard = self.heard.read(cx);
        let pane = heard.pane_of(self.side);
        let keyed = if self.side == Side::Workspace { heard.console.typing.is_none() } else { heard.keyed(pane) };
        if keyed {
            window.focus(&self.focus, cx);
            return;
        }
        // GPUI hands a tracked element the keys on any press inside it, which
        // would move them before the session has said so.
        window.prevent_default();
        if let Some(pane) = pane.filter(|_| !heard.held()) {
            self.caller.call(if self.side == Side::Workspace {
                Call::Select { pane }
            } else {
                Call::Console { gesture: ConsoleGesture::Select { pane } }
            });
        }
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
fn drawn(heard: &Heard, side: Side, blank: &Arc<Screen>) -> Arc<Screen> {
    let shown = heard.pane_of(side);
    if shown.is_some() && heard.screen_of(side).pane() == shown {
        heard.screen_of(side).clone()
    } else {
        blank.clone()
    }
}

/// The element id of a side's screen.
fn id(side: Side) -> &'static str {
    match side {
        Side::First => "terminal-first",
        Side::Second => "terminal-second",
        _ => "terminal",
    }
}

impl Render for Terminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let screen = drawn(self.heard.read(cx), self.side, &self.blank);
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
            .id(id(self.side))
            .test_support()
            .key_context("Terminal")
            .track_focus(&self.focus)
            .size_full()
            .bg(ledger.background)
            .p(px(8.0))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.press(event, cx)))
            // A press with any button on the pane gives it the keys, or asks for them
            // (SHELL-INPUT-5, -9); only the start of a selection is the left button's.
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.pressed(window, cx)))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, _, window, cx| this.pressed(window, cx)))
            .on_mouse_down(MouseButton::Middle, cx.listener(|this, _, window, cx| this.pressed(window, cx)))
            .child(element)
    }
}

#[cfg(test)]
mod tests;
