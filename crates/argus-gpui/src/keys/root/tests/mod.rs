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
    pub shell: Entity<Shell>,
    pub calls: Calls,
    pub events: UnboundedSender<Vec<Event>>,
}

/// The shell in a window whose calls are collected, and whose events are the test's to send. Its
/// Workspace pane has the keys.
pub(crate) fn open(cx: &mut TestAppContext) -> Opened {
    open_sized(cx, 1400.0)
}

/// The shell in a window `width` wide: a window of 1200 or less folds the sidebar to icons.
pub(crate) fn open_sized(cx: &mut TestAppContext, width: f32) -> Opened {
    let calls: Calls = Rc::default();
    let sink = calls.clone();
    let (events, listening) = tokio::sync::mpsc::unbounded_channel();
    let (handle, heard, shell) = open_on_sized(cx, Caller::new(move |call| sink.borrow_mut().push(call)), listening, width);
    Opened { handle, heard, shell, calls, events }
}

/// The shell in a window over `caller`, hearing `listening`.
pub(crate) fn open_on(cx: &mut TestAppContext, caller: Caller, listening: UnboundedReceiver<Vec<Event>>) -> (AnyWindowHandle, Entity<Heard>, Entity<Shell>) {
    open_on_sized(cx, caller, listening, 1400.0)
}

fn open_on_sized(cx: &mut TestAppContext, caller: Caller, listening: UnboundedReceiver<Vec<Event>>, width: f32) -> (AnyWindowHandle, Entity<Heard>, Entity<Shell>) {
    cx.update(crate::window::init);
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(width), px(700.0))))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let heard = cx.new(|_| Heard::new());
                heard.update(cx, |heard, cx| heard.listen(listening, cx));
                let shell = cx.new(|cx| Shell::new(heard.clone(), caller, window, cx));
                built = Some((heard, shell.clone()));
                shell
            },
        )
        .expect("open test window")
    });
    cx.update_window(handle, |_, window, _| window.activate_window()).unwrap();
    cx.run_until_parked();
    let (heard, shell) = built.expect("the window built its model");
    (handle, heard, shell)
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

/// Sends `events` to the window as the session would, and lets it draw them.
pub(crate) fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    opened.events.send(events).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

/// Whether the window draws an element with the accessible name `label` (and one only).
pub(crate) fn has_label(cx: &mut TestAppContext, opened: &Opened, label: &str) -> bool {
    use gpui_kit::test::TestQueryExt as _;
    cx.update_window(opened.handle, |_, window, _| window.try_find_by_label(label).is_some()).unwrap()
}

/// Clicks the element with the accessible name `label`.
pub(crate) fn click_label(cx: &mut TestAppContext, opened: &Opened, label: &str) {
    use gpui_kit::test::{TestEventExt as _, TestQueryExt as _};
    let find = |cx: &mut TestAppContext| cx.update_window(opened.handle, |_, window, _| window.find_by_label(label).bounds().center()).unwrap();
    // A dialog slides in over its first frames, on the clock of the machine and not the test's: the
    // pointer goes where the element is once a frame the move draws has left it where it was.
    let mut at = find(cx);
    for _ in 0..60 {
        cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, None, cx)).unwrap();
        let next = find(cx);
        if next == at {
            break;
        }
        at = next;
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_move(at, None, cx);
        window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

/// Types text into whatever has the keys.
pub(crate) fn type_text(cx: &mut TestAppContext, opened: &Opened, text: &str) {
    for ch in text.chars() {
        stroke(cx, opened, Keystroke { modifiers: Modifiers::none(), key: ch.to_string(), key_char: Some(ch.to_string()) });
    }
}

/// The calls so far, cleared.
pub(crate) fn drain(opened: &Opened) -> Vec<String> {
    let said: Vec<String> = asked(opened).into_iter().filter(|call| !call.starts_with("Resize")).collect();
    opened.calls.borrow_mut().clear();
    said
}

/// The `settings` event a session sends once it is up: the Settings sheet opens on nothing else
/// (SHELL-SETTINGS-1), so a test that opens it hears this first.
pub(crate) fn settings_event() -> Event {
    use argus_desktop_core::connection::page::SettingsPage;
    use argus_desktop_core::settings::remembered::Preferences;
    Event::Settings(Box::new(SettingsPage { chosen: Preferences::default(), min_opacity: 30, backdrops: Vec::new(), fonts: Vec::new(), version: "9.9.9".into() }))
}
