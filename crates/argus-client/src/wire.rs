//! The two tasks between the app and the daemon's socket, and the one
//! thing that happens in between: collapsing a frame's worth of
//! subscription churn into the subscriptions that actually changed.

use std::collections::HashSet;

use argus_protocol::{read_msg, write_msg, ClientMsg, PaneId, ServerMsg};
use tokio::io::split;
use tokio::sync::mpsc;

use crate::FRAME_INTERVAL;

/// How many server messages may queue for the app before the reader
/// task blocks — backpressure onto the socket rather than unbounded growth.
const SERVER_QUEUE_MESSAGES: usize = 256;

pub fn connection_channels<S>(
    stream: S,
) -> (mpsc::UnboundedSender<ClientMsg>, mpsc::Receiver<ServerMsg>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut rd, wr) = split(stream);
    let (in_tx, in_rx) = mpsc::unbounded_channel::<ClientMsg>();
    let (out_tx, out_rx) = mpsc::channel::<ServerMsg>(SERVER_QUEUE_MESSAGES);

    tokio::spawn(client_writer(wr, in_rx));
    tokio::spawn(async move {
        while let Ok(msg) = read_msg::<_, ServerMsg>(&mut rd).await {
            if out_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    (in_tx, out_rx)
}

async fn client_writer<W>(mut wr: W, mut rx: mpsc::UnboundedReceiver<ClientMsg>)
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut subscribed = HashSet::new();
    while let Some(first) = rx.recv().await {
        let subscription_change = is_subscription_change(&first);
        let mut batch = vec![first];
        if subscription_change {
            // Selection can change many times inside one rendered frame. Let
            // those changes settle before asking the daemon for full grids.
            tokio::time::sleep(FRAME_INTERVAL).await;
            batch.extend(std::iter::from_fn(|| rx.try_recv().ok()));
        }
        for msg in compact_subscriptions(batch, &mut subscribed) {
            if write_msg(&mut wr, &msg).await.is_err() {
                return;
            }
        }
    }
}

fn is_subscription_change(msg: &ClientMsg) -> bool {
    matches!(
        msg,
        ClientMsg::Subscribe { .. } | ClientMsg::Unsubscribe { .. }
    )
}

fn compact_subscriptions(
    mut batch: Vec<ClientMsg>,
    subscribed: &mut HashSet<PaneId>,
) -> Vec<ClientMsg> {
    // Letting a pane go drops the client's cached grid for it, and only the
    // snapshot a Subscribe brings back can rebuild one: incremental damage
    // has no rows to land on. So a pane this batch let go of and then asked
    // for again still needs its Subscribe on the wire, even though the
    // daemon never stopped streaming it and the message looks redundant —
    // dropping it leaves the column permanently blank.
    let regrid: HashSet<PaneId> = batch
        .iter()
        .filter_map(|msg| match msg {
            ClientMsg::Unsubscribe { pane } => Some(*pane),
            _ => None,
        })
        .collect();

    let mut seen = HashSet::new();
    batch.reverse();
    batch.retain(|msg| match msg {
        ClientMsg::Subscribe { pane } | ClientMsg::Unsubscribe { pane } => seen.insert(*pane),
        _ => true,
    });
    batch.reverse();
    batch.retain(|msg| match msg {
        ClientMsg::Subscribe { pane } => subscribed.insert(*pane) || regrid.contains(pane),
        ClientMsg::Unsubscribe { pane } => subscribed.remove(pane),
        _ => true,
    });
    batch
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn rapid_pane_swaps_cost_one_message_however_many_panes_were_crossed() {
        // Every selection the user passes through drops one grid and asks
        // for another, so a held key can queue hundreds of subscription
        // changes inside a single frame. Only the settled selection is
        // worth a full grid — but it is worth exactly one, because the
        // client threw its own copy away on the way past.
        let (client, mut daemon) = tokio::io::duplex(1024 * 1024);
        let (tx, _server_rx) = connection_channels(client);
        let pane_a = PaneId(1);
        let pane_b = PaneId(2);

        tx.send(ClientMsg::Subscribe { pane: pane_a }).unwrap();
        assert!(matches!(
            read_msg::<_, ClientMsg>(&mut daemon).await.unwrap(),
            ClientMsg::Subscribe { pane } if pane == pane_a
        ));

        for _ in 0..100 {
            tx.send(ClientMsg::Unsubscribe { pane: pane_a }).unwrap();
            tx.send(ClientMsg::Subscribe { pane: pane_b }).unwrap();
            tx.send(ClientMsg::Unsubscribe { pane: pane_b }).unwrap();
            tx.send(ClientMsg::Subscribe { pane: pane_a }).unwrap();
        }

        let settled = timeout(
            Duration::from_secs(5),
            read_msg::<_, ClientMsg>(&mut daemon),
        )
        .await
        .expect("the settled selection must be re-sent, or its column stays blank")
        .unwrap();
        assert!(
            matches!(settled, ClientMsg::Subscribe { pane } if pane == pane_a),
            "{settled:?}"
        );
        assert!(
            timeout(
                Duration::from_millis(50),
                read_msg::<_, ClientMsg>(&mut daemon)
            )
            .await
            .is_err(),
            "intermediate pane selections were written to the daemon"
        );
    }
    #[test]
    fn a_pane_let_go_of_and_taken_back_in_one_batch_is_still_re_subscribed() {
        // Regression: compaction used to see that the daemon was already
        // streaming pane A and drop the Subscribe as redundant. It was not
        // redundant — the app had dropped A's grid on the way out, and with
        // no snapshot to replace it every later damage span landed on a
        // grid with no rows. The pane rendered blank until something else
        // forced a resize.
        let pane_a = PaneId(1);
        let pane_b = PaneId(2);
        let mut subscribed = HashSet::from([pane_a]);
        let batch = vec![
            ClientMsg::Unsubscribe { pane: pane_a },
            ClientMsg::Subscribe { pane: pane_b },
            ClientMsg::Unsubscribe { pane: pane_b },
            ClientMsg::Subscribe { pane: pane_a },
        ];

        let compacted = compact_subscriptions(batch, &mut subscribed);

        assert!(
            matches!(compacted.as_slice(), [ClientMsg::Subscribe { pane }] if *pane == pane_a),
            "{compacted:?}"
        );
        assert_eq!(subscribed, HashSet::from([pane_a]));
    }
    #[test]
    fn subscription_compaction_keeps_the_final_selection() {
        let pane_a = PaneId(1);
        let pane_b = PaneId(2);
        let mut subscribed = HashSet::from([pane_a]);
        let batch = vec![
            ClientMsg::Unsubscribe { pane: pane_a },
            ClientMsg::Subscribe { pane: pane_b },
            ClientMsg::Unsubscribe { pane: pane_b },
            ClientMsg::Subscribe { pane: pane_a },
            ClientMsg::Unsubscribe { pane: pane_a },
            ClientMsg::Subscribe { pane: pane_b },
        ];

        let compacted = compact_subscriptions(batch, &mut subscribed);

        assert!(matches!(
            compacted.as_slice(),
            [
                ClientMsg::Unsubscribe { pane: first },
                ClientMsg::Subscribe { pane: second }
            ] if *first == pane_a && *second == pane_b
        ));
        assert_eq!(subscribed, HashSet::from([pane_b]));
    }
}
