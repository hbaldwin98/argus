//! Daemon startup: build the `Daemon`, start the pollers, watchers and
//! hook receiver, then accept clients until the process is asked to stop.

mod browse;
mod command;
mod config;
mod conn;
mod diff;
mod editor;
mod git;
mod harness;
mod highlight;
mod logging;
mod paths;
mod pty;
mod state;
mod store;
mod watch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    tracing::info!(
        "argusd starting; logging to {}",
        logging::log_path().display()
    );

    let cfg = config::load()?;
    // A store that will not open costs this run its memory, not its
    // startup: running without persistence beats refusing to run, and an
    // in-memory store also cannot overwrite whatever made the real one
    // unreadable.
    let store = store::Store::open().unwrap_or_else(|e| {
        tracing::error!("could not open the runtime store: {e}; nothing will be remembered");
        store::Store::in_memory().expect("an in-memory store needs nothing that can fail")
    });
    let daemon = state::Daemon::with_store(cfg, store);
    // Before anything can write new ones: any managed hooks still on disk
    // name a previous daemon's port and are stale by definition.
    daemon.sweep_stale_hooks();
    daemon.start_hook_server()?;
    daemon.start_git_poll();
    daemon.start_git_watch();
    daemon.start_config_watch();
    daemon.start_project_scan();
    // After the hook server, so a restored agent gets working hooks.
    daemon.restore_session();

    let mut listener = argus_protocol::transport::Listener::bind().await?;
    tracing::info!("argusd listening");

    let serve = serve(&mut listener, &daemon);

    // On a clean shutdown, take the managed hooks back out rather than
    // leaving them pointing at a port this process is about to release.
    // A hard kill still leaves them behind — that's what the startup sweep
    // above is for.
    tokio::select! {
        _ = serve => {}
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutting down");
        }
    }
    daemon.sweep_stale_hooks();
    Ok(())
}

/// The shortest and longest a failed accept waits before trying again.
const ACCEPT_BACKOFF_MIN: std::time::Duration = std::time::Duration::from_millis(50);
const ACCEPT_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_secs(2);

/// Takes clients until the process is asked to stop. Never returns.
///
/// A failed accept used to end the process, and with it every pty the
/// daemon was running — a transient `EMFILE`, or a pipe instance briefly
/// busy because a hook was connecting at the same moment, cost the operator
/// every agent they had open, silently: the daemon's stderr is the null
/// device, so nothing was written anywhere. Nothing an accept can report is
/// worth that. A daemon that cannot take new clients is still running the
/// ones it has, so the error is logged and backed off instead, and the loop
/// keeps going.
async fn serve(
    listener: &mut argus_protocol::transport::Listener,
    daemon: &std::sync::Arc<state::Daemon>,
) -> ! {
    let mut backoff = ACCEPT_BACKOFF_MIN;
    loop {
        match listener.accept().await {
            Ok(stream) => {
                backoff = ACCEPT_BACKOFF_MIN;
                let daemon = daemon.clone();
                tokio::spawn(async move {
                    conn::handle(stream, daemon).await;
                });
            }
            Err(e) => {
                tracing::warn!("could not accept a client: {e}; retrying in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(ACCEPT_BACKOFF_MAX);
            }
        }
    }
}
