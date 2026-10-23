//! What each side of a connection says about itself before anything else,
//! so that neither sends the other a message it cannot read.
//!
//! A peer from before the handshake hangs up on any message it does not
//! know, so the exchange is shaped around never sending it one. The client
//! greets first; a daemon that greets back can take what it listed. A
//! daemon that sends its tree instead predates the handshake, and a client
//! that sends nothing is taken to predate it too.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::connection::message::ServerMsg;

/// The protocol generation this build speaks.
///
/// Raised only for a change the capabilities cannot carry — one that no
/// peer on the previous number could be spoken to across. Additions are
/// capabilities, not a new number.
pub const PROTOCOL: u32 = 1;

/// The optional messages and encodings this build can take. A side sends
/// one only when the other listed it, so each addition is safe against a
/// peer of any age.
pub const CAPABILITIES: &[&str] = &[
    PANE_TELEMETRY,
    CELL_RUNS,
    PASTE_FILE,
    FOCUS,
    SESSIONS,
    ADD_PROJECT,
    CONSOLES,
    EXTENSIONS,
    EXTENSION_DIALOGS,
    AGENT_VIEWS,
    VIEW_ACTIONS,
    DESCRIBE_EXTENSIONS,
    RENAME_PANE,
    MOVE_TO_CHECKOUT,
    SAVED_VIEWS,
    CODE,
    CODE_EDIT,
    FILES,
    REQUEST_REFUSALS,
    BROWSERS,
    BROWSER_POINTER,
    INDEX_WRITES,
];

/// `ServerMsg::PaneTelemetry` in place of a whole tree when an agent
/// reports its model, context or spend.
pub const PANE_TELEMETRY: &str = "pane-telemetry";

/// `ServerMsg::PaneRows`, `ServerMsg::RowDamage` and
/// `ServerMsg::ScrollbackRuns` in place of their per-cell forms: a pane's
/// screen and its history as runs of cells, and scrolls as moves.
pub const CELL_RUNS: &str = "cell-runs";

/// `ClientMsg::PasteFile`: a file off the client's clipboard or dropped on
/// its terminal, kept on the daemon's host and pasted into a pane as the
/// path there.
pub const PASTE_FILE: &str = "paste-file";

/// Where the person is working: `ClientMsg::View` and
/// `ClientMsg::WatchFocus` from a client, `ServerMsg::Focus` from the
/// daemon. Listed by each side for the half it takes.
pub const FOCUS: &str = "focus";

/// What agents read and asked to show: `ClientMsg::WatchViews`,
/// `ClientMsg::CloseView` and `ClientMsg::RefreshView` from a client,
/// `ServerMsg::ViewShown` and `ServerMsg::ViewClosed` from the daemon.
pub const AGENT_VIEWS: &str = "agent-views";

/// The browsers the daemon runs for agents' panes: `ClientMsg::WatchBrowsers`,
/// `ClientMsg::BrowserInput` and `ClientMsg::CloseBrowser` from a client,
/// `ServerMsg::BrowserPage`, `ServerMsg::BrowserFrame` and
/// `ServerMsg::BrowserClosed` from the daemon. A client that cannot draw a
/// picture leaves it off and is sent none.
pub const BROWSERS: &str = "browsers";

/// The person's pointer on a browser as a window's is: `BrowserInput`'s
/// `Move`, `Press`, `Release` and `Leave` from a client, and
/// `ServerMsg::BrowserCursor` from the daemon, so the page hovers, drags
/// and shows its cursor; and the widgets headless Chrome draws nowhere,
/// a `<select>`'s list and a script's dialog, drawn by the window and
/// answered with `Choose` and `Answer`. A client without it sends a
/// `Click`. Listed by each side for the half it takes, beside
/// [`BROWSERS`].
pub const BROWSER_POINTER: &str = "browser-pointer";

/// `ClientMsg::PressView`: a press of an action an agent put in a view, and
/// `ServerMsg::ViewPressFailed` to the client that pressed when the agent
/// could not be told of it.
pub const VIEW_ACTIONS: &str = "view-actions";

/// `ClientMsg::ListSessions`, `ClientMsg::ReadSession`,
/// `ClientMsg::ListResumes` and `ClientMsg::ResumeSession`, and their
/// answers: a repository's past agent sessions, what one said, and
/// reopening one.
pub const SESSIONS: &str = "sessions";

/// `ClientMsg::PreviewProject` and its answer, and `ClientMsg::AddProject`
/// carrying a name, folders to skip, or a repository to initialise.
pub const ADD_PROJECT: &str = "add-project";

/// Shells that belong to no checkout: `ClientMsg::SpawnConsole` from a
/// client, `ServerMsg::Consoles` from the daemon. Listed by each side for
/// the half it takes.
pub const CONSOLES: &str = "consoles";

/// What extensions publish: `ServerMsg::ExtensionViews`, `ServerMsg::ExtensionCommands` and
/// `ServerMsg::ExtensionNotice` from the daemon, `ClientMsg::ExtensionAction` and
/// `ClientMsg::ExtensionCommand` from a client. Listed by each side for the half it
/// takes.
pub const EXTENSIONS: &str = "extensions";

/// An extension's dialogs: `ServerMsg::ExtensionDialog` and
/// `ServerMsg::ExtensionDialogWithdrawn` to a client that can draw one, and
/// `ClientMsg::ExtensionDialogAnswer` back. Apart from [`EXTENSIONS`] because a
/// dialog goes to one client, and only one that can ask it may be chosen.
pub const EXTENSION_DIALOGS: &str = "extension-dialogs";

/// `ClientMsg::DescribeExtensions` and its answer: what each running
/// extension is and publishes, for `argus extension list` and `show`.
/// Apart from [`EXTENSIONS`] because a client asking need draw none of it.
pub const DESCRIBE_EXTENSIONS: &str = "describe-extensions";

/// `ClientMsg::RenamePane`: a person naming a pane or a console from a
/// client.
pub const RENAME_PANE: &str = "rename-pane";

/// `ClientMsg::MoveToCheckout`: a person moving a pane to another checkout
/// from a client.
pub const MOVE_TO_CHECKOUT: &str = "move-to-checkout";

/// Saved views: `ClientMsg::WatchSavedViews`, `ClientMsg::RefreshSavedView`,
/// `ClientMsg::MarkSavedViewStale`, `ClientMsg::DeleteSavedView` and
/// `ClientMsg::PressSavedView` from a client, `ServerMsg::SavedViews`,
/// `ServerMsg::SavedViewChanged`, `ServerMsg::SavedViewRemoved` and
/// `ServerMsg::SavedViewFailed` from the daemon.
pub const SAVED_VIEWS: &str = "saved-views";

/// The Code stage: `ClientMsg::ListCode`, `ClientMsg::ReadCode` and
/// `ClientMsg::SearchCode` from a client, `ServerMsg::CodeTree`,
/// `ServerMsg::Code`, `ServerMsg::CodeMatches` and their refusals from the
/// daemon.
pub const CODE: &str = "code";

/// Editing a working file live: `ClientMsg::OpenEdit`, `ClientMsg::Edit`
/// and `ClientMsg::CloseEdit` from a client, `ServerMsg::EditOpened`,
/// `ServerMsg::EditChanged`, `ServerMsg::EditSaved`,
/// `ServerMsg::EditRefused` and `ServerMsg::EditClosed` from the daemon.
pub const CODE_EDIT: &str = "code-edit";

/// A checkout's folders as they are on disk: `ClientMsg::ListDir` from a
/// client, `ServerMsg::CheckoutDir` and its refusals from the daemon.
pub const FILES: &str = "files";

/// `ClientMsg::Stage` and `ClientMsg::Commit`, answered by
/// `ServerMsg::Written`: the index taken whole files or picked lines, and a
/// commit made of it.
pub const INDEX_WRITES: &str = "index-writes";

/// `ServerMsg::Refused`: a refusal of a request that named itself, by its
/// id, in place of `Created` with nothing and an `Error`, or the request's
/// own `…Failed`. See `request_refusal`.
pub const REQUEST_REFUSALS: &str = "request-refusals";

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

    /// This greeting less one capability: a client that cannot draw what
    /// it names, though its build knows the messages.
    pub fn without(mut self, capability: &str) -> Hello {
        self.capabilities.retain(|c| c != capability);
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
    use crate::connection::framing::{read_msg, write_msg};
    use crate::connection::message::ClientMsg;

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
            while let Ok(msg) = crate::connection::framing::read_known_msg::<_, ServerMsg>(&mut rd).await {
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
