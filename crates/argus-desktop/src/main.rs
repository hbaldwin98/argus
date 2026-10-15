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
mod agents;
mod capture_note;
mod clipboard;
mod checkouts;
mod checkouts_stage;
mod commands;
mod daemon;
mod dialogs;
mod feature_edits;
mod feature_stage;
mod feature_view;
mod features;
#[cfg(test)]
mod fixtures;
mod keymap;
mod history;
mod keys;
mod logging;
mod page;
mod palette;
mod rail;
mod review;
mod screen;
mod sent;
mod session;
mod sessions;
mod sessions_dialog;
mod settings;
mod shown;
mod status;
mod switcher;
mod shared;
mod window_frame;

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

    let shared = Arc::new(Shared::new(settings::load()));
    let built = tauri::Builder::default()
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![commands::connect, commands::call])
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                webview.state::<Arc<Shared>>().detach_page();
            }
        })
        .setup(move |app| {
            open_window(app)?;
            tauri::async_runtime::spawn(daemon::run(shared.clone()));
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

fn open_window(app: &mut tauri::App) -> tauri::Result<()> {
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
        .build()?;
    window_frame::paint(&window);
    let painted = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::ThemeChanged(_) = event {
            window_frame::paint(&painted);
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
