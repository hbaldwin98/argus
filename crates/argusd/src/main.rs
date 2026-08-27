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

    let serve = async {
        loop {
            let stream = listener.accept().await?;
            let daemon = daemon.clone();
            tokio::spawn(async move {
                conn::handle(stream, daemon).await;
            });
        }
        // Unreachable, but it types the block as fallible.
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    // On a clean shutdown, take the managed hooks back out rather than
    // leaving them pointing at a port this process is about to release.
    // A hard kill still leaves them behind — that's what the startup sweep
    // above is for.
    tokio::select! {
        r = serve => r?,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutting down");
        }
    }
    daemon.sweep_stale_hooks();
    Ok(())
}
