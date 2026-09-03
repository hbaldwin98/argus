//! The pane API's URL grammar.
//!
//! The daemon parses these paths and `argus-hook` builds them, in separate
//! binaries that never share a type unless it lives here. Written twice they
//! drift silently: a new endpoint on one side compiles perfectly and fails
//! at runtime against the other. Written once they cannot.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

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
    Context,
    /// The one write an agent has: a `TodoWrite` against its own checkout's
    /// note. Separate from `Context` because reading is unconditional and
    /// this is not — the daemon refuses it unless the project allows it.
    Todo,
    /// The decision board of the feature this pane's checkout is on, read
    /// whole. A tree with its roots cut off explains nothing, so it is
    /// never trimmed inside a feature — but it is scoped to one, because a
    /// project-wide board is a pile rather than a reference.
    Decisions,
    /// One decision appended to that board.
    Decide,
    /// The project's features, and which one this checkout is on.
    Features,
    /// A change to that: opening a feature, pointing this checkout at one,
    /// or adding a paragraph to its document.
    Feature,
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
            Endpoint::Context => Cow::Borrowed("context"),
            Endpoint::Todo => Cow::Borrowed("todo"),
            Endpoint::Decisions => Cow::Borrowed("decisions"),
            Endpoint::Decide => Cow::Borrowed("decide"),
            Endpoint::Features => Cow::Borrowed("features"),
            Endpoint::Feature => Cow::Borrowed("feature"),
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
        "context" => Endpoint::Context,
        "todo" => Endpoint::Todo,
        "decisions" => Endpoint::Decisions,
        "decide" => Endpoint::Decide,
        "features" => Endpoint::Features,
        "feature" => Endpoint::Feature,
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((pane, endpoint))
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
            Endpoint::Context,
            Endpoint::Todo,
            Endpoint::Decisions,
            Endpoint::Decide,
            Endpoint::Features,
            Endpoint::Feature,
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
        assert_eq!(pane_path(PaneId(3), Endpoint::Context), "/pane/3/context");
        assert_eq!(pane_path(PaneId(3), Endpoint::Todo), "/pane/3/todo");
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
