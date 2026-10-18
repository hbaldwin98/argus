//! One client's conversation with a [`Host`], written down as it goes:
//! each request sent, then everything the daemon said before it went
//! quiet, scrubbed and rendered, as text for a snapshot to pin.
//!
//! A step ends when the message it waits for has arrived and nothing more
//! follows within [`QUIET`]. Within a step, messages are grouped by kind,
//! keeping their order inside each kind: topics are forwarded on tasks of
//! their own, so a `Tree` and a `Decisions` from one change can arrive
//! either way round, and no client relies on which.

use std::fmt::Write as _;
use std::time::Duration;

use argus_protocol::{read_msg, write_msg, ClientMsg, Hello, ServerMsg};
use tokio::io::{split, WriteHalf};
use tokio::sync::mpsc;

use super::render::{kind_and_body, Render};
use super::scrub::Scrub;
use super::{Host, Stream};

/// How long a step waits past its last message for another. Long enough
/// for a topic's forwarder to pass on news from the change the step made;
/// work the daemon does off its message loop, such as Git's, is waited
/// for by the message it ends in instead.
const QUIET: Duration = Duration::from_millis(250);

/// The longest a step waits for the message it is waiting for.
const PATIENCE: Duration = Duration::from_secs(10);

pub struct Transcript {
    out: WriteHalf<Box<dyn Stream>>,
    incoming: mpsc::UnboundedReceiver<ServerMsg>,
    scrub: Scrub,
    render: Render,
    text: String,
}

impl Transcript {
    /// A new connection to `host`, past what the daemon sends every client
    /// that connects, which is written down as the first step.
    pub async fn open(host: &Host) -> Transcript {
        let (mut read, out) = split(host.connect().await);
        let (tx, incoming) = mpsc::unbounded_channel();
        // Reading on a task of its own, so that a step that stops waiting
        // never abandons a frame half read.
        tokio::spawn(async move {
            while let Ok(msg) = read_msg::<_, ServerMsg>(&mut read).await {
                if tx.send(msg).is_err() {
                    return;
                }
            }
        });
        let mut transcript = Transcript {
            out,
            incoming,
            scrub: Scrub::new(host.root()),
            render: Render::default(),
            text: String::new(),
        };
        transcript.text.push_str("(connected)\n");
        transcript.until(|m| matches!(m, ServerMsg::Workspaces(_))).await;
        transcript
    }

    /// Greets as this build's clients do, and waits for the greeting back.
    pub async fn hello(&mut self) -> Vec<ServerMsg> {
        self.send(ClientMsg::Hello(Hello::this_build())).await;
        self.until(|m| matches!(m, ServerMsg::Hello(_))).await
    }

    /// A connection to `host` that has greeted, written down from there:
    /// the opening is its own transcript, and repeating it in every other
    /// one would make a new capability a change to all of them. What the
    /// opening sent still counts as the last of each kind, so the first
    /// tree after it is written as what changed.
    pub async fn greeted(host: &Host) -> Transcript {
        let mut transcript = Transcript::open(host).await;
        transcript.hello().await;
        transcript.text = "(connected and greeted, as in connecting_and_greeting)\n".to_owned();
        transcript
    }

    pub async fn send(&mut self, msg: ClientMsg) {
        let (kind, body) = kind_and_body(&msg);
        let text = self.render.sent(&kind, self.scrub.value(body));
        self.text.push_str(&text);
        write_msg(&mut self.out, &msg)
            .await
            .expect("the host should take a request");
    }

    /// Sends `msg`, then what [`Self::until`] reads for `done`.
    pub async fn ask(&mut self, msg: ClientMsg, done: impl Fn(&ServerMsg) -> bool) -> Vec<ServerMsg> {
        self.send(msg).await;
        self.until(done).await
    }

    /// Sends `msg`, then what [`Self::quiet`] reads.
    pub async fn tell(&mut self, msg: ClientMsg) -> Vec<ServerMsg> {
        self.send(msg).await;
        self.quiet().await
    }

    /// Reads until a message `done` accepts has arrived and the daemon has
    /// gone quiet after it, and writes the step down.
    pub async fn until(&mut self, done: impl Fn(&ServerMsg) -> bool) -> Vec<ServerMsg> {
        self.until_each(&[&done]).await
    }

    /// Sends `msg`, then what [`Self::until_each`] reads for `done`.
    pub async fn ask_each(&mut self, msg: ClientMsg, done: &[&dyn Fn(&ServerMsg) -> bool]) -> Vec<ServerMsg> {
        self.send(msg).await;
        self.until_each(done).await
    }

    /// [`Self::until`] for a step that ends in several messages, which may
    /// arrive in any order: an answer, and the push of what it changed.
    pub async fn until_each(&mut self, done: &[&dyn Fn(&ServerMsg) -> bool]) -> Vec<ServerMsg> {
        let mut step = Vec::new();
        let deadline = tokio::time::Instant::now() + PATIENCE;
        while !done.iter().all(|done| step.iter().any(done)) {
            match tokio::time::timeout_at(deadline, self.incoming.recv()).await {
                Ok(Some(msg)) => step.push(msg),
                Ok(None) => panic!("the host hung up before answering; heard {step:#?}"),
                Err(_) => panic!("no answer arrived in {PATIENCE:?}; heard {step:#?}"),
            }
        }
        self.settle(step).await
    }

    /// Reads until the daemon has said nothing for [`QUIET`], for a request
    /// that may be answered by nothing at all.
    pub async fn quiet(&mut self) -> Vec<ServerMsg> {
        self.settle(Vec::new()).await
    }

    /// Writes down something seen other than on the wire: what a request
    /// did to a pane's process, or to the disk.
    pub fn note(&mut self, line: &str) {
        let _ = writeln!(self.text, "== {line}");
    }

    pub fn finish(self) -> String {
        self.text
    }

    async fn settle(&mut self, mut step: Vec<ServerMsg>) -> Vec<ServerMsg> {
        while let Ok(Some(msg)) = tokio::time::timeout(QUIET, self.incoming.recv()).await {
            step.push(msg);
        }
        if step.is_empty() {
            self.text.push_str("<- (nothing)\n");
        }
        let mut heard: Vec<_> = step.iter().map(kind_and_body).collect();
        heard.sort_by(|a, b| a.0.cmp(&b.0));
        for (kind, body) in heard {
            let text = self.render.heard(&kind, self.scrub.value(body));
            self.text.push_str(&text);
        }
        step
    }
}
