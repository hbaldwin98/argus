//! Daemon startup: build the `Daemon`, start the pollers, watchers and
//! hook receiver, then accept clients until the process is asked to stop.

mod browser;
mod code;
mod config;
mod connection;
mod files;
mod git;
mod harness;
mod hook;
mod pty;
mod review;
mod state;
mod store;
mod syntax;
mod system;
#[cfg(test)]
mod test_host;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    system::logging::init();
    tracing::info!(
        "argusd starting; logging to {}",
        system::logging::log_path().display()
    );
    // Before any pane can load ConPTY by name.
    #[cfg(windows)]
    if let Err(e) = system::dll_search::restrict() {
        tracing::error!("could not keep library loads out of the working directory: {e}");
    }
    // Before any pane can inherit the daemon's own deafness to it.
    #[cfg(windows)]
    if let Err(e) = system::ctrl_c::restore() {
        tracing::error!("could not restore Ctrl-C for panes: {e}");
    }

    let Some(_daemon_lock) = system::daemon_lock::DaemonLock::acquire()? else {
        tracing::info!("another argusd already owns this instance; exiting");
        return Ok(());
    };

    // Bind before loading or restoring panes. If a daemon from an older
    // version still owns the endpoint, this process must exit before it can
    // launch a duplicate copy of every recorded agent.
    let mut listener = argus_protocol::connection::transport::Listener::bind().await?;

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
    // Before any pane restores: a Claude pane is started with its mod.
    daemon.set_plugin_root(argus_protocol::config_dir().join("plugins"));
    daemon.set_paste_root(argus_protocol::config_dir().join("pastes"));
    daemon.start_git_poll();
    daemon.start_git_watch();
    daemon.start_edit_watch();
    daemon.start_config_watch();
    daemon.start_project_scan();
    // After the hook server, so a restored agent gets working hooks.
    daemon.restore_session();
    // After the hook server too: each extension is handed its port.
    // Before any browser starts: what a daemon of this instance killed
    // outright left behind.
    browser::sweep();
    let extensions = daemon.start_extensions(config::load_extensions(), argus_protocol::config_dir());

    tracing::info!("argusd listening");

    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
    let serve = connection::accept::serve(&mut listener, &daemon, shutdown_tx.clone(), shutdown_tx.subscribe());

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
    extensions.stop().await;
    daemon.close_browsers();
    daemon.sweep_stale_hooks();
    Ok(())
}
