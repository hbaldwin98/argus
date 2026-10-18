//! Taking clients off the listener, each onto a connection task of its
//! own, until the daemon is asked to stop.

use super::ShutdownReason;
use crate::state::Daemon;

/// The shortest and longest a failed accept waits before trying again.
const ACCEPT_BACKOFF_MIN: std::time::Duration = std::time::Duration::from_millis(50);
const ACCEPT_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_secs(2);

/// Takes clients until the process is asked to stop.
///
/// A failed accept used to end the process, and with it every pty the
/// daemon was running — a transient `EMFILE`, or a pipe instance briefly
/// busy because a hook was connecting at the same moment, cost the operator
/// every agent they had open, silently: the daemon's stderr is the null
/// device, so nothing was written anywhere. Nothing an accept can report is
/// worth that. A daemon that cannot take new clients is still running the
/// ones it has, so the error is logged and backed off instead, and the loop
/// keeps going.
///
/// `shutdown_rx` is subscribed by the caller, so a stop sent before this
/// loop first runs, as one spawned on a task may, still ends it.
pub async fn serve(
    listener: &mut argus_protocol::connection::transport::Listener,
    daemon: &std::sync::Arc<Daemon>,
    shutdown: tokio::sync::broadcast::Sender<ShutdownReason>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<ShutdownReason>,
) {
    let mut connections = tokio::task::JoinSet::new();
    let mut backoff = ACCEPT_BACKOFF_MIN;
    loop {
        // Subscribe before accepting so a stop arriving between `accept`
        // and spawning its handler cannot strand a connection outside the
        // shutdown broadcast.
        let connection_shutdown = shutdown.subscribe();
        let accepted = tokio::select! {
            biased;
            _ = shutdown_rx.recv() => break,
            result = connections.join_next(), if !connections.is_empty() => {
                if let Some(Err(error)) = result {
                    tracing::warn!("client connection task failed: {error}");
                }
                continue;
            }
            result = listener.accept() => result,
        };
        match accepted {
            Ok(stream) => {
                backoff = ACCEPT_BACKOFF_MIN;
                let daemon = daemon.clone();
                let shutdown = shutdown.clone();
                connections.spawn(async move {
                    super::handle(stream, daemon, shutdown, connection_shutdown).await;
                });
            }
            Err(e) => {
                tracing::warn!("could not accept a client: {e}; retrying in {backoff:?}");
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = next_backoff(backoff);
            }
        }
    }

    while let Some(result) = connections.join_next().await {
        if let Err(error) = result {
            tracing::warn!("client connection task failed during shutdown: {error}");
        }
    }
}

/// The wait after one more failed accept: twice the last, up to the most.
fn next_backoff(backoff: std::time::Duration) -> std::time::Duration {
    (backoff * 2).min(ACCEPT_BACKOFF_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bounds are the ones `ACCEPT_BACKOFF_MIN` and `ACCEPT_BACKOFF_MAX`
    /// document: the shortest and longest a failed accept waits.
    #[test]
    fn a_run_of_failed_accepts_waits_within_the_bounds_and_reaches_the_longest() {
        let mut backoff = ACCEPT_BACKOFF_MIN;
        for _ in 0..16 {
            backoff = next_backoff(backoff);
            assert!((ACCEPT_BACKOFF_MIN..=ACCEPT_BACKOFF_MAX).contains(&backoff), "{backoff:?}");
        }
        assert_eq!(backoff, ACCEPT_BACKOFF_MAX);
    }
}
