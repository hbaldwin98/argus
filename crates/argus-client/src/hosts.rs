//! The daemons this client is attached to, and which one is on screen.
//!
//! Each host has an app of its own. A tree, a grid and a selection hold one
//! daemon's ids, which mean nothing to another daemon, so they are never
//! mixed: the host on screen is drawn and takes input, and any other keeps
//! its connection so its agents' states stay current.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use argus_protocol::{PaneId, ServerMsg};
use tokio::sync::mpsc;

use crate::app::App;
use crate::launch::Connected;

/// One daemon, and everything this client holds for it.
pub struct Host {
    pub app: App,
    pub out_rx: mpsc::Receiver<ServerMsg>,
    /// False from the moment the daemon's messages stop, which is what takes
    /// the host out of the wait: a closed receiver is ready immediately and
    /// forever, and waiting on it would spin the loop.
    pub connected: bool,
    pub pending_reconnect: Option<mpsc::Receiver<Connected>>,
    /// Keyed by pane id, not just dimensions — switching to a different pane
    /// at the same on-screen size still needs its own Resize, since each
    /// pane's pty starts at a hardcoded default until told otherwise.
    pub last_sizes: HashMap<PaneId, (u16, u16)>,
}

impl Host {
    pub fn new(app: App, out_rx: mpsc::Receiver<ServerMsg>) -> Host {
        Host {
            app,
            out_rx,
            connected: true,
            pending_reconnect: None,
            last_sizes: HashMap::new(),
        }
    }
}

/// What happened on one of the hosts, and which.
pub enum HostEvent {
    /// A message from the host's daemon, or `None` for the daemon going away.
    Message(usize, Option<ServerMsg>),
    /// The connection a host's reconnect produced, or `None` for a reconnect
    /// that stopped trying.
    Reconnected(usize, Option<Connected>),
}

pub struct Hosts {
    list: Vec<Host>,
    current: usize,
}

impl Hosts {
    pub fn new(first: Host) -> Hosts {
        let mut hosts = Hosts {
            list: vec![first],
            current: 0,
        };
        hosts.share_names();
        hosts
    }

    /// Adds a host, off screen, and says where it went.
    pub fn push(&mut self, mut host: Host) -> usize {
        host.app.set_on_screen(false);
        self.list.push(host);
        self.share_names();
        self.list.len() - 1
    }

    /// Puts a host on screen. The one leaving lets go of its panes, and the
    /// one arriving asks for its own again and claims their sizes afresh.
    pub fn show(&mut self, index: usize) {
        if index >= self.list.len() || index == self.current {
            return;
        }
        self.list[self.current].app.set_on_screen(false);
        self.current = index;
        let host = &mut self.list[index];
        host.last_sizes.clear();
        host.app.set_on_screen(true);
    }

    /// The host whose daemon is on `place`: `None` for this machine.
    pub fn find(&self, place: &Option<String>) -> Option<usize> {
        self.list.iter().position(|host| host.app.host == *place)
    }

    /// Tells every app which hosts are attached, for the host picker.
    fn share_names(&mut self) {
        let names: Vec<Option<String>> = self.list.iter().map(|h| h.app.host.clone()).collect();
        for host in &mut self.list {
            host.app.attached_hosts = names.clone();
        }
    }

    pub fn on_screen(&mut self) -> &mut Host {
        &mut self.list[self.current]
    }

    pub fn get(&mut self, index: usize) -> &mut Host {
        &mut self.list[index]
    }

    pub fn is_on_screen(&self, index: usize) -> bool {
        index == self.current
    }

    /// The next thing to happen on any host: a message from one that is
    /// connected, or a connection for one that is reconnecting. Never, while
    /// no host is either.
    ///
    /// One wait over every host rather than an arm each, because the arms of
    /// a `select!` cannot all borrow the list at once.
    pub async fn next(&mut self) -> HostEvent {
        let mut waiting: Vec<Pin<Box<dyn Future<Output = HostEvent> + '_>>> = Vec::new();
        for (index, host) in self.list.iter_mut().enumerate() {
            if host.connected {
                let messages = &mut host.out_rx;
                waiting.push(Box::pin(async move {
                    HostEvent::Message(index, messages.recv().await)
                }));
            } else if let Some(reconnect) = host.pending_reconnect.as_mut() {
                waiting.push(Box::pin(async move {
                    HostEvent::Reconnected(index, reconnect.recv().await)
                }));
            }
        }
        if waiting.is_empty() {
            return std::future::pending().await;
        }
        futures::future::select_all(waiting).await.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    fn host() -> (Host, mpsc::Sender<ServerMsg>) {
        let (in_tx, _in_rx) = mpsc::unbounded_channel();
        let (out_tx, out_rx) = mpsc::channel(8);
        (Host::new(App::new(in_tx), out_rx), out_tx)
    }

    #[tokio::test]
    async fn a_message_is_told_by_the_host_it_came_from() {
        let (first, _first_tx) = host();
        let (second, second_tx) = host();
        let mut hosts = Hosts::new(first);
        hosts.list.push(second);

        second_tx.send(ServerMsg::Tree(Vec::new())).await.unwrap();

        let event = timeout(Duration::from_secs(5), hosts.next()).await.unwrap();
        assert!(matches!(event, HostEvent::Message(1, Some(ServerMsg::Tree(_)))));
    }

    #[tokio::test]
    async fn one_host_is_on_screen_and_every_app_knows_which_are_attached() {
        let (local, _local_tx) = host();
        let (mut remote, _remote_tx) = host();
        remote.app.host = Some("devbox".into());
        let mut hosts = Hosts::new(local);

        let index = hosts.push(remote);
        assert!(hosts.get(0).app.on_screen);
        assert!(!hosts.get(index).app.on_screen, "a new host arrives off screen");
        assert_eq!(hosts.find(&Some("devbox".into())), Some(index));
        for i in [0, index] {
            assert_eq!(
                hosts.get(i).app.attached_hosts,
                vec![None, Some("devbox".to_string())]
            );
        }

        hosts.show(index);
        assert!(!hosts.get(0).app.on_screen);
        assert!(hosts.get(index).app.on_screen);
        assert!(hosts.is_on_screen(index));
    }

    #[tokio::test]
    async fn a_daemon_going_away_is_reported_once_and_then_left_alone() {
        let (only, tx) = host();
        let mut hosts = Hosts::new(only);

        drop(tx);
        let event = timeout(Duration::from_secs(5), hosts.next()).await.unwrap();
        assert!(matches!(event, HostEvent::Message(0, None)));

        hosts.get(0).connected = false;
        assert!(
            timeout(Duration::from_millis(50), hosts.next()).await.is_err(),
            "a closed host must not be waited on, or the loop spins"
        );
    }
}
