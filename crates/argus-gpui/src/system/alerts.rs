//! Handing the alerts a session raised to the operating system's notifications.
//!
//! What raises one is core's (`system::notifications`); the owner's outlet only queues it, since
//! the session's lock is held on a daemon thread and the platform's notification centre belongs
//! to the UI thread. This drains that queue there, through the kit's `Notification::system`,
//! which also activates the window when the person clicks the notification, as the Tauri
//! window's does. The kit posts nothing in-app: a toast over a window the person is looking at
//! is the notice line's job.

use argus_desktop_core::system::notifications::Alert;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{AnyWindowHandle, App, Task};
use tokio::sync::mpsc::UnboundedReceiver;

/// The notification an alert is: its own, so one pane's does not replace another's.
pub fn notification(alert: &Alert) -> Notification {
    Notification::new().title(alert.title.clone()).message(alert.body.clone()).system()
}

/// Posts each alert that arrives from `window`, until the queue closes or the window goes. The
/// task is the loop; the caller holds it.
pub fn raise_from(window: AnyWindowHandle, mut alerts: UnboundedReceiver<Alert>, cx: &mut App) -> Task<()> {
    cx.spawn(async move |cx| {
        while let Some(alert) = alerts.recv().await {
            // Best effort: a gone window is a gone person to tell.
            if window.update(cx, |_, window, cx| window.push_notification(notification(&alert), cx)).is_err() {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{px, size, AppContext as _, Bounds, TestAppContext, WindowBounds, WindowOptions};
    use tokio::sync::mpsc::unbounded_channel;

    struct Blank;

    impl gpui_kit::Render for Blank {
        fn render(&mut self, _: &mut gpui_kit::Window, _: &mut gpui_kit::Context<Self>) -> impl gpui_kit::IntoElement {
            gpui_kit::div()
        }
    }

    fn alert(title: &str, body: &str) -> Alert {
        Alert { title: title.into(), body: body.into() }
    }

    /// A window the notifications can be pushed from, as the kit's own `open_window` builds one.
    fn window(cx: &mut TestAppContext) -> AnyWindowHandle {
        cx.update(crate::window::init);
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(400.0), px(300.0))))),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(|_| Blank),
            )
            .expect("open test window")
        });
        handle
    }

    #[gpui_kit::test]
    fn an_alert_goes_to_the_system_with_its_title_and_body_and_no_toast_in_the_window(cx: &mut TestAppContext) {
        let handle = window(cx);
        let (send, alerts) = unbounded_channel();
        let _loop = cx.update(|cx| raise_from(handle, alerts, cx));
        send.send(alert("Trim context", "Is waiting for you · argus / main")).unwrap();
        cx.run_until_parked();
        let shown = cx.shown_system_notifications();
        assert_eq!(shown.len(), 1);
        assert_eq!((shown[0].title.as_ref(), shown[0].body.as_ref()), ("Trim context", "Is waiting for you · argus / main"));
    }

    #[gpui_kit::test]
    fn two_panes_alerts_are_two_notifications_not_one_replacing_the_other(cx: &mut TestAppContext) {
        let handle = window(cx);
        let (send, alerts) = unbounded_channel();
        let _loop = cx.update(|cx| raise_from(handle, alerts, cx));
        send.send(alert("one", "Failed")).unwrap();
        send.send(alert("two", "Failed")).unwrap();
        cx.run_until_parked();
        let shown = cx.shown_system_notifications();
        assert_eq!(shown.len(), 2);
        assert_ne!(shown[0].tag, shown[1].tag);
    }

    #[gpui_kit::test]
    fn the_app_is_named_for_the_systems_notifications(cx: &mut TestAppContext) {
        cx.update(crate::window::init);
        assert_eq!(cx.app_identity(), Some(("com.github.hbaldwin98.argus".into(), "Argus".into())));
    }
}
