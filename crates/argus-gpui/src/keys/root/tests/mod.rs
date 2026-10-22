//! The key walk in a test window: the whole shell, with the keys sent through the window so they
//! pass the interceptor as a person's do.
//!
//! The harness is shared with the tests of the modules the walk is made of (`field`, `over`, `run`,
//! `hooks`), which cannot be tried without a window.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, Event};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{prelude::*, px, size, AnyWindowHandle, Bounds, Entity, Keystroke, Modifiers, TestAppContext, WindowBounds, WindowOptions};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::chrome::shell::Shell;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;

mod chords;
mod walk;

pub(crate) type Calls = Rc<RefCell<Vec<Call>>>;

pub(crate) struct Opened {
    pub handle: AnyWindowHandle,
    pub heard: Entity<Heard>,
    pub calls: Calls,
    pub events: UnboundedSender<Vec<Event>>,
}

/// The shell in a window whose calls are collected, and whose events are the test's to send. Its
/// Workspace pane has the keys.
pub(crate) fn open(cx: &mut TestAppContext) -> Opened {
    let calls: Calls = Rc::default();
    let sink = calls.clone();
    let (events, listening) = tokio::sync::mpsc::unbounded_channel();
    let (handle, heard) = open_on(cx, Caller::new(move |call| sink.borrow_mut().push(call)), listening);
    Opened { handle, heard, calls, events }
}

/// The shell in a window over `caller`, hearing `listening`.
pub(crate) fn open_on(cx: &mut TestAppContext, caller: Caller, listening: UnboundedReceiver<Vec<Event>>) -> (AnyWindowHandle, Entity<Heard>) {
    cx.update(crate::window::init);
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(1000.0), px(700.0))))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let heard = cx.new(|_| Heard::new());
                heard.update(cx, |heard, cx| heard.listen(listening, cx));
                built = Some(heard.clone());
                cx.new(|cx| Shell::new(heard, caller, window, cx))
            },
        )
        .expect("open test window")
    });
    cx.update_window(handle, |_, window, _| window.activate_window()).unwrap();
    cx.run_until_parked();
    (handle, built.expect("the window built its model"))
}

/// The keys leave the pane the shell gave them to and rest on nothing: the stage's keys.
pub(crate) fn blur(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| window.blur(cx)).unwrap();
    cx.run_until_parked();
}

/// Presses a key as the platform delivers it: the interceptor first, then the rest.
pub(crate) fn press(cx: &mut TestAppContext, opened: &Opened, key: &str) {
    stroke(cx, opened, Keystroke::parse(key).unwrap());
}

/// `?`, which is Shift and `/` printed `?`.
pub(crate) fn question_mark(cx: &mut TestAppContext, opened: &Opened) {
    let shift = Modifiers { shift: true, ..Modifiers::none() };
    stroke(cx, opened, Keystroke { modifiers: shift, key: "/".into(), key_char: Some("?".into()) });
}

pub(crate) fn stroke(cx: &mut TestAppContext, opened: &Opened, keystroke: Keystroke) {
    cx.update_window(opened.handle, |_, window, cx| window.dispatch_keystroke(keystroke, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

/// What the window asked of the session, as `Debug` says it, the focus reports left out.
pub(crate) fn asked(opened: &Opened) -> Vec<String> {
    opened
        .calls
        .borrow()
        .iter()
        .filter(|call| !matches!(call, Call::Focus { .. }))
        .map(|call| format!("{call:?}"))
        .collect()
}

pub(crate) fn overlays(cx: &mut TestAppContext) -> Vec<Overlay> {
    cx.update(|cx| Overlays::get(cx).open_kinds())
}

/// The Control chord that is the window's zoom, as the keymap spells it on this platform.
pub(crate) fn zoom(key: &str) -> String {
    format!("{}-{key}", if cfg!(target_os = "macos") { "cmd" } else { "ctrl" })
}
