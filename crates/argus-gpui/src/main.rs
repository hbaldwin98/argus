//! The Argus window on GPUI: startup only.
//!
//! The tokio runtime is built before the GPUI app, since GPUI's executors
//! are not tokio, and the daemon loop runs on it; what it hears reaches the
//! UI thread through `connection::owner`'s channel. What is here is the
//! runtime, the window, and running until it closes.
//!
//! Set `ARGUS_GPUI_QUIT_MS` to have it print what it heard after that long
//! and exit, which is how an agent runs it unattended. `ARGUS_GPUI_PROBE_SHELL`
//! shows one pane, `ARGUS_GPUI_PROBE_GIT` opens a checkout's review, reads
//! what the Git stage was sent and closes it again, and
//! `ARGUS_GPUI_PROBE_HISTORY` does the same for its history, unfolding the
//! newest commit and filtering to nothing on the way.

// A console window behind the GUI is noise outside a debug build.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod probe;
mod report;

use std::time::Duration;

use argus_desktop_core::connection::daemon;
use argus_desktop_core::settings::remembered;
use argus_desktop_core::system::logging;
use argus_gpui::chrome::shell::Shell;
use argus_gpui::connection::caller::Caller;
use argus_gpui::connection::heard::Heard;
use argus_gpui::connection::owner::Owner;
use argus_gpui::window;
use gpui_kit::{App, AppContext as _};

fn main() {
    logging::init();
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            tracing::error!("could not start the runtime: {err}");
            std::process::exit(1);
        }
    };

    let mut saved = remembered::load();
    // Only where none was chosen: the person's own choice outlasts the
    // environment's.
    if saved.host.is_none() {
        saved.host = std::env::var("ARGUS_DESKTOP_HOST").ok().filter(|host| !host.trim().is_empty());
    }
    let (owner, events) = Owner::new(saved);
    let daemon = runtime.spawn(daemon::run(owner.clone()));
    let frames = runtime.spawn(owner.clone().frames());

    let quit_after: Option<u64> = std::env::var("ARGUS_GPUI_QUIT_MS").ok().and_then(|ms| ms.parse().ok());
    let calls = owner.clone();
    window::application().run(move |cx: &mut App| {
        window::init(cx);
        // The loops are the window's: they end with it, and so does the app.
        window::quit::quit_when_closed(cx, move || {
            daemon.abort();
            frames.abort();
        })
        .detach();
        let heard = cx.new(|_| Heard::new());
        heard.update(cx, |heard, cx| heard.listen(events, cx));
        let caller = Caller::new(move |call| calls.call(call));
        let probing = caller.clone();
        let options = window::options(cx);
        let shell = heard.clone();
        gpui_kit::open_window(options, cx, move |window, cx| {
            window::quit::quit_instead_of_closing(window, cx);
            cx.new(|cx| Shell::new(shell, caller, window, cx))
        })
        .expect("the window opens");
        cx.activate(true);
        if let Some(pane) = std::env::var("ARGUS_GPUI_PROBE_SHELL").ok().and_then(|pane| pane.parse().ok()) {
            probe::shell(heard.clone(), probing.clone(), pane, cx);
        }
        if let Ok(which) = std::env::var("ARGUS_GPUI_PROBE_GIT") {
            probe::git(heard.clone(), probing.clone(), which, cx);
        }
        if let Ok(which) = std::env::var("ARGUS_GPUI_PROBE_HISTORY") {
            probe::history(heard.clone(), probing.clone(), which, cx);
        }
        if let Some(ms) = quit_after {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(ms)).await;
                cx.update(|cx| {
                    println!("{}", report::line(heard.read(cx)));
                    cx.quit();
                })
            })
            .detach();
        }
    });
    // Dropping the tasks drops the bridge, which is killed with them.
    runtime.shutdown_timeout(Duration::from_secs(1));
}
