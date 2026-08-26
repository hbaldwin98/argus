mod browse;
mod command;
mod config;
mod conn;
mod diff;
mod editor;
mod git;
mod harness;
mod logging;
mod pty;
mod session;
mod state;
mod watch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    tracing::info!("argusd starting; logging to {}", logging::log_path().display());

    let cfg = config::load()?;
    let daemon = state::Daemon::new(cfg);
    // Before anything can write new ones: any managed hooks still on disk
    // name a previous daemon's port and are stale by definition.
    daemon.sweep_stale_hooks();
    daemon.start_hook_server()?;
    daemon.start_git_poll();
    daemon.start_git_watch();
    daemon.start_config_watch();
    daemon.start_project_scan();
    // After the hook server, so a restored agent gets working hooks.
    if daemon.restore_session() {
        daemon.persist_session();
    }

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
