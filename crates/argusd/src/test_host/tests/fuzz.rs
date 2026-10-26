//! Arbitrary sequences of requests, sent over the socket to a fresh host
//! each, holding the daemon to what it promises a client:
//!
//! - It never drops a connection on its own, hangs, or panics. The daemon
//!   owns every pane and outlives its clients; only `Stop` and `Restart`
//!   end a connection, and each is acknowledged first. A panic in dispatch
//!   ends the asker's connection, and one in a task loses an answer this
//!   contract owes.
//! - A request named by a `request_id` is answered by that id, with what
//!   it made or read or with `Refused`: "A named request the daemon turns
//!   down — a make, a read answered by id, a change to a feature, a review
//!   comment — is refused by that id", to a client that listed
//!   `request-refusals`. A change to a feature is answered by the board it
//!   pushes, or refused by its id.
//! - An unnamed request is never answered: no `Created` or `Refused`
//!   carries id zero ("An unnamed request … is never answered";
//!   `RequestRefusal`: "A daemon never sends zero").
//!
//! A case's size is bounded twice: by [`MAX_STEPS`] requests, and by
//! [`PATIENCE`] for each answer. `PROPTEST_CASES` sets the count, which
//! each reliab tier scales; left unset, a plain `cargo test` runs
//! [`DEFAULT_CASES`].

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use argus_protocol::{read_msg, write_msg, ClientMsg, Hello, ServerMsg, REQUEST_REFUSALS};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestError, TestRunner};
use serde_json::Value;
use tokio::io::split;
use tokio::sync::mpsc;

use crate::test_host::requests::{any_but_an_end, fill_in, kind, name_requests, request_id_mut, Kind, Known};
use crate::test_host::Host;

const MAX_STEPS: usize = 12;
const PATIENCE: Duration = Duration::from_secs(10);
const DEFAULT_CASES: u32 = 8;

/// What the host's runtime names its threads, so that a panic on one is
/// told from a panic anywhere else in the test process.
const HOST_THREAD: &str = "argus-fuzz-host";

static HOST_PANICS: AtomicUsize = AtomicUsize::new(0);

/// Counts panics on the host's threads, then lets them report as before:
/// a panic in a task the daemon spawned is otherwise caught by tokio and
/// seen by nobody.
fn count_host_panics() {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().name() == Some(HOST_THREAD) {
                HOST_PANICS.fetch_add(1, Ordering::SeqCst);
            }
            previous(info);
        }));
    });
}

/// How a named request is answered, by `connection::request_refusal` and
/// the variant's own doc in `connection::message`. Exhaustive, so a new kind
/// is placed here too.
enum Answered {
    /// A message carrying its id: what it made or read, or `Refused`.
    ById,
    /// `ReviewCommentSaved` under its id, or `Refused`.
    BySavedComment,
    /// The board it pushes, or `Refused` under its id.
    ByBoardOrId,
    /// Nothing waits on it by id.
    NotById,
}

fn answered(kind: Kind) -> Answered {
    use Kind as K;
    match kind {
        K::SpawnShell | K::SpawnAgent | K::SpawnConsole | K::CreateWorktree | K::OpenInEditor
        | K::AddProject | K::AddRepository | K::InitRepository | K::ResumeSession | K::Review | K::ReviewFile
        | K::ReviewContext | K::ListCode | K::ReadCode | K::SearchCode | K::ListDir | K::ListCommits
        | K::ListSessions | K::ReadSession | K::ListResumes | K::PreviewProject
        | K::ListDirectories | K::Stage | K::Commit | K::Draft => Answered::ById,
        K::ReviewComment => Answered::BySavedComment,
        K::Feature => Answered::ByBoardOrId,
        K::Subscribe | K::Unsubscribe | K::Input | K::Paste | K::PasteFile | K::Scrollback
        | K::Resize | K::Kill | K::RenamePane | K::MoveToCheckout | K::RemoveCheckout | K::OpenWorkspace
        | K::CreateWorkspace | K::ListCommitFiles | K::GetDecisions | K::TrackerMap
        | K::Reconcile | K::Task | K::Decide | K::ChangeDecision | K::SequenceDiagram
        | K::ListBranches | K::ListFiles | K::SwitchBranch | K::CreateBranch | K::DeleteBranch
        | K::Fetch | K::Pull | K::RemoveProject | K::RemoveRepository | K::Restart | K::Stop
        | K::View | K::WatchFocus | K::WatchViews | K::CloseView | K::RefreshView
        | K::PressView | K::WatchSavedViews | K::RefreshSavedView | K::MarkSavedViewStale
        | K::DeleteSavedView | K::PressSavedView | K::ExtensionAction | K::ExtensionCommand
        | K::ExtensionDialogAnswer | K::DescribeExtensions | K::WatchBrowsers | K::BrowserInput
        | K::CloseBrowser | K::Hello | K::OpenEdit | K::Edit
        | K::CloseEdit => Answered::NotById,
    }
}

/// Whether any `request_id` field in `msg`, at any depth, is `id`.
fn carries(msg: &ServerMsg, field: &str, id: u64) -> bool {
    fn walk(value: &Value, field: &str, id: u64) -> bool {
        match value {
            Value::Object(fields) => fields
                .iter()
                .any(|(k, v)| (k == field && v.as_u64() == Some(id)) || walk(v, field, id)),
            Value::Array(items) => items.iter().any(|v| walk(v, field, id)),
            _ => false,
        }
    }
    walk(&serde_json::to_value(msg).expect("a reply serializes"), field, id)
}

fn is_board(msg: &ServerMsg) -> bool {
    matches!(msg, ServerMsg::Decisions(_) | ServerMsg::Tasks(_) | ServerMsg::SequenceDiagrams(_))
}

/// A request still owed an answer, and where in what was heard the wait
/// for it began.
struct Owed {
    id: u64,
    answered: Answered,
    since: usize,
    msg: ClientMsg,
}

impl Owed {
    fn paid_by(&self, heard: &[ServerMsg]) -> bool {
        let refused = |m: &ServerMsg| matches!(m, ServerMsg::Refused(r) if r.request_id == self.id);
        match self.answered {
            Answered::ById => heard.iter().any(|m| carries(m, "request_id", self.id)),
            Answered::BySavedComment => heard
                .iter()
                .any(|m| refused(m) || matches!(m, ServerMsg::ReviewCommentSaved { id, .. } if *id == self.id)),
            Answered::ByBoardOrId => heard[self.since..].iter().any(|m| refused(m) || is_board(m)),
            Answered::NotById => true,
        }
    }
}

/// One client's connection, read on a task of its own so that a wait that
/// runs out never leaves a frame half read.
struct Client {
    out: tokio::io::WriteHalf<Box<dyn crate::test_host::Stream>>,
    incoming: mpsc::UnboundedReceiver<ServerMsg>,
    heard: Vec<ServerMsg>,
}

impl Client {
    async fn connect(host: &Host) -> Client {
        let (mut read, out) = split(host.connect().await);
        let (tx, incoming) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok(msg) = read_msg::<_, ServerMsg>(&mut read).await {
                if tx.send(msg).is_err() {
                    return;
                }
            }
        });
        Client { out, incoming, heard: Vec::new() }
    }

    async fn send(&mut self, msg: &ClientMsg) -> Result<(), String> {
        write_msg(&mut self.out, msg)
            .await
            .map_err(|e| format!("the daemon would not take {msg:?}: {e}"))
    }

    /// Reads until `done` has been heard, failing if the connection closes
    /// or the daemon says nothing for [`PATIENCE`].
    async fn until(&mut self, what: &str, done: impl Fn(&[ServerMsg]) -> bool) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + PATIENCE;
        while !done(&self.heard) {
            match tokio::time::timeout_at(deadline, self.incoming.recv()).await {
                Ok(Some(msg)) => self.heard.push(msg),
                Ok(None) => return Err(format!("the daemon hung up while {what}")),
                Err(_) => return Err(format!("the daemon said nothing for {PATIENCE:?} while {what}")),
            }
        }
        Ok(())
    }

    /// Asks for something answered in order behind everything sent before
    /// it: once its answer is in, the daemon has dealt with all of that and
    /// is still listening.
    async fn barrier(&mut self, after: &ClientMsg) -> Result<(), String> {
        let since = self.heard.len();
        self.send(&ClientMsg::DescribeExtensions).await?;
        self.until(&format!("answering what followed {after:?}"), |heard| {
            heard[since..].iter().any(|m| matches!(m, ServerMsg::ExtensionsDescribed(_)))
        })
        .await
    }

    /// Reads until the connection closes, which an acknowledged `Stop` or
    /// `Restart` owes.
    async fn closed(&mut self) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + PATIENCE;
        loop {
            match tokio::time::timeout_at(deadline, self.incoming.recv()).await {
                Ok(Some(msg)) => self.heard.push(msg),
                Ok(None) => return Ok(()),
                Err(_) => return Err("the daemon acknowledged an end and kept the connection open".into()),
            }
        }
    }
}

/// Waits for every request in `owed` to be answered.
async fn pay(client: &mut Client, owed: &[Owed]) -> Result<(), String> {
    let paid = |heard: &[ServerMsg]| owed.iter().all(|o| o.paid_by(heard));
    if let Err(e) = client.until("answering the named requests", paid).await {
        let missing: Vec<_> = owed.iter().filter(|o| !o.paid_by(&client.heard)).map(|o| &o.msg).collect();
        return Err(format!("{e}; unanswered: {missing:?}"));
    }
    Ok(())
}

async fn case(known: &Known, msgs: &[ClientMsg]) -> Result<(), String> {
    let panics = HOST_PANICS.load(Ordering::SeqCst);
    let host = Host::start().await;
    let here = Known::prepare(&host);
    if here != *known {
        return Err(format!("this host's fixture differs from the one requests were made for: {here:?}"));
    }
    let mut client = Client::connect(&host).await;
    client.send(&ClientMsg::Hello(Hello::this_build())).await?;
    client.until("greeting", |heard| heard.iter().any(|m| matches!(m, ServerMsg::Hello(_)))).await?;

    let mut refused_by_id = true;
    let mut owed = Vec::new();
    for msg in msgs {
        let msg = fill_in(msg, host.root());
        if let ClientMsg::Hello(hello) = &msg {
            refused_by_id = hello.can(REQUEST_REFUSALS);
        }
        if matches!(msg, ClientMsg::Stop | ClientMsg::Restart) {
            // Nothing is pushed after an end's acknowledgement (dispatch,
            // and `Stop` tells every client to exit), so what is still owed is collected before asking.
            pay(&mut client, &owed).await?;
            owed.clear();
        }
        let mut named = msg.clone();
        let id = request_id_mut(&mut named).map_or(0, |id| *id);
        if id != 0 && refused_by_id {
            owed.push(Owed { id, answered: answered(kind(&msg)), since: client.heard.len(), msg: msg.clone() });
        }
        client.send(&msg).await?;
        match msg {
            ClientMsg::Stop | ClientMsg::Restart => {
                client
                    .until("acknowledging an end", |heard| {
                        heard.iter().any(|m| matches!(m, ServerMsg::Stopping | ServerMsg::Restarting))
                    })
                    .await?;
                client.closed().await?;
                break;
            }
            _ => client.barrier(&msg).await?,
        }
    }

    pay(&mut client, &owed).await?;
    if let Some(m) = client.heard.iter().find(|m| {
        matches!(m, ServerMsg::Created { request_id: 0, .. }) || matches!(m, ServerMsg::Refused(r) if r.request_id == 0)
    }) {
        return Err(format!("an unnamed request was answered: {m:?}"));
    }

    host.stop().await;
    let panicked = HOST_PANICS.load(Ordering::SeqCst) - panics;
    if panicked > 0 {
        return Err(format!("{panicked} panic(s) on the daemon's threads"));
    }
    Ok(())
}

#[test]
fn any_sequence_of_requests_is_answered_and_survived() {
    count_host_panics();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name(HOST_THREAD)
        .enable_all()
        .build()
        .unwrap();
    let known = runtime.block_on(async {
        let host = Host::start().await;
        let known = Known::prepare(&host);
        host.stop().await;
        known
    });
    let end = proptest::option::of(prop_oneof![Just(ClientMsg::Restart), Just(ClientMsg::Stop)]);
    let sequences = (proptest::collection::vec(any_but_an_end(&known), 1..=MAX_STEPS), end).prop_map(|(mut msgs, end)| {
        msgs.extend(end);
        name_requests(&mut msgs);
        msgs
    });
    let cases = std::env::var("PROPTEST_CASES").ok().and_then(|n| n.parse().ok()).unwrap_or(DEFAULT_CASES);
    let mut runner = TestRunner::new(Config {
        cases,
        // Failures are saved beside the crate's other proptest regressions,
        // which is proptest's default unless PROPTEST_DISABLE_FAILURE_PERSISTENCE.
        source_file: Some(file!()),
        // Each try starts a daemon, so shrinking is bounded where a pure
        // property's would not need to be.
        max_shrink_iters: 256,
        ..Config::default()
    });
    let result = runner.run(&sequences, |msgs| runtime.block_on(case(&known, &msgs)).map_err(TestCaseError::fail));
    match result {
        Ok(()) => {}
        Err(TestError::Fail(why, msgs)) => panic!("{why}\nminimal sequence ({} requests): {msgs:#?}", msgs.len()),
        Err(e) => panic!("{e}"),
    }
}
