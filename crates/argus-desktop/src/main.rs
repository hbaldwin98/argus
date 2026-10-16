//! The Argus desktop window: a second view of the same daemon the terminal
//! client shows, drawn in a webview.
//!
//! Startup only: a runtime of our own, handed to Tauri before anything
//! else, the window, and running until it closes. The model is `session`,
//! the daemon is reached in `daemon`, and the page in `assets/` draws what
//! it is sent.
//!
//! On Windows the release binary is a GUI program, so starting it opens no
//! console window beside it; tests keep the console subsystem so their
//! output is still seen.
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod add_project;
mod agent_views;
mod agents;
mod backdrop;
mod capture_note;
mod clipboard;
mod checkouts;
mod checkouts_stage;
mod code_stage;
mod code_tree;
mod comment_box;
mod commands;
mod console;
mod daemon;
mod dialogs;
mod docks;
mod dragged_width;
mod extension_dialog;
mod extension_keys;
mod extension_views;
mod feature_edits;
mod feature_stage;
mod feature_view;
mod features;
mod file_jump;
mod flows_lens;
mod fonts;
#[cfg(test)]
mod fixtures;
mod keymap;
mod history;
mod keys;
mod link_dialog;
mod logging;
mod notifications;
mod opener;
mod page;
mod stage;
mod palette;
mod preferences;
mod reconcile_dialog;
mod record_lens;
mod rail;
mod review;
mod saved_views;
mod screen;
mod sent;
mod sequence;
mod session;
mod sessions;
mod sessions_dialog;
mod settings;
mod shown;
mod status;
mod switcher;
mod shared;
mod tracker_dialogs;
mod transcript_reader;
mod view_changes;
mod view_refs;
mod window_frame;
mod work_lens;

use std::sync::Arc;
use std::time::Duration;

use tauri::webview::PageLoadEvent;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use shared::Shared;

/// How often the shown pane's changed rows are sent: about a frame.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

fn main() {
    logging::init();
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            tracing::error!("could not start the runtime: {err}");
            std::process::exit(1);
        }
    };
    tauri::async_runtime::set(runtime.handle().clone());

    let mut saved = settings::load();
    // Only where none was chosen: the person's own choice outlasts the
    // environment's.
    if saved.host.is_none() {
        saved.host = std::env::var("ARGUS_DESKTOP_HOST").ok().filter(|host| !host.trim().is_empty());
    }
    let shared = Arc::new(Shared::new(saved));
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![commands::connect, commands::call])
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                webview.state::<Arc<Shared>>().detach_page();
            }
        })
        .setup(move |app| {
            shared.set_app(app.handle().clone());
            open_window(app, &shared)?;
            tauri::async_runtime::spawn(daemon::run(shared.clone()));
            let found = shared.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let fonts = fonts::installed();
                found.act(|session| session.found_fonts(fonts));
            });
            let configured = shared.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let hosts = argus_protocol::ssh_hosts::hosts();
                configured.act(|session| {
                    session.found_hosts(hosts);
                    Vec::new()
                });
            });
            tauri::async_runtime::spawn(frames(shared));
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match built {
        Ok(app) => app,
        Err(err) => {
            tracing::error!("could not open the window: {err}");
            std::process::exit(1);
        }
    };

    let code = app.run_return(|_, _| {});
    // Dropping the tasks drops the bridge, which is killed with them.
    runtime.shutdown_timeout(Duration::from_secs(1));
    std::process::exit(code);
}

fn open_window(app: &mut tauri::App, shared: &Arc<Shared>) -> tauri::Result<()> {
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()));
    let window = window_frame::frameless(builder)
        .title("Argus")
        .inner_size(1200.0, 800.0)
        .min_inner_size(720.0, 480.0)
        // Per instance, so an `ARGUS_INSTANCE=dev` window never shares
        // webview storage with the installed one. macOS ignores it.
        .data_directory(argus_protocol::config_dir().join("desktop"))
        // Tauri's own handler would swallow drops the page will want.
        .disable_drag_drop_handler()
        // Always, since it can only be set as the window is made: the
        // background opacity is then the page's alpha alone, and moving
        // its slider never rebuilds the window (`backdrop`).
        .transparent(true)
        .build()?;
    commands::dress(&window, shared);
    if let Err(err) = commands::scale(&window, shared) {
        tracing::warn!("could not zoom the window: {err}");
    }
    let painted = window.clone();
    let shared = shared.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::ThemeChanged(_) = event {
            commands::dress(&painted, &shared);
        }
    });
    Ok(())
}

async fn frames(shared: Arc<Shared>) {
    let mut tick = tokio::time::interval(FRAME_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        shared.tick();
    }
}
