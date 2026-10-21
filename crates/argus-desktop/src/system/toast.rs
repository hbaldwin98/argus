//! Handing an alert to the system's notifications. Whether a tree's
//! changes raise one, and what it says, is
//! `argus_desktop_core::system::notifications`.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use argus_desktop_core::system::notifications::Alert;

/// Best effort: a system with notifications off, or none to show them,
/// costs the person the notification and nothing else.
pub fn show(app: &AppHandle, alert: &Alert) {
    if let Err(err) = app.notification().builder().title(&alert.title).body(&alert.body).show() {
        tracing::warn!("could not raise a notification: {err}");
    }
}
