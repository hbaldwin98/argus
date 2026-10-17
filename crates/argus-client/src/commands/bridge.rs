//! This machine's daemon on stdin and stdout, so an argus on another
//! machine can reach it over ssh: `ssh <host> argus bridge`.
//!
//! The bridge knows nothing of the protocol. It carries bytes both ways
//! until either side closes, and writes nothing of its own to stdout —
//! every byte there is the daemon's.

use tokio::io::{AsyncRead, AsyncWrite};

/// `argus bridge`: connects to this machine's daemon, starting it if
/// nothing is listening, and carries it on stdin and stdout.
///
/// Exits the process the moment `pipe` returns rather than returning to
/// `main`: `tokio::io::stdin()` reads on a blocking thread the runtime
/// cannot cancel, so when the daemon goes away first, dropping the runtime
/// waits on a read of stdin that may never end, and the bridge outlives
/// its daemon until the far side happens to type or hang up.
pub async fn run() -> anyhow::Result<()> {
    let daemon = crate::connection::launch::ensure_daemon_and_connect().await?;
    let carried = pipe(daemon, tokio::io::stdin(), tokio::io::stdout()).await;
    if let Err(err) = carried {
        // The same line `main` would print for an error it returned.
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
    std::process::exit(0);
}

/// Carries `input` to the daemon and the daemon to `output` until either
/// side closes. Either direction ending ends both: a client that hung up
/// has nobody to read the daemon, and a daemon that went away has nothing
/// more to say.
async fn pipe<S, R, W>(daemon: S, mut input: R, mut output: W) -> anyhow::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (mut from_daemon, mut to_daemon) = tokio::io::split(daemon);
    tokio::select! {
        sent = tokio::io::copy(&mut input, &mut to_daemon) => { sent?; }
        received = tokio::io::copy(&mut from_daemon, &mut output) => { received?; }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{read_msg, write_msg, ClientMsg, Hello, ServerMsg};
    use tokio::io::duplex;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn messages_cross_both_ways_until_the_client_hangs_up() {
        let (daemon_end, mut daemon) = duplex(64 * 1024);
        let (mut client_in, input) = duplex(64 * 1024);
        let (output, mut client_out) = duplex(64 * 1024);
        let bridge = tokio::spawn(pipe(daemon_end, input, output));

        write_msg(&mut daemon, &ServerMsg::Tree(Vec::new()))
            .await
            .unwrap();
        let tree: ServerMsg = read_msg(&mut client_out).await.unwrap();
        assert!(matches!(tree, ServerMsg::Tree(_)), "{tree:?}");

        write_msg(&mut client_in, &ClientMsg::Hello(Hello::this_build()))
            .await
            .unwrap();
        let hello: ClientMsg = read_msg(&mut daemon).await.unwrap();
        assert!(matches!(hello, ClientMsg::Hello(_)), "{hello:?}");

        drop(client_in);
        timeout(Duration::from_secs(5), bridge)
            .await
            .expect("the bridge should end with its client")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn the_bridge_ends_when_the_daemon_goes_away() {
        let (daemon_end, daemon) = duplex(1024);
        let (_client_in, input) = duplex(1024);
        let (output, _client_out) = duplex(1024);
        let bridge = tokio::spawn(pipe(daemon_end, input, output));

        drop(daemon);
        timeout(Duration::from_secs(5), bridge)
            .await
            .expect("the bridge should end with its daemon")
            .unwrap()
            .unwrap();
    }
}
