//! The calls the page makes, handed to the session.
//!
//! Each is `async` because Tauri runs a synchronous command on the UI
//! thread, and none of them should hold the window while it takes a lock.
//! They answer nothing: what a call changed reaches the page as events.

use std::sync::Arc;

use argus_protocol::{CheckoutId, PaneId, ProjectId, RepositoryId};
use tauri::ipc::Channel;
use tauri::State;

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
pub async fn reconnect(shared: State<'_, Arc<Shared>>) -> Answer {
    shared.ask_reconnect();
    Ok(())
}
