//! Requests about the panel's rows: workspaces, projects and their
//! repositories, and the directory browser that finds them.
//!
//! Every path is under the host's directory ([`dir_under_root`]): adding
//! a project scans what it names and `init` creates it, and the browser
//! reads it, so a path anywhere else would be the test machine's.

use argus_protocol::ClientMsg;
use proptest::prelude::*;

use super::values::{dir_under_root, maybe_text, project, repository, request_id, text, workspace, ROOT};
use super::Known;

fn skip() -> BoxedStrategy<Vec<String>> {
    proptest::collection::vec(prop_oneof![Just("target".to_owned()), Just("repo".to_owned()), text()], 0..3).boxed()
}

pub fn open_workspace(k: &Known) -> BoxedStrategy<ClientMsg> {
    workspace(k).prop_map(|workspace| ClientMsg::OpenWorkspace { workspace }).boxed()
}

pub fn create_workspace() -> BoxedStrategy<ClientMsg> {
    prop_oneof![1 => Just("default".to_owned()), 4 => text()]
        .prop_map(|name| ClientMsg::CreateWorkspace { name })
        .boxed()
}

pub fn remove_project(k: &Known) -> BoxedStrategy<ClientMsg> {
    project(k).prop_map(|project| ClientMsg::RemoveProject { project }).boxed()
}

pub fn remove_repository(k: &Known) -> BoxedStrategy<ClientMsg> {
    repository(k).prop_map(|repository| ClientMsg::RemoveRepository { repository }).boxed()
}

pub fn add_project() -> BoxedStrategy<ClientMsg> {
    (dir_under_root(), request_id(), maybe_text(), skip(), any::<bool>())
        .prop_map(|(path, request_id, name, skip, init)| ClientMsg::AddProject { path, request_id, name, skip, init })
        .boxed()
}

pub fn preview_project() -> BoxedStrategy<ClientMsg> {
    (request_id(), dir_under_root(), skip())
        .prop_map(|(request_id, path, skip)| ClientMsg::PreviewProject { request_id, path, skip })
        .boxed()
}

/// An empty path is where a browse starts, which the daemon decides; it
/// only reads, so it is the one path here outside the host's directory.
pub fn list_directories() -> BoxedStrategy<ClientMsg> {
    (request_id(), prop_oneof![4 => dir_under_root(), 1 => Just(String::new())])
        .prop_map(|(request_id, path)| ClientMsg::ListDirectories { request_id, path })
        .boxed()
}

pub fn add_repository(k: &Known) -> BoxedStrategy<ClientMsg> {
    (project(k), dir_under_root(), request_id())
        .prop_map(|(project, path, request_id)| ClientMsg::AddRepository { project, path, request_id })
        .boxed()
}

/// Its path must be absolute, so a relative one is the daemon's refusal,
/// never a directory made wherever the daemon's working directory is.
pub fn init_repository(k: &Known) -> BoxedStrategy<ClientMsg> {
    let path = prop_oneof![
        4 => "[a-z]{1,8}".prop_map(|name| format!("{ROOT}/made/{name}")),
        1 => dir_under_root(),
    ];
    (project(k), path, request_id())
        .prop_map(|(project, path, request_id)| ClientMsg::InitRepository { project, path, request_id })
        .boxed()
}
