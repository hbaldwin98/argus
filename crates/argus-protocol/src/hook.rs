//! The pane API's URL grammar: the loopback base an agent is handed, the
//! path of each endpoint under it, and the board a request is filed on.
//!
//! The daemon and its installer build these URLs, `argus-hook` rebases and
//! extends them, and the daemon parses them back — in separate binaries
//! that never share a type unless it lives here. Written twice they drift
//! silently: a new endpoint on one side compiles perfectly and fails at
//! runtime against the other. Written once they cannot.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::artifacts::ArtifactScope;
use crate::ids::PaneId;
use crate::tree::PaneStatus;

/// The environment every agent pane is handed. The daemon sets these and
/// `argus-hook` reads them back, so they are named here for the reason the
/// URL grammar is: written on both sides they drift in silence.
pub const URL_VAR: &str = "ARGUS_HOOK_URL";
pub const TOKEN_VAR: &str = "ARGUS_HOOK_TOKEN";
pub const HELPER_VAR: &str = "ARGUS_HOOK";
pub const PANE_VAR: &str = "ARGUS_PANE";
pub const INSTRUCTIONS_VAR: &str = "ARGUS_INSTRUCTIONS";

/// Set by a person, never by the daemon: files an agent's board requests on
/// the workspace board instead of its checkout's repository. Crossing
/// repositories is opted into per command rather than inherited from
/// whichever workspace the TUI has open.
pub const ARTIFACT_SCOPE_VAR: &str = "ARGUS_ARTIFACT_SCOPE";

/// The only host a pane URL may name. The listener binds loopback, and a
/// helper handed a URL anywhere else would be posting its token off the
/// machine.
const LOOPBACK: &str = "127.0.0.1";

/// The query that files a request on the workspace board. Absent means the
/// repository board, so a helper that predates the scope still lands where
/// it always did.
const WORKSPACE_QUERY: &str = "scope=workspace";

/// The query that names a feature for a board request, ahead of its slug.
/// Absent means the feature the pane's checkout is on.
const FEATURE_QUERY: &str = "feature=";

/// The context-only helper command used by stable, environment-based hooks.
pub const INSTRUCTIONS_COMMAND: &str = "instructions";

/// What an agent reads before it starts: review comments, the feature brief
/// and its open tasks in one answer. Context hooks run it so an agent begins
/// with them instead of spending tool calls on three separate reads.
pub const CONTEXT_COMMAND: &str = "context";

/// Names the conversation a report comes from, so the daemon can tell the
/// agent that owns a pane from one spawned inside it — which inherits the
/// pane's environment and would otherwise rewrite its parent's row.
pub const SESSION_HEADER: &str = "X-Argus-Session";

/// Flags a managed hook command carries. The installer writes them into a
/// harness's settings file and the helper parses them back out.
///
/// Read the harness's message off stdin and send it as the pane's note.
/// Only passed on events that actually supply one — the helper must never
/// block on a stdin nobody is writing to.
pub const NOTE_FLAG: &str = "--note-from-stdin";
/// Same stdin, posted as the pane title. Prompt-submit events only.
pub const TITLE_FLAG: &str = "--title-from-stdin";
/// The key under which that stdin carries the harness's session id.
pub const SESSION_KEY_FLAG: &str = "--session-id-from-stdin";
/// Marks the one event per harness that may claim the pane's resume
/// identity. Without it a CLI started from inside a pane would overwrite
/// the conversation Argus reopens for that row.
pub const OWNS_SESSION_FLAG: &str = "--owns-session";

/// What an agent can say about itself.
///
/// The wire spelling is the one in the URL, which is also the one a harness
/// config maps its own event names onto — so this is the vocabulary of the
/// pane API rather than an internal enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Report {
    Working,
    Idle,
    Waiting,
    #[serde(rename = "needs-review")]
    NeedsReview,
    Done,
    Failed,
}

impl Report {
    pub const ALL: [Report; 6] = [
        Report::Working,
        Report::Idle,
        Report::Waiting,
        Report::NeedsReview,
        Report::Done,
        Report::Failed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Report::Working => "working",
            Report::Idle => "idle",
            Report::Waiting => "waiting",
            Report::NeedsReview => "needs-review",
            Report::Done => "done",
            Report::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Report> {
        Report::ALL.into_iter().find(|r| r.as_str() == s)
    }

    pub fn status(self) -> PaneStatus {
        match self {
            Report::Working => PaneStatus::Working,
            Report::Idle => PaneStatus::Idle,
            Report::Waiting => PaneStatus::Waiting,
            Report::NeedsReview => PaneStatus::NeedsReview,
            Report::Done => PaneStatus::Done,
            Report::Failed => PaneStatus::Failed,
        }
    }
}

/// What a request to a pane is asking it to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    Status(Report),
    Title,
    Checkout,
    Session,
    Comments,
    /// The decision board of the feature this pane's checkout is on, read
    /// whole. A tree with its roots cut off explains nothing, so it is
    /// never trimmed inside a feature — but it is scoped to one, because a
    /// broad board is a pile rather than a reference.
    Decisions,
    /// One decision appended to that board.
    Decide,
    /// A [`crate::DecisionChange`] to one decision already on it.
    DecisionChange,
    /// The artifact scope's features, and which one this checkout is on.
    Features,
    /// A change to that: opening a feature, pointing this checkout at one,
    /// or adding a paragraph to its document.
    Feature,
    /// The current feature's tasks, and every change to them.
    Tasks,
    /// The current feature's sequence diagrams, and every change to them.
    Diagrams,
    /// A partial [`crate::AgentTelemetry`] report, as JSON.
    Telemetry,
}

impl Endpoint {
    /// The part of the path after `/pane/<id>/`.
    ///
    /// The status is named here rather than in the body because the harness
    /// installer already resolved the harness's own event name into one of
    /// these — which is what lets a new harness be a config block instead of
    /// a match arm in the daemon.
    pub fn suffix(self) -> Cow<'static, str> {
        match self {
            Endpoint::Status(report) => Cow::Owned(format!("status/{}", report.as_str())),
            Endpoint::Title => Cow::Borrowed("title"),
            Endpoint::Checkout => Cow::Borrowed("checkout"),
            Endpoint::Session => Cow::Borrowed("session"),
            Endpoint::Comments => Cow::Borrowed("comments"),
            Endpoint::Decisions => Cow::Borrowed("decisions"),
            Endpoint::Decide => Cow::Borrowed("decide"),
            Endpoint::DecisionChange => Cow::Borrowed("decision"),
            Endpoint::Features => Cow::Borrowed("features"),
            Endpoint::Feature => Cow::Borrowed("feature"),
            Endpoint::Tasks => Cow::Borrowed("tasks"),
            Endpoint::Diagrams => Cow::Borrowed("diagrams"),
            Endpoint::Telemetry => Cow::Borrowed("telemetry"),
        }
    }
}

/// `/pane/<id>` — the prefix every pane request shares, and everything an
/// `ARGUS_HOOK_URL` carries after its authority.
pub fn pane_prefix(pane: PaneId) -> String {
    format!("/pane/{}", pane.0)
}

/// `/pane/<id>/<suffix>` — the inverse of [`parse_pane_path`].
pub fn pane_path(pane: PaneId, endpoint: Endpoint) -> String {
    format!("{}/{}", pane_prefix(pane), endpoint.suffix())
}

/// The pane and endpoint a request path names, or `None` for anything that
/// is not one of them.
pub fn parse_pane_path(path: &str) -> Option<(PaneId, Endpoint)> {
    let mut parts = path.trim_start_matches('/').split('/');
    if parts.next()? != "pane" {
        return None;
    }
    let pane = PaneId(parts.next()?.parse().ok()?);
    let endpoint = match parts.next()? {
        "status" => Endpoint::Status(Report::parse(parts.next()?)?),
        "title" => Endpoint::Title,
        "checkout" => Endpoint::Checkout,
        "session" => Endpoint::Session,
        "comments" => Endpoint::Comments,
        "decisions" => Endpoint::Decisions,
        "decide" => Endpoint::Decide,
        "decision" => Endpoint::DecisionChange,
        "features" => Endpoint::Features,
        "feature" => Endpoint::Feature,
        "tasks" => Endpoint::Tasks,
        "diagrams" => Endpoint::Diagrams,
        "telemetry" => Endpoint::Telemetry,
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((pane, endpoint))
}

/// `http://127.0.0.1:<port>/pane/<id>` — what `ARGUS_HOOK_URL` holds, and
/// the base every endpoint for the pane hangs off.
pub fn pane_url(port: u16, pane: PaneId) -> String {
    format!("http://{LOOPBACK}:{port}{}", pane_prefix(pane))
}

/// A pane URL read back: the listener it names, the pane, and whatever
/// follows the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneUrl<'a> {
    pub port: u16,
    pub pane: PaneId,
    /// Everything after `/pane/<id>`, endpoint and query included; empty
    /// for a bare base.
    pub rest: &'a str,
}

impl PaneUrl<'_> {
    /// The URL with everything after the pane dropped.
    pub fn base(&self) -> String {
        pane_url(self.port, self.pane)
    }
}

/// The inverse of [`pane_url`], and of [`endpoint_url`] with the endpoint
/// left in `rest`. `None` for anything not on the loopback listener.
pub fn parse_pane_url(url: &str) -> Option<PaneUrl<'_>> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = rest.split_once('/')?;
    let (host, port) = authority.rsplit_once(':')?;
    if host != LOOPBACK {
        return None;
    }
    let port = port.parse().ok()?;
    let path = path.strip_prefix("pane/")?;
    let (id, rest) = path.find('/').map_or((path, ""), |i| path.split_at(i));
    let pane = PaneId(id.parse().ok()?);
    Some(PaneUrl { port, pane, rest })
}

/// A pane URL plus the endpoint asked for, filed on `scope`'s board.
///
/// `base` is usually [`pane_url`]'s, but may be a shell expression that
/// expands to one — a hook installed checkout-wide cannot name a pane.
pub fn endpoint_url(base: &str, endpoint: Endpoint, scope: ArtifactScope) -> String {
    let url = format!("{}/{}", base.trim_end_matches('/'), endpoint.suffix());
    match scope {
        ArtifactScope::Workspace => format!("{url}?{WORKSPACE_QUERY}"),
        ArtifactScope::RepositoryBranch => url,
    }
}

/// The pane, endpoint and board a request target names: the path half of
/// [`endpoint_url`], as it arrives on the request line.
pub fn parse_request_target(target: &str) -> (Option<(PaneId, Endpoint)>, ArtifactScope) {
    let (route, query) = target.split_once('?').unwrap_or((target, ""));
    let scope = if query.split('&').any(|part| part == WORKSPACE_QUERY) {
        ArtifactScope::Workspace
    } else {
        ArtifactScope::RepositoryBranch
    };
    (parse_pane_path(route), scope)
}

/// `url` asking about the feature `slug` rather than the one the pane's
/// checkout is on.
pub fn feature_url(url: &str, slug: &str) -> String {
    let joiner = if url.contains('?') { '&' } else { '?' };
    format!("{url}{joiner}{FEATURE_QUERY}{slug}")
}

/// The feature a request target names, if it names one. Not checked here:
/// one this board does not have is the daemon's to refuse, by name, rather
/// than quietly read as naming none.
pub fn requested_feature(target: &str) -> Option<String> {
    let (_, query) = target.split_once('?')?;
    query
        .split('&')
        .find_map(|part| part.strip_prefix(FEATURE_QUERY))
        .filter(|slug| !slug.is_empty())
        .map(str::to_string)
}

/// The board [`ARTIFACT_SCOPE_VAR`] asks for, given its value. Only the
/// exact word crosses repositories; anything else is the repository board.
pub fn requested_scope(var: Option<&str>) -> ArtifactScope {
    if var == Some("workspace") {
        ArtifactScope::Workspace
    } else {
        ArtifactScope::RepositoryBranch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_endpoint() -> Vec<Endpoint> {
        let mut all = vec![
            Endpoint::Title,
            Endpoint::Checkout,
            Endpoint::Session,
            Endpoint::Comments,
            Endpoint::Decisions,
            Endpoint::Decide,
            Endpoint::DecisionChange,
            Endpoint::Features,
            Endpoint::Feature,
            Endpoint::Tasks,
            Endpoint::Diagrams,
            Endpoint::Telemetry,
        ];
        all.extend(Report::ALL.into_iter().map(Endpoint::Status));
        all
    }

    #[test]
    fn every_endpoint_survives_a_round_trip() {
        // The whole point of the module: what the helper writes is what the
        // daemon reads, for every endpoint there is.
        for endpoint in every_endpoint() {
            let path = pane_path(PaneId(7), endpoint);
            assert_eq!(
                parse_pane_path(&path),
                Some((PaneId(7), endpoint)),
                "{path}"
            );
        }
    }

    #[test]
    fn the_built_paths_are_the_documented_ones() {
        assert_eq!(pane_path(PaneId(3), Endpoint::Title), "/pane/3/title");
        assert_eq!(pane_path(PaneId(3), Endpoint::Checkout), "/pane/3/checkout");
        assert_eq!(pane_path(PaneId(3), Endpoint::Session), "/pane/3/session");
        assert_eq!(pane_path(PaneId(3), Endpoint::Comments), "/pane/3/comments");
        assert_eq!(
            pane_path(PaneId(3), Endpoint::Decisions),
            "/pane/3/decisions"
        );
        assert_eq!(pane_path(PaneId(3), Endpoint::Decide), "/pane/3/decide");
        assert_eq!(
            pane_path(PaneId(3), Endpoint::Status(Report::NeedsReview)),
            "/pane/3/status/needs-review"
        );
        assert_eq!(pane_prefix(PaneId(3)), "/pane/3");
    }

    #[test]
    fn a_path_that_is_not_a_pane_request_is_refused() {
        for path in [
            "/pane",
            "/pane/7",
            "/pane/seven/title",
            "/pane/7/status",
            "/pane/7/status/pondering",
            "/pane/7/title/extra",
            "/pane/7/delegate",
            "/pane/7/handoff",
            "/panes/7/title",
            "/status/idle",
            "",
        ] {
            assert_eq!(parse_pane_path(path), None, "{path}");
        }
    }

    #[test]
    fn every_endpoint_on_either_board_survives_the_trip_to_the_listener() {
        // What the installer and the helper build is what the hook server
        // reads off the request line, board included.
        let base = pane_url(4242, PaneId(7));
        for endpoint in every_endpoint() {
            for scope in [ArtifactScope::RepositoryBranch, ArtifactScope::Workspace] {
                let url = endpoint_url(&base, endpoint, scope);
                let parsed = parse_pane_url(&url).unwrap();
                assert_eq!((parsed.port, parsed.pane), (4242, PaneId(7)), "{url}");
                assert_eq!(parsed.base(), base, "{url}");
                let target = url.strip_prefix("http://127.0.0.1:4242").unwrap();
                assert_eq!(
                    parse_request_target(target),
                    (Some((PaneId(7), endpoint)), scope),
                    "{url}"
                );
            }
        }
    }

    #[test]
    fn the_built_urls_are_the_documented_ones() {
        assert_eq!(pane_url(4242, PaneId(9)), "http://127.0.0.1:4242/pane/9");
        assert_eq!(
            endpoint_url(
                "http://127.0.0.1:4242/pane/9",
                Endpoint::Status(Report::Idle),
                ArtifactScope::RepositoryBranch
            ),
            "http://127.0.0.1:4242/pane/9/status/idle"
        );
        assert_eq!(
            endpoint_url(
                "http://127.0.0.1:4242/pane/9/",
                Endpoint::Tasks,
                ArtifactScope::Workspace
            ),
            "http://127.0.0.1:4242/pane/9/tasks?scope=workspace"
        );
        assert_eq!(
            endpoint_url(
                "$ARGUS_HOOK_URL",
                Endpoint::Session,
                ArtifactScope::RepositoryBranch
            ),
            "$ARGUS_HOOK_URL/session"
        );
    }

    #[test]
    fn a_pane_url_keeps_whatever_follows_the_pane() {
        let parsed = parse_pane_url("http://127.0.0.1:4242/pane/1/status/idle").unwrap();
        assert_eq!(parsed.rest, "/status/idle");
        assert_eq!(parse_pane_url("http://127.0.0.1:4242/pane/1").unwrap().rest, "");
    }

    #[test]
    fn a_url_off_the_loopback_listener_is_not_a_pane_url() {
        for url in [
            "http://10.0.0.1:4242/pane/1",
            "http://localhost:4242/pane/1",
            "http://127.0.0.1/pane/1",
            "http://127.0.0.1:99999/pane/1",
            "https://127.0.0.1:4242/pane/1",
            "http://127.0.0.1:4242/panes/1",
            "http://127.0.0.1:4242/pane/one",
            "http://127.0.0.1:4242/pane/",
            "http://127.0.0.1:4242",
            "",
        ] {
            assert_eq!(parse_pane_url(url), None, "{url}");
        }
    }

    #[test]
    fn a_request_target_without_the_scope_query_is_on_the_repository_board() {
        assert_eq!(
            parse_request_target("/pane/7/tasks"),
            (Some((PaneId(7), Endpoint::Tasks)), ArtifactScope::RepositoryBranch)
        );
        assert_eq!(
            parse_request_target("/pane/7/tasks?scope=repository&x=1"),
            (Some((PaneId(7), Endpoint::Tasks)), ArtifactScope::RepositoryBranch)
        );
        assert_eq!(
            parse_request_target("/pane/7/tasks?x=1&scope=workspace"),
            (Some((PaneId(7), Endpoint::Tasks)), ArtifactScope::Workspace)
        );
    }

    #[test]
    fn a_feature_named_in_the_url_is_read_back_beside_the_scope() {
        let url = endpoint_url(
            "http://127.0.0.1:4242/pane/9",
            Endpoint::Tasks,
            ArtifactScope::Workspace,
        );
        let named = feature_url(&url, "protocol-handshake");
        let target = named.trim_start_matches("http://127.0.0.1:4242");
        assert_eq!(requested_feature(target).as_deref(), Some("protocol-handshake"));
        assert_eq!(parse_request_target(target).1, ArtifactScope::Workspace);

        let bare = feature_url("http://127.0.0.1:4242/pane/9/tasks", "x");
        assert!(bare.ends_with("/tasks?feature=x"), "{bare}");
        assert_eq!(requested_feature("/pane/9/tasks?scope=workspace"), None);
        assert_eq!(requested_feature("/pane/9/tasks?feature="), None);
    }

    #[test]
    fn only_the_exact_word_opts_into_the_workspace_board() {
        assert_eq!(requested_scope(Some("workspace")), ArtifactScope::Workspace);
        for var in [None, Some(""), Some("Workspace"), Some("repository")] {
            assert_eq!(requested_scope(var), ArtifactScope::RepositoryBranch, "{var:?}");
        }
    }

    #[test]
    fn a_leading_slash_is_optional() {
        assert_eq!(
            parse_pane_path("pane/7/title"),
            Some((PaneId(7), Endpoint::Title))
        );
    }

    #[test]
    fn every_report_maps_to_a_pane_status_and_back_to_its_wire_name() {
        for report in Report::ALL {
            assert_eq!(Report::parse(report.as_str()), Some(report));
            assert_eq!(report.status(), report.status(), "{report:?}");
        }
        assert_eq!(
            Report::parse("needs_review"),
            None,
            "underscores are not it"
        );
    }
}
