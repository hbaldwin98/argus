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
use crate::keymap::Action;
use crate::keys::PageKey;
use crate::page::{Event, PageMouse};
use crate::session::ReviewClick;
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
    if shared.read(|s| s.pastes(&key)) {
        return paste_clipboard(shared).await;
    }
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

/// The wheel over the pane, in notches (negative up), at the `[row, col]`
/// under the pointer.
#[tauri::command]
pub async fn wheel(shared: State<'_, Arc<Shared>>, notches: i32, at: (u16, u16)) -> Answer {
    shared.act(|s| s.wheel(notches, at));
    Ok(())
}

/// A mouse gesture over the pane the page did not keep for a selection,
/// at the `[row, col]` under the pointer.
#[tauri::command]
pub async fn mouse(shared: State<'_, Arc<Shared>>, gesture: PageMouse, button: u8, at: (u16, u16)) -> Answer {
    shared.act(|s| s.mouse(gesture, button, at));
    Ok(())
}

/// The paste key: the clipboard is read here, not by the page, so a paste
/// needs no permission from the webview.
#[tauri::command]
pub async fn paste_clipboard(shared: State<'_, Arc<Shared>>) -> Answer {
    // A picture first, as the terminal client does: a screenshot copied
    // from some apps carries a text form too, and the picture is the point.
    if let Some(png) = clipboard::read_image() {
        shared.act(|s| s.paste_file("clipboard.png".into(), png));
        return Ok(());
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
        return Ok(());
    }
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

/// Starts an agent from the daemon's `template` on a checkout.
#[tauri::command]
pub async fn spawn_agent(shared: State<'_, Arc<Shared>>, checkout: u64, template: String) -> Answer {
    shared.act(|s| s.spawn_agent(CheckoutId(checkout), template));
    Ok(())
}

#[tauri::command]
pub async fn spawn_shell(shared: State<'_, Arc<Shared>>, checkout: u64) -> Answer {
    shared.act(|s| s.spawn_shell(CheckoutId(checkout)));
    Ok(())
}

#[tauri::command]
pub async fn close(shared: State<'_, Arc<Shared>>, pane: u64) -> Answer {
    shared.act(|s| s.close(PaneId(pane)));
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

/// `open` true shows the Checkouts stage, false goes back to the Workspace.
#[tauri::command]
pub async fn show_checkouts(shared: State<'_, Arc<Shared>>, open: bool) -> Answer {
    shared.act(|s| s.show_checkouts(open));
    Ok(())
}

/// A row of the Checkouts table chosen by its key.
#[tauri::command]
pub async fn select_checkout_row(shared: State<'_, Arc<Shared>>, key: String) -> Answer {
    shared.act(|s| s.select_checkout_row(key));
    Ok(())
}

#[tauri::command]
pub async fn filter_checkouts(shared: State<'_, Arc<Shared>>, filter: String) -> Answer {
    shared.act(|s| s.filter_checkouts(filter));
    Ok(())
}

#[tauri::command]
pub async fn open_review(shared: State<'_, Arc<Shared>>, checkout: u64) -> Answer {
    shared.act(|s| s.open_review(CheckoutId(checkout)));
    Ok(())
}

/// A key or button of the review dialog.
#[tauri::command]
pub async fn review_action(shared: State<'_, Arc<Shared>>, action: Action) -> Answer {
    shared.act(|s| s.review_action(action));
    Ok(())
}

#[tauri::command]
pub async fn review_click(shared: State<'_, Arc<Shared>>, target: ReviewClick) -> Answer {
    shared.act(|s| s.review_click(target));
    Ok(())
}

#[tauri::command]
pub async fn open_history(shared: State<'_, Arc<Shared>>, checkout: u64) -> Answer {
    shared.act(|s| s.open_history(CheckoutId(checkout)));
    Ok(())
}

/// A key of the history dialog, or with `row` a click on a row.
#[tauri::command]
pub async fn history_action(shared: State<'_, Arc<Shared>>, action: Action, row: Option<usize>) -> Answer {
    shared.act(|s| s.history_action(action, row));
    Ok(())
}

#[tauri::command]
pub async fn review_refresh(shared: State<'_, Arc<Shared>>) -> Answer {
    shared.act(|s| s.review_refresh());
    Ok(())
}

/// Sends the comment, or with no `body` puts the box away.
#[tauri::command]
pub async fn review_comment(shared: State<'_, Arc<Shared>>, body: Option<String>) -> Answer {
    shared.act(|s| s.review_comment(body));
    Ok(())
}

/// Asks whether to remove a worktree picked in the rail.
#[tauri::command]
pub async fn remove_checkout(shared: State<'_, Arc<Shared>>, checkout: u64) -> Answer {
    shared.act(|s| s.remove_checkout(CheckoutId(checkout)));
    Ok(())
}

/// The answer to the Checkouts stage's question.
#[tauri::command]
pub async fn answer_checkouts(shared: State<'_, Arc<Shared>>, yes: bool) -> Answer {
    shared.act(|s| s.answer_checkouts(yes));
    Ok(())
}

/// A worktree for the selected row, on the new branch `name` for a checkout.
#[tauri::command]
pub async fn new_worktree(shared: State<'_, Arc<Shared>>, name: Option<String>) -> Answer {
    shared.act(|s| s.new_worktree(name));
    Ok(())
}

/// A key or button of the Checkouts stage, acting on its selected row.
#[tauri::command]
pub async fn checkouts_action(shared: State<'_, Arc<Shared>>, action: Action) -> Answer {
    shared.act(|s| s.checkouts_action(action));
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

/// The command palette opened, or with `open` false closed.
#[tauri::command]
pub async fn palette(shared: State<'_, Arc<Shared>>, open: bool) -> Answer {
    shared.act(|s| s.palette(open));
    Ok(())
}

/// A command chosen in the palette.
#[tauri::command]
pub async fn command(shared: State<'_, Arc<Shared>>, action: Action) -> Answer {
    shared.act(|s| s.command(action));
    Ok(())
}

#[tauri::command]
pub async fn go_to_checkout(shared: State<'_, Arc<Shared>>, checkout: u64) -> Answer {
    shared.act(|s| s.go_to_checkout(CheckoutId(checkout)));
    Ok(())
}

#[tauri::command]
pub async fn go_to_feature(shared: State<'_, Arc<Shared>>, slug: String) -> Answer {
    shared.act(|s| s.go_to_feature(&slug));
    Ok(())
}

#[tauri::command]
pub async fn reconnect(shared: State<'_, Arc<Shared>>) -> Answer {
    shared.ask_reconnect();
    Ok(())
}
