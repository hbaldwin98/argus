mod config;
mod conn;
mod git;
mod hooks;
mod pty;
mod state;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .init();

    let cfg = config::load()?;
    let daemon = state::Daemon::new(cfg);
    daemon.start_hook_server()?;
    daemon.start_git_poll();

    let mut listener = orion_protocol::transport::Listener::bind().await?;
    tracing::info!("oriond listening");

    loop {
        let stream = listener.accept().await?;
        let daemon = daemon.clone();
        tokio::spawn(async move {
            conn::handle(stream, daemon).await;
        });
    }
}
