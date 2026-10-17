//! `argus focus`: where the person is working, printed as JSON for a
//! script or a status line — once, or with `--watch` a line per change.
//!
//! It never starts a daemon. A status line runs it every few seconds, and
//! one that started argusd whenever nothing was listening would keep a
//! daemon nobody asked for alive. Watching outlives the daemon instead: a
//! daemon going away is printed as `null`, since nobody is anywhere, and
//! the watch connects again once one is back.

use std::time::Duration;

use anyhow::Context;
use argus_protocol::{connection::transport, ClientMsg, Focus, ServerMsg, FOCUS, GREETING_WAIT};

use crate::connection::launch::{greet, Connected, Greeted};

/// How long a watch waits between looks for a daemon that has gone.
const RETRY: Duration = Duration::from_secs(2);

pub async fn run(watch: bool) -> anyhow::Result<()> {
    if !watch {
        let (_out, mut incoming) = watch_focus().await?;
        let focus = next_focus(&mut incoming)
            .await
            .context("argusd closed the connection before saying")?;
        return print(&focus);
    }

    let mut printed = None;
    loop {
        if let Ok((_out, mut incoming)) = watch_focus().await {
            while let Some(focus) = next_focus(&mut incoming).await {
                print_changed(&mut printed, focus)?;
            }
        }
        print_changed(&mut printed, None)?;
        tokio::time::sleep(RETRY).await;
    }
}

/// A connection to the daemon that has been asked to say where the person
/// is. The sending half is the connection too: dropping it hangs up.
async fn watch_focus() -> anyhow::Result<crate::Connection> {
    if !transport::is_daemon_listening() {
        anyhow::bail!("argusd is not running");
    }
    let stream = transport::connect()
        .await
        .context("could not connect to argusd")?;
    let channels = crate::connection::wire::connection_channels(stream);
    let Greeted::Up(Connected {
        channels: (out, incoming),
        daemon,
        ..
    }) = greet(channels, GREETING_WAIT).await
    else {
        anyhow::bail!("argusd closed the connection before saying anything");
    };
    if !daemon.is_some_and(|d| d.can(FOCUS)) {
        anyhow::bail!("argusd predates `argus focus`; `argus server restart` to update it");
    }
    out.send(ClientMsg::WatchFocus)
        .context("argusd closed the connection")?;
    Ok((out, incoming))
}

/// The next focus the daemon says, or `None` once it has gone.
async fn next_focus(
    incoming: &mut tokio::sync::mpsc::Receiver<ServerMsg>,
) -> Option<Option<Focus>> {
    while let Some(msg) = incoming.recv().await {
        if let ServerMsg::Focus(focus) = msg {
            return Some(focus.map(|focus| *focus));
        }
    }
    None
}

/// Prints `focus` unless it is what was printed last. The daemon already
/// sends only changes; this covers a reconnect, which is answered with
/// where the person already was.
fn print_changed(printed: &mut Option<Option<Focus>>, focus: Option<Focus>) -> anyhow::Result<()> {
    if printed.as_ref() == Some(&focus) {
        return Ok(());
    }
    print(&focus)?;
    *printed = Some(focus);
    Ok(())
}

fn print(focus: &Option<Focus>) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string(focus)?);
    Ok(())
}
