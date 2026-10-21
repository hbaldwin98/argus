//! The two Tauri commands the page invokes: `connect`, which opens its
//! channel, and `call`, which takes every [`Call`] and hands it to whatever
//! answers it — the session, or for the clipboard, the window's frame and
//! a reconnect and a view's links, the part of the window that owns them.
//! A setting is the session's, and the window's frame, backdrop and zoom
//! follow it.
//!
//! Each is `async` because Tauri runs a synchronous command on the UI
//! thread, and none of them should hold the window while it takes a lock.
//! They answer nothing: what a call changed reaches the page as events.

use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::{State, WebviewWindow};

use crate::settings::backdrop;
use crate::system::clipboard;
use crate::system::opener;
use crate::connection::page::{Call, Event, SettingChange};
use crate::connection::shared::Shared;
use crate::system::window_frame;

type Answer = Result<(), String>;

/// The page has loaded: events go down `events` from now on, starting
/// with everything it needs to draw.
#[tauri::command]
pub async fn connect(shared: State<'_, Arc<Shared>>, events: Channel<Event>) -> Answer {
    shared.attach_page(events);
    Ok(())
}

#[tauri::command]
pub async fn call(shared: State<'_, Arc<Shared>>, window: WebviewWindow, call: Call) -> Answer {
    let zoom = shared.read(|s| s.zoom());
    match call {
        Call::Key { key } if shared.read(|s| s.pastes(&key)) => paste_clipboard(&shared),
        Call::PasteClipboard => paste_clipboard(&shared),
        Call::Copy { from, to } => copy(&shared, from, to),
        Call::CopyText { text } => copy_text(&shared, &text),
        Call::Reconnect => shared.ask_reconnect(),
        Call::Edit { gesture } => shared.relay(gesture.message()),
        Call::SetHost { host } => shared.set_host(host),
        Call::Minimize => return window_frame::minimize(&window),
        Call::ToggleMaximize => return window_frame::toggle_maximize(&window),
        Call::CloseWindow => return window_frame::close(&window),
        Call::DragWindow => return window_frame::drag(&window),
        Call::SnapLayouts => return window_frame::snap_layouts(&window),
        Call::OpenUrl { url } => {
            if let Err(said) = opener::open(&url) {
                shared.act(|s| s.note(said));
            }
        }
        Call::Setting { change } => {
            let look = matches!(change, SettingChange::Theme { .. } | SettingChange::Backdrop { .. });
            shared.act(|s| s.call(Call::Setting { change }));
            if look {
                dress(&window, &shared);
            }
        }
        call => shared.act(|s| s.call(call)),
    }
    if shared.read(|s| s.zoom()) != zoom {
        return scale(&window, &shared);
    }
    Ok(())
}

/// Draws the whole page at the zoom chosen. The webview's own zoom rather
/// than the page's CSS, so the panes' cells grow with everything else: the
/// page sees a new pixel ratio and size, and refits each pane's grid.
pub fn scale(window: &WebviewWindow, shared: &Shared) -> Answer {
    let zoom = shared.read(|s| s.zoom());
    window.set_zoom(f64::from(zoom) / 100.0).map_err(|e| e.to_string())
}

/// Paints the window's frame and puts its backdrop on, as the look chosen
/// and the system's light or dark say.
pub fn dress(window: &WebviewWindow, shared: &Shared) {
    let (theme, chosen) = shared.read(|s| s.look());
    window_frame::paint(window, theme);
    backdrop::show(window, chosen, theme.is_dark(window_frame::system_dark(window)));
}

fn paste_clipboard(shared: &Shared) {
    // A picture first, as the terminal client does: a screenshot copied
    // from some apps carries a text form too, and the picture is the point.
    if let Some(png) = clipboard::read_image() {
        shared.act(|s| s.paste_file("clipboard.png".into(), png));
        return;
    }
    let files = clipboard::read_files();
    if !files.is_empty() {
        for path in files {
            let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
            match std::fs::read(&path) {
                Ok(bytes) => shared.act(|s| s.paste_file(name, bytes)),
                Err(e) => shared.act(|s| s.note(format!("could not read {name}: {e}"))),
            }
        }
        return;
    }
    match clipboard::read() {
        Some(text) => shared.act(|s| s.paste(text)),
        None => shared.act(|s| s.note("nothing on the clipboard to paste")),
    }
}

fn copy_text(shared: &Shared, text: &str) {
    if let Err(e) = clipboard::write(text) {
        shared.act(|s| s.note(format!("could not copy: {e}")));
    }
}

fn copy(shared: &Shared, from: (u16, u16), to: (u16, u16)) {
    let text = shared.read(|s| s.selected_text(from, to));
    if text.is_empty() {
        return;
    }
    if let Err(e) = clipboard::write(&text) {
        shared.act(|s| s.note(format!("could not copy: {e}")));
    }
}
