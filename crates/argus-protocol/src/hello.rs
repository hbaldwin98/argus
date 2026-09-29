//! What each side of a connection says about itself before anything else,
//! so that neither sends the other a message it cannot read.
//!
//! A peer from before the handshake hangs up on any message it does not
//! know, so the exchange is shaped around never sending it one. The client
//! greets first; a daemon that greets back can take what it listed. A
//! daemon that sends its tree instead predates the handshake, and a client
//! that sends nothing is taken to predate it too. See DESIGN.md, "Process
//! model".

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::message::ServerMsg;

/// The protocol generation this build speaks.
///
/// Raised only for a change the capabilities cannot carry — one that no
/// peer on the previous number could be spoken to across. Additions are
/// capabilities, not a new number.
pub const PROTOCOL: u32 = 1;

/// The optional messages and encodings this build can take. A side sends
/// one only when the other listed it, so each addition is safe against a
/// peer of any age.
pub const CAPABILITIES: &[&str] = &[PANE_TELEMETRY, CELL_RUNS, TRANSCRIPTS, OUTBOX];

/// `ServerMsg::PaneTelemetry` in place of a whole tree when an agent
/// reports its model, context or spend.
pub const PANE_TELEMETRY: &str = "pane-telemetry";

/// `ServerMsg::PaneRows`, `ServerMsg::RowDamage` and
/// `ServerMsg::ScrollbackRuns` in place of their per-cell forms: a pane's
/// screen and its history as runs of cells, and scrolls as moves.
pub const CELL_RUNS: &str = "cell-runs";

/// A pane's conversation: `ClientMsg::WatchTranscript` and its siblings
/// from a client, `ServerMsg::Transcript` and `ServerMsg::EarlierTranscript`
/// from the daemon. Listed by each side for the half it takes.
pub const TRANSCRIPTS: &str = "transcripts";

/// Saying something to an agent: `ClientMsg::SendToAgent`,
/// `ClientMsg::CancelQueued` and `ClientMsg::Interrupt` from a client, and
/// `ServerMsg::Sent` in answer.
pub const OUTBOX: &str = "outbox";

/// `ServerMsg::WideTree` in place of `ServerMsg::Tree`: every workspace's
/// projects rather than the open one's. For a client that follows every
/// agent, and must not re-scope the terminals to do it. Not in
/// [`CAPABILITIES`]: the terminal client shows one workspace at a time, so
/// only a client that wants it adds it to its greeting ([`Hello::and`]).
pub const WIDE_TREE: &str = "wide-tree";

/// A client that shows what agents' live channels carry: the web server.
/// Nothing is sent for it; the daemon starts a template's live channel
/// only for a pane that starts while such a client is connected, because
/// a channel puts Argus between the agent and its harness and nobody else
/// reads what it carries. Not in [`CAPABILITIES`], for the same reason.
pub const LIVE_CHANNELS: &str = "live-channels";

/// One side's greeting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    /// The build's own version, for saying which side is behind.
    pub version: String,
    /// What this side can take beyond the protocol's floor. Strings rather
    /// than an enum, so a name this build has never heard of is ignored
    /// rather than failing the whole greeting.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl Hello {
    /// This build's greeting.
    pub fn this_build() -> Hello {
        Hello {
            protocol: PROTOCOL,
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: CAPABILITIES.iter().map(|c| c.to_string()).collect(),
        }
    }

    /// This greeting, also able to take `capability`.
    pub fn and(mut self, capability: &str) -> Hello {
        self.capabilities.push(capability.to_string());
        self
    }

    /// Whether the side that sent this greeting can take `capability`.
    pub fn can(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    /// Whether this build speaks the protocol the greeting names.
    pub fn speaks_ours(&self) -> bool {
        self.protocol == PROTOCOL
    }
}

/// How long a client waits for the daemon to answer its greeting. A daemon
/// that greets answers the moment it reads the greeting, right behind its
/// opening messages, so this only runs out on one that took the greeting
/// and said nothing.
pub const GREETING_WAIT: Duration = Duration::from_secs(3);

/// How a client's greeting went.
#[derive(Debug)]
pub enum Greeting {
    /// The connection is up. `daemon` is `None` from a daemon that took the
    /// greeting and said nothing for the whole wait: kept, with nothing
    /// optional assumed.
    Up {
        daemon: Option<Hello>,
        /// What the daemon sent before its answer, for the client to take
        /// first.
        opening: Vec<ServerMsg>,
    },
    Refused(Refusal),
    /// Closed before the daemon said anything: it went away, rather than
    /// turned the greeting down.
    Closed,
}

/// Why a daemon could not take a client's greeting.
#[derive(Debug)]
pub enum Refusal {
    /// It hung up after its opening messages: a daemon from before the
    /// handshake, which hangs up on a message it cannot read. Told apart
    /// from one that went away by having spoken first.
    Predates,
    /// It greeted back on a protocol number this build does not speak. The
    /// connection is still open, and what it sent is here, for a client
    /// that would rather carry on and say so.
    Protocol {
        daemon: Hello,
        opening: Vec<ServerMsg>,
    },
}

impl Greeting {
    /// Reads the daemon's answer to a greeting the client has just sent,
    /// keeping what comes first.
    ///
    /// From a channel the connection's reader fills rather than from the
    /// stream itself: a frame half read when the wait runs out would leave
    /// the stream misaligned for everything after it.
    pub async fn read(incoming: &mut mpsc::Receiver<ServerMsg>, wait: Duration) -> Greeting {
        let mut opening = Vec::new();
        let answer = tokio::time::timeout(wait, async {
            while let Some(msg) = incoming.recv().await {
                match msg {
                    ServerMsg::Hello(hello) => return Some(hello),
                    other => opening.push(other),
                }
            }
            None
        })
        .await;

        match answer {
            Ok(Some(daemon)) if !daemon.speaks_ours() => {
                Greeting::Refused(Refusal::Protocol { daemon, opening })
            }
            Ok(Some(daemon)) => Greeting::Up {
                daemon: Some(daemon),
                opening,
            },
            Ok(None) if opening.is_empty() => Greeting::Closed,
            Ok(None) => Greeting::Refused(Refusal::Predates),
            Err(_) => Greeting::Up {
                daemon: None,
                opening,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::{read_msg, write_msg};
    use crate::message::ClientMsg;

    /// A fake daemon on the other end of a greeting: sends its opening
    /// messages, reads the greeting, and then does what `then` says. The
    /// client's side is what each client does: a reader filling a channel,
    /// and its greeting written.
    async fn greeting_against<F, Fut>(then: F) -> Greeting
    where
        F: FnOnce(tokio::io::DuplexStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let (client, mut daemon) = tokio::io::duplex(1024 * 1024);
        tokio::spawn(async move {
            write_msg(&mut daemon, &ServerMsg::Tree(Vec::new())).await.unwrap();
            write_msg(&mut daemon, &ServerMsg::Templates(Vec::new()))
                .await
                .unwrap();
            let greeting: ClientMsg = read_msg(&mut daemon).await.unwrap();
            assert!(matches!(greeting, ClientMsg::Hello(_)), "{greeting:?}");
            then(daemon).await;
        });
        greet(client).await
    }

    async fn greet(client: tokio::io::DuplexStream) -> Greeting {
        let (mut rd, mut wr) = tokio::io::split(client);
        let (tx, mut incoming) = mpsc::channel(16);
        tokio::spawn(async move {
            while let Ok(msg) = crate::framing::read_known_msg::<_, ServerMsg>(&mut rd).await {
                if tx.send(msg).await.is_err() {
                    break;
                }
            }
        });
        let _ = write_msg(&mut wr, &ClientMsg::Hello(Hello::this_build())).await;
        Greeting::read(&mut incoming, Duration::from_millis(200)).await
    }

    #[tokio::test]
    async fn a_daemon_that_greets_back_is_named_and_its_opening_kept() {
        let greeting = greeting_against(|mut daemon| async move {
            write_msg(&mut daemon, &ServerMsg::Hello(Hello::this_build()))
                .await
                .unwrap();
            std::future::pending::<()>().await;
        })
        .await;

        let Greeting::Up { daemon, opening } = greeting else {
            panic!("the connection should be up: {greeting:?}");
        };
        assert_eq!(daemon, Some(Hello::this_build()));
        assert!(matches!(
            opening.as_slice(),
            [ServerMsg::Tree(_), ServerMsg::Templates(_)]
        ));
    }

    #[tokio::test]
    async fn a_daemon_that_hangs_up_on_the_greeting_predates_it() {
        // What a daemon from before the handshake does with a message it
        // cannot read.
        let greeting = greeting_against(|daemon| async move { drop(daemon) }).await;
        assert!(matches!(greeting, Greeting::Refused(Refusal::Predates)), "{greeting:?}");
    }

    #[tokio::test]
    async fn a_daemon_on_another_protocol_is_refused_with_what_it_said() {
        let greeting = greeting_against(|mut daemon| async move {
            let other = Hello {
                protocol: PROTOCOL + 1,
                ..Hello::this_build()
            };
            write_msg(&mut daemon, &ServerMsg::Hello(other)).await.unwrap();
            std::future::pending::<()>().await;
        })
        .await;

        let Greeting::Refused(Refusal::Protocol { daemon, opening }) = greeting else {
            panic!("another protocol is refused: {greeting:?}");
        };
        assert_eq!(daemon.protocol, PROTOCOL + 1);
        assert_eq!(opening.len(), 2);
    }

    #[tokio::test]
    async fn a_daemon_that_takes_the_greeting_and_says_nothing_is_still_connected() {
        let greeting = greeting_against(|daemon| async move {
            let _open = daemon;
            std::future::pending::<()>().await
        })
        .await;

        let Greeting::Up { daemon, opening } = greeting else {
            panic!("the connection should be up: {greeting:?}");
        };
        assert_eq!(daemon, None);
        assert_eq!(opening.len(), 2, "nothing it sent is lost");
    }

    #[tokio::test]
    async fn a_connection_closed_before_anything_is_not_mistaken_for_a_refusal() {
        let (client, daemon) = tokio::io::duplex(1024);
        drop(daemon);
        assert!(matches!(greet(client).await, Greeting::Closed));
    }

    #[test]
    fn a_greeting_names_this_build() {
        let hello = Hello::this_build();
        assert_eq!(hello.protocol, PROTOCOL);
        assert_eq!(hello.version, env!("CARGO_PKG_VERSION"));
        for capability in CAPABILITIES {
            assert!(hello.can(capability));
        }
    }

    #[test]
    fn a_capability_this_build_has_never_heard_of_is_just_not_there() {
        let hello = Hello {
            protocol: PROTOCOL + 1,
            version: "99.0.0".into(),
            capabilities: vec!["teleport".into()],
        };
        let bytes = rmp_serde::to_vec_named(&hello).unwrap();
        let read: Hello = rmp_serde::from_slice(&bytes).unwrap();
        assert!(read.can("teleport"));
        assert!(!read.can("pane-telemetry"));
    }
}
