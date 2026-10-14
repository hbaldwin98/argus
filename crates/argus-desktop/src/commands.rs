//! The calls the page makes, handed to the session.
//!
//! Each is `async` because Tauri runs a synchronous command on the UI
//! thread, and none of them should hold the window while it takes a lock.
//! They answer nothing: what a call changed reaches the page as events.

use std::sync::Arc;

use argus_protocol::{CheckoutId, PaneId, ProjectId, RepositoryId};
use tauri::ipc::Channel;
use tauri::State;

use crate::clipboard;
use crate::features::Layout;
use crate::keys::PageKey;
use crate::page::Event;
use crate::shared::Shared;

type Answer = Result<(), String>;

/// The page has loaded: events go down `events` from now on, starting
/// with everything it needs to draw.
#[tauri::command]
pub async fn connect(shared: State<'_, Arc<Shared>>, events: Channel<Event>) -> Answer {
    shared.attach_page(events);
    Ok(())
}

#[tauri::command]
pub async fn select(shared: State<'_, Arc<Shared>>, pane: u64) -> Answer {
    shared.act(|s| s.select(PaneId(pane)));
    Ok(())
}

#[tauri::command]
pub async fn resize(shared: State<'_, Arc<Shared>>, rows: u16, cols: u16) -> Answer {
    shared.act(|s| s.resize(rows, cols));
    Ok(())
}

#[tauri::command]
pub async fn key(shared: State<'_, Arc<Shared>>, key: PageKey) -> Answer {
    shared.act(|s| s.key(&key));
    Ok(())
}

#[tauri::command]
pub async fn text(shared: State<'_, Arc<Shared>>, text: String) -> Answer {
    shared.act(|s| s.text(&text));
    Ok(())
}

#[tauri::command]
pub async fn paste(shared: State<'_, Arc<Shared>>, text: String) -> Answer {
    shared.act(|s| s.paste(text));
    Ok(())
}

/// The wheel over the pane, in lines: negative up into the history.
#[tauri::command]
pub async fn scroll(shared: State<'_, Arc<Shared>>, lines: i32) -> Answer {
    shared.act(|s| s.scroll(lines));
    Ok(())
}

/// The paste key: the clipboard is read here, not by the page, so a paste
/// needs no permission from the webview.
#[tauri::command]
pub async fn paste_clipboard(shared: State<'_, Arc<Shared>>) -> Answer {
    match clipboard::read() {
        Some(text) => shared.act(|s| s.paste(text)),
        None => shared.act(|s| s.note("nothing on the clipboard to paste")),
    }
    Ok(())
}

/// A drag across the pane, `from` and `to` as `[row, col]`: the cells
/// between them go on the clipboard.
#[tauri::command]
pub async fn copy(shared: State<'_, Arc<Shared>>, from: (u16, u16), to: (u16, u16)) -> Answer {
    let text = shared.read(|s| s.selected_text(from, to));
    if text.is_empty() {
        return Ok(());
    }
    if let Err(e) = clipboard::write(&text) {
        shared.act(|s| s.note(format!("could not copy: {e}")));
    }
    Ok(())
}

#[tauri::command]
pub async fn spawn_shell(shared: State<'_, Arc<Shared>>, checkout: u64) -> Answer {
    shared.act(|s| s.spawn_shell(CheckoutId(checkout)));
    Ok(())
}

#[tauri::command]
pub async fn focus(shared: State<'_, Arc<Shared>>, focused: bool) -> Answer {
    shared.act(|s| s.focus(focused));
    Ok(())
}

#[tauri::command]
pub async fn select_project(shared: State<'_, Arc<Shared>>, project: u64) -> Answer {
    shared.act(|s| s.select_project(ProjectId(project)));
    Ok(())
}

#[tauri::command]
pub async fn toggle_repository(shared: State<'_, Arc<Shared>>, repository: u64) -> Answer {
    shared.act(|s| s.toggle_repository(RepositoryId(repository)));
    Ok(())
}

#[tauri::command]
pub async fn choose_repository(shared: State<'_, Arc<Shared>>, repository: u64) -> Answer {
    shared.act(|s| s.choose_repository(RepositoryId(repository)));
    Ok(())
}

/// `open` true shows the Feature stage, false goes back to the Workspace.
#[tauri::command]
pub async fn show_features(shared: State<'_, Arc<Shared>>, open: bool) -> Answer {
    shared.act(|s| s.show_features(open));
    Ok(())
}

#[tauri::command]
pub async fn peek(shared: State<'_, Arc<Shared>>, slug: String) -> Answer {
    shared.act(|s| s.peek(&slug));
    Ok(())
}

#[tauri::command]
pub async fn set_layout(shared: State<'_, Arc<Shared>>, layout: String) -> Answer {
    let layout = Layout::from_word(&layout).ok_or_else(|| format!("no layout called {layout}"))?;
    shared.act(|s| s.set_layout(layout));
    Ok(())
}

#[tauri::command]
pub async fn toggle_folded(shared: State<'_, Arc<Shared>>) -> Answer {
    shared.act(|s| s.toggle_folded());
    Ok(())
}

/// Holds a feature for `reason`, or lifts its hold when there is none.
#[tauri::command]
pub async fn hold(shared: State<'_, Arc<Shared>>, slug: String, reason: Option<String>) -> Answer {
    shared.act(|s| s.hold(&slug, reason));
    Ok(())
}

/// Archives a feature, accepting it, or reopens it with `archived` false.
#[tauri::command]
pub async fn archive(shared: State<'_, Arc<Shared>>, slug: String, archived: bool) -> Answer {
    shared.act(|s| s.archive(&slug, archived));
    Ok(())
}

#[tauri::command]
pub async fn go_to_pane(shared: State<'_, Arc<Shared>>, pane: u64) -> Answer {
    shared.act(|s| s.go_to_pane(PaneId(pane)));
    Ok(())
}

#[tauri::command]
pub async fn reconnect(shared: State<'_, Arc<Shared>>) -> Answer {
    shared.ask_reconnect();
    Ok(())
}
