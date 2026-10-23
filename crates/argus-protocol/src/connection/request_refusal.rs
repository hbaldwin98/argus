//! How a request the daemon turned down is told to the client that named
//! it — by the id it was given and why, in one shape for every kind of
//! request — and the shapes a peer from before that shape is sent, or
//! sends, instead.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::tree::ids::CheckoutId;
use crate::connection::message::{ClientMsg, ServerMsg};

/// A request turned down, to the client that named it. Sent only to a
/// client that listed [`crate::connection::hello::REQUEST_REFUSALS`], and
/// only for a request whose id is not zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestRefusal {
    /// The id the request named itself with. A daemon never sends zero:
    /// zero is what [`OlderDaemon`] makes of a bare `Error`, which named
    /// nothing.
    pub request_id: u64,
    /// Why, in a sentence for the person.
    pub reason: String,
    /// The request only read. Its refusal matters to a reader still waiting
    /// on that answer and is dropped by any other, as the answer would have
    /// been; a refused change is the person's to hear whoever waited on it.
    pub read: bool,
}

impl RequestRefusal {
    /// Whether this turns down the change named `request_id`. An older
    /// daemon's bare error names no request, so it may be any change still
    /// waiting: all that daemon lets a client know.
    pub fn refuses_change(&self, request_id: u64) -> bool {
        self.request_id == request_id || self.request_id == 0
    }
}

/// A request as a refusal of it is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asked {
    /// The id it named itself with; zero for one that named none.
    pub request_id: u64,
    read: bool,
    older: Older,
}

/// How a client from before `REQUEST_REFUSALS` is told a request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Older {
    /// A bare `Error`.
    Error,
    /// `Created` with nothing made, then the reason as an `Error`.
    Made,
    Review(CheckoutId),
    ReviewContext(CheckoutId),
    Code(CheckoutId),
    Commits(CheckoutId),
}

impl Asked {
    /// What `msg` is, as a refusal of it would be told. Every message has
    /// its arm, so a request added to the protocol says how it is refused
    /// before it compiles.
    pub fn of(msg: &ClientMsg) -> Asked {
        use ClientMsg as M;
        let (request_id, read, older) = match msg {
            M::SpawnShell { request_id, .. }
            | M::SpawnAgent { request_id, .. }
            | M::SpawnConsole { request_id }
            | M::CreateWorktree { request_id, .. }
            | M::OpenInEditor { request_id, .. }
            | M::AddProject { request_id, .. }
            | M::AddRepository { request_id, .. }
            | M::InitRepository { request_id, .. }
            | M::ResumeSession { request_id, .. } => (*request_id, false, Older::Made),
            M::Feature { request_id, .. }
            | M::ReviewComment { request_id, .. }
            | M::Stage { request_id, .. }
            | M::Commit { request_id, .. } => (*request_id, false, Older::Error),
            M::Review { request_id, checkout, .. } => (*request_id, true, Older::Review(*checkout)),
            M::ReviewContext { request_id, checkout, .. } => (*request_id, true, Older::ReviewContext(*checkout)),
            M::ListCode { request_id, checkout, .. }
            | M::ReadCode { request_id, checkout, .. }
            | M::SearchCode { request_id, checkout, .. } => (*request_id, true, Older::Code(*checkout)),
            M::ListDir { request_id, .. } => (*request_id, true, Older::Error),
            M::ListCommits { request_id, checkout } => (*request_id, true, Older::Commits(*checkout)),
            M::ListSessions { request_id, .. }
            | M::ReadSession { request_id, .. }
            | M::ListResumes { request_id }
            | M::PreviewProject { request_id, .. }
            | M::ListDirectories { request_id, .. } => (*request_id, true, Older::Error),
            // Nothing waits on how these went, so a refusal is said to
            // whoever is there.
            M::Subscribe { .. } | M::Unsubscribe { .. } | M::Input { .. } | M::Paste { .. }
            | M::PasteFile { .. } | M::Scrollback { .. } | M::Resize { .. } | M::Kill { .. }
            | M::RenamePane { .. } | M::MoveToCheckout { .. } | M::RemoveCheckout { .. } | M::OpenWorkspace { .. }
            | M::CreateWorkspace { .. } | M::ListCommitFiles { .. }
            | M::GetDecisions { .. } | M::TrackerMap { .. } | M::Reconcile { .. } | M::Task { .. }
            | M::Decide { .. } | M::ChangeDecision { .. } | M::SequenceDiagram { .. }
            | M::ListBranches { .. } | M::ListFiles { .. } | M::SwitchBranch { .. }
            | M::CreateBranch { .. } | M::DeleteBranch { .. } | M::Fetch { .. } | M::Pull { .. }
            | M::RemoveProject { .. } | M::RemoveRepository { .. } | M::Restart | M::Stop
            | M::View(_) | M::WatchFocus | M::WatchViews | M::CloseView { .. } | M::RefreshView { .. }
            | M::PressView { .. } | M::WatchSavedViews | M::RefreshSavedView { .. }
            | M::MarkSavedViewStale { .. } | M::DeleteSavedView { .. } | M::PressSavedView { .. }
            | M::ExtensionAction { .. } | M::ExtensionCommand { .. } | M::ExtensionDialogAnswer { .. }
            | M::DescribeExtensions | M::WatchBrowsers { .. } | M::BrowserInput { .. } | M::CloseBrowser { .. }
            | M::Hello(_) => (0, false, Older::Error),
            // An edit's refusals are its own messages, keyed by the file.
            M::OpenEdit { .. } | M::Edit { .. } | M::CloseEdit { .. } => (0, false, Older::Error),
        };
        Asked { request_id, read, older }
    }

    /// What the daemon sends the client that asked, refusing it with
    /// `reason`. `tagged`: that client listed `REQUEST_REFUSALS`.
    pub fn refusal(&self, reason: String, tagged: bool) -> Vec<ServerMsg> {
        let request_id = self.request_id;
        if tagged && request_id != 0 {
            return vec![ServerMsg::Refused(RequestRefusal { request_id, reason, read: self.read })];
        }
        let message = reason;
        match self.older {
            Older::Made if request_id != 0 => {
                vec![ServerMsg::Created { request_id, created: None }, ServerMsg::Error { message }]
            }
            Older::Error | Older::Made => vec![ServerMsg::Error { message }],
            Older::Review(checkout) => vec![ServerMsg::ReviewFailed { request_id, checkout, message }],
            Older::ReviewContext(checkout) => vec![ServerMsg::ReviewContextFailed { request_id, checkout, message }],
            Older::Code(checkout) => vec![ServerMsg::CodeFailed { request_id, checkout, message }],
            Older::Commits(checkout) => vec![ServerMsg::CommitsFailed { request_id, checkout, message }],
        }
    }
}

/// Reads a daemon from before `REQUEST_REFUSALS` as one after it, so a
/// client hears a refusal one way whatever daemon it reached. Its old
/// failures name their request already. A refused make came as `Created`
/// with nothing and then its reason as an `Error` from the same task, so
/// the next bare `Error` is taken as that reason, oldest first; any other
/// bare `Error` named nothing and is read as a refusal of id zero.
#[derive(Debug, Default)]
pub struct OlderDaemon {
    made_nothing: VecDeque<u64>,
}

impl OlderDaemon {
    /// `msg` as the newer daemon would have sent it; `None` for half of a
    /// refusal still to be completed.
    pub fn read(&mut self, msg: ServerMsg) -> Option<ServerMsg> {
        let refused = |request_id, reason, read| Some(ServerMsg::Refused(RequestRefusal { request_id, reason, read }));
        match msg {
            ServerMsg::ReviewFailed { request_id, message, .. }
            | ServerMsg::ReviewContextFailed { request_id, message, .. }
            | ServerMsg::CodeFailed { request_id, message, .. }
            | ServerMsg::CommitsFailed { request_id, message, .. } => refused(request_id, message, true),
            ServerMsg::Created { request_id, created: None } => {
                self.made_nothing.push_back(request_id);
                None
            }
            ServerMsg::Error { message } => refused(self.made_nothing.pop_front().unwrap_or(0), message, false),
            other => Some(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{CodeVersion, SearchOptions};
    use crate::board::features::FeatureAction;
    use crate::tree::ids::{PaneId, ProjectId, RepositoryId};
    use crate::review::{ReviewAnchor, ReviewBase};

    const CO: CheckoutId = CheckoutId(3);

    /// `ServerMsg` has no `PartialEq`; its debug form says everything a
    /// refusal carries.
    fn shown(msgs: &[ServerMsg]) -> String {
        format!("{msgs:?}")
    }

    fn refused(request_id: u64, reason: &str, read: bool) -> ServerMsg {
        ServerMsg::Refused(RequestRefusal { request_id, reason: reason.into(), read })
    }

    fn spawn_shell(request_id: u64) -> ClientMsg {
        ClientMsg::SpawnShell { checkout: CO, request_id }
    }

    fn add_project(request_id: u64) -> ClientMsg {
        ClientMsg::AddProject { path: "/src".into(), request_id, name: None, skip: Vec::new(), init: false }
    }

    fn feature(request_id: u64) -> ClientMsg {
        ClientMsg::Feature {
            project: ProjectId(1),
            checkout: CO,
            action: FeatureAction::Drop { slug: "trim".into() },
            request_id,
        }
    }

    fn review_comment(request_id: u64) -> ClientMsg {
        let anchor = ReviewAnchor {
            base: ReviewBase::Unstaged,
            commit: None,
            path: "a.rs".into(),
            old_path: None,
            old_start: None,
            old_end: None,
            new_start: Some(2),
            new_end: Some(2),
            text: vec!["+new".into()],
        };
        ClientMsg::ReviewComment { checkout: CO, recipient: PaneId(4), anchor: Box::new(anchor), body: "tidy".into(), request_id }
    }

    fn review(request_id: u64) -> ClientMsg {
        ClientMsg::Review { request_id, checkout: CO, base: ReviewBase::Unstaged, commit: None }
    }

    fn review_context(request_id: u64) -> ClientMsg {
        ClientMsg::ReviewContext { request_id, checkout: CO, blob: "abc".into(), path: "a.rs".into(), start: 1, end: 9 }
    }

    fn list_code(request_id: u64) -> ClientMsg {
        ClientMsg::ListCode { request_id, checkout: CO, version: CodeVersion::WorkingTree }
    }

    fn read_code(request_id: u64) -> ClientMsg {
        ClientMsg::ReadCode { request_id, checkout: CO, path: "a.rs".into(), version: CodeVersion::WorkingTree }
    }

    fn search_code(request_id: u64) -> ClientMsg {
        ClientMsg::SearchCode { request_id, checkout: CO, query: "fn".into(), options: SearchOptions::default() }
    }

    fn list_dir(request_id: u64) -> ClientMsg {
        ClientMsg::ListDir { request_id, checkout: CO, path: String::new() }
    }

    fn list_commits(request_id: u64) -> ClientMsg {
        ClientMsg::ListCommits { request_id, checkout: CO }
    }

    fn list_sessions(request_id: u64) -> ClientMsg {
        ClientMsg::ListSessions { request_id, repository: RepositoryId(2) }
    }

    #[test]
    fn a_client_that_listed_refusals_is_refused_by_the_id_it_gave() {
        for (msg, id, read) in [(spawn_shell(8), 8, false), (read_code(4), 4, true), (feature(5), 5, false), (review_comment(6), 6, false)] {
            let told = Asked::of(&msg).refusal("no".into(), true);
            assert_eq!(shown(&told), shown(&[refused(id, "no", read)]), "{msg:?}");
        }
    }

    #[test]
    fn a_request_that_named_nothing_is_refused_as_an_error_whoever_asks() {
        let fetch = ClientMsg::Fetch { checkout: CO };
        let input = ClientMsg::Input { pane: PaneId(1), bytes: b"x".to_vec() };
        for msg in [spawn_shell(0), add_project(0), feature(0), review_comment(0), fetch, input] {
            for tagged in [false, true] {
                let told = Asked::of(&msg).refusal("no".into(), tagged);
                assert!(matches!(&told[..], [ServerMsg::Error { message }] if message == "no"), "{msg:?}: {told:?}");
            }
        }
    }

    #[test]
    fn an_older_client_is_refused_each_request_the_way_it_always_was() {
        let message = String::from("gone");
        let cases = [
            (
                spawn_shell(8),
                vec![ServerMsg::Created { request_id: 8, created: None }, ServerMsg::Error { message: message.clone() }],
            ),
            (
                add_project(6),
                vec![ServerMsg::Created { request_id: 6, created: None }, ServerMsg::Error { message: message.clone() }],
            ),
            (review(1), vec![ServerMsg::ReviewFailed { request_id: 1, checkout: CO, message: message.clone() }]),
            (
                review_context(2),
                vec![ServerMsg::ReviewContextFailed { request_id: 2, checkout: CO, message: message.clone() }],
            ),
            (list_code(3), vec![ServerMsg::CodeFailed { request_id: 3, checkout: CO, message: message.clone() }]),
            (read_code(4), vec![ServerMsg::CodeFailed { request_id: 4, checkout: CO, message: message.clone() }]),
            (search_code(5), vec![ServerMsg::CodeFailed { request_id: 5, checkout: CO, message: message.clone() }]),
            (list_dir(8), vec![ServerMsg::Error { message: message.clone() }]),
            (list_commits(7), vec![ServerMsg::CommitsFailed { request_id: 7, checkout: CO, message: message.clone() }]),
            (feature(9), vec![ServerMsg::Error { message: message.clone() }]),
            (list_sessions(10), vec![ServerMsg::Error { message: message.clone() }]),
            (review_comment(11), vec![ServerMsg::Error { message: message.clone() }]),
        ];
        for (msg, want) in cases {
            assert_eq!(shown(&Asked::of(&msg).refusal(message.clone(), false)), shown(&want), "{msg:?}");
        }
    }

    #[test]
    fn a_read_is_refused_as_a_read_and_a_change_as_a_change() {
        let read = |msg: ClientMsg| match &Asked::of(&msg).refusal("no".into(), true)[..] {
            [ServerMsg::Refused(refusal)] => refusal.read,
            other => panic!("{msg:?}: {other:?}"),
        };
        assert!(read(review(1)) && read(list_sessions(2)) && read(search_code(3)) && read(list_dir(8)));
        let spawn_agent = ClientMsg::SpawnAgent { checkout: CO, template: "claude".into(), prompt: None, request_id: 4 };
        assert!(!read(spawn_agent) && !read(add_project(5)) && !read(feature(6)) && !read(review_comment(7)));
    }

    #[test]
    fn an_older_daemons_refusal_reads_as_this_ones_where_it_named_the_request() {
        let mut older = OlderDaemon::default();
        for msg in [spawn_shell(1), add_project(2), review(3), review_context(4), list_code(5), list_commits(6)] {
            let asked = Asked::of(&msg);
            let read: Vec<ServerMsg> =
                asked.refusal("gone".into(), false).into_iter().filter_map(|msg| older.read(msg)).collect();
            assert_eq!(shown(&read), shown(&asked.refusal("gone".into(), true)), "{msg:?}");
        }
    }

    #[test]
    fn an_older_daemons_refused_makes_take_the_errors_that_follow_them_in_order() {
        let mut older = OlderDaemon::default();
        let read: Vec<ServerMsg> = [
            ServerMsg::Created { request_id: 1, created: None },
            ServerMsg::Created { request_id: 2, created: None },
            ServerMsg::Error { message: "a".into() },
            ServerMsg::Error { message: "b".into() },
        ]
        .into_iter()
        .filter_map(|msg| older.read(msg))
        .collect();
        assert_eq!(shown(&read), shown(&[refused(1, "a", false), refused(2, "b", false)]));
    }

    #[test]
    fn an_older_daemons_bare_error_may_refuse_any_waiting_change_and_no_named_read() {
        let mut older = OlderDaemon::default();
        let Some(ServerMsg::Refused(bare)) = older.read(ServerMsg::Error { message: "fetch failed".into() }) else {
            panic!("a bare error reads as a refusal");
        };
        assert_eq!(bare, RequestRefusal { request_id: 0, reason: "fetch failed".into(), read: false });
        assert!(bare.refuses_change(7));
        let named = RequestRefusal { request_id: 8, reason: "no".into(), read: false };
        assert!(!named.refuses_change(7) && named.refuses_change(8));
    }

    #[tokio::test]
    async fn a_refusal_survives_the_wire() {
        let refusal = RequestRefusal { request_id: 7, reason: "no such checkout: 9999".into(), read: false };
        let bytes = rmp_serde::to_vec_named(&ServerMsg::Refused(refusal.clone())).unwrap();
        let read: ServerMsg = rmp_serde::from_slice(&bytes).unwrap();
        assert!(matches!(read, ServerMsg::Refused(read) if read == refusal));

        // A build from before the variant skips it and reads on.
        #[derive(Deserialize)]
        enum OlderServerMsg {
            Error { message: String },
        }
        let mut buf: Vec<u8> = Vec::new();
        for msg in [ServerMsg::Error { message: "a".into() }, ServerMsg::Refused(refusal), ServerMsg::Error { message: "b".into() }] {
            crate::connection::framing::write_msg(&mut buf, &msg).await.unwrap();
        }
        let mut r = buf.as_slice();
        for want in ["a", "b"] {
            let OlderServerMsg::Error { message } = crate::connection::framing::read_known_msg(&mut r).await.unwrap();
            assert_eq!(message, want);
        }
    }
}
