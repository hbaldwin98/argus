//! The values requests are made of: ids that are mostly the fixture's and
//! sometimes nobody's, short text that is sometimes not ASCII, and paths
//! that stay under the host's directory.
//!
//! A path is generated against [`ROOT`] and filled in when the request is
//! sent ([`fill_in`]), since every host's directory is a new temp dir and a
//! strategy is built once. Paths never leave that directory: a request
//! that creates or scans one would otherwise do it on the machine running
//! the tests.

use std::path::Path;

use argus_protocol::{CheckoutId, ClientMsg, PaneId, ProjectId, RepositoryId, WorkspaceId};
use proptest::prelude::*;

use super::Known;

/// Stands for the host's directory in a generated path.
pub const ROOT: &str = "@ROOT@";

/// One in this many ids is nobody's, so the daemon's refusal of a stale or
/// made-up id is exercised beside the fixture's own.
const STRANGER: u32 = 5;

fn id(known: u64) -> BoxedStrategy<u64> {
    prop_oneof![
        STRANGER - 1 => Just(known),
        1 => prop_oneof![0..32u64, any::<u64>()],
    ]
    .boxed()
}

pub fn pane(k: &Known) -> BoxedStrategy<PaneId> {
    id(k.pane.0).prop_map(PaneId).boxed()
}

pub fn checkout(k: &Known) -> BoxedStrategy<CheckoutId> {
    id(k.checkout.0).prop_map(CheckoutId).boxed()
}

pub fn project(k: &Known) -> BoxedStrategy<ProjectId> {
    id(k.project.0).prop_map(ProjectId).boxed()
}

pub fn repository(k: &Known) -> BoxedStrategy<RepositoryId> {
    id(k.repository.0).prop_map(RepositoryId).boxed()
}

pub fn workspace(k: &Known) -> BoxedStrategy<WorkspaceId> {
    id(k.workspace.0).prop_map(WorkspaceId).boxed()
}

/// A board row's number: the fixture's, or one that is not there.
pub fn row(known: i64) -> BoxedStrategy<i64> {
    prop_oneof![
        STRANGER - 1 => Just(known),
        1 => prop_oneof![-2..8i64, any::<i64>()],
    ]
    .boxed()
}

/// The fixture's feature, or a slug no board files.
pub fn slug(k: &Known) -> BoxedStrategy<String> {
    prop_oneof![STRANGER - 1 => Just(k.feature.clone()), 1 => text()].boxed()
}

/// A request's id: zero, which asks for no answer, or one, which
/// [`super::name_requests`] makes unique within its sequence.
pub fn request_id() -> BoxedStrategy<u64> {
    prop_oneof![1 => Just(0u64), 4 => Just(1u64)].boxed()
}

/// Short text: plain words mostly, sometimes empty, sometimes any
/// printable Unicode.
pub fn text() -> BoxedStrategy<String> {
    prop_oneof![
        6 => "[a-z][a-z ]{0,15}",
        1 => Just(String::new()),
        2 => "\\PC{0,12}",
    ]
    .boxed()
}

pub fn maybe_text() -> BoxedStrategy<Option<String>> {
    proptest::option::of(text()).boxed()
}

/// A path in the checkout, as a client names one: a fixture file, one
/// that is not there, or one that tries to leave.
pub fn checkout_path(k: &Known) -> BoxedStrategy<String> {
    prop_oneof![
        4 => proptest::sample::select(k.files.clone()),
        1 => "[a-z]{1,8}(/[a-z]{1,8}){0,2}(\\.rs)?",
        1 => Just("../outside.txt".to_owned()),
        1 => Just(String::new()),
    ]
    .boxed()
}

/// An absolute directory under the host's: its repository, one that does
/// not exist yet, or the directory itself.
pub fn dir_under_root() -> BoxedStrategy<String> {
    proptest::sample::select(vec!["", "/repo", "/repo/src", "/other", "/missing/deeper"])
        .prop_map(|rel| format!("{ROOT}{rel}"))
        .boxed()
}

/// A commit: the fixture's head, or a name Git will not find.
pub fn commit(k: &Known) -> BoxedStrategy<String> {
    prop_oneof![
        3 => Just(k.commit.clone()),
        1 => "[0-9a-f]{7,40}",
        1 => text(),
    ]
    .boxed()
}

/// `msg` with every [`ROOT`] replaced by `root`, written with forward
/// slashes, which every platform's paths accept.
pub fn fill_in(msg: &ClientMsg, root: &Path) -> ClientMsg {
    let root = root.to_string_lossy().replace('\\', "/");
    let json = serde_json::to_string(msg).expect("a request serializes");
    if !json.contains(ROOT) {
        return msg.clone();
    }
    let filled = json.replace(ROOT, &root.replace('"', "\\\""));
    serde_json::from_str(&filled).expect("a request with its root filled in still reads")
}
