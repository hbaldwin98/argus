//! Requests about a checkout's Git and code: its worktrees and branches,
//! its review and history, its files and their search, and the agent
//! sessions recorded against it.

use argus_protocol::{ClientMsg, CodeVersion, ReviewAnchor, ReviewBase, SearchOptions};
use proptest::prelude::*;

use super::values::{checkout, checkout_path, commit, maybe_text, pane, repository, request_id, text};
use super::Known;

/// The harnesses that keep transcripts, and one nobody has.
const HARNESSES: &[&str] = &["claude", "codex", "no-such-harness"];

/// A branch name: a plain one, the fixture's own, or one Git refuses,
/// including one that reads as a flag.
fn branch() -> BoxedStrategy<String> {
    prop_oneof![
        4 => "[a-z][a-z0-9-]{0,10}",
        1 => Just("main".to_owned()),
        1 => Just("-x".to_owned()),
        1 => text(),
    ]
    .boxed()
}

fn base() -> BoxedStrategy<ReviewBase> {
    prop_oneof![Just(ReviewBase::Unstaged), Just(ReviewBase::Staged), Just(ReviewBase::Commit)].boxed()
}

fn version(k: &Known) -> BoxedStrategy<CodeVersion> {
    prop_oneof![
        Just(CodeVersion::WorkingTree),
        Just(CodeVersion::Index),
        Just(CodeVersion::Head),
        commit(k).prop_map(CodeVersion::Commit),
    ]
    .boxed()
}

fn line() -> BoxedStrategy<u32> {
    prop_oneof![4 => 0..8u32, 1 => any::<u32>()].boxed()
}

pub fn create_worktree(k: &Known) -> BoxedStrategy<ClientMsg> {
    (checkout(k), branch(), request_id())
        .prop_map(|(checkout, branch, request_id)| ClientMsg::CreateWorktree { checkout, branch, request_id })
        .boxed()
}

pub fn remove_checkout(k: &Known) -> BoxedStrategy<ClientMsg> {
    checkout(k).prop_map(|checkout| ClientMsg::RemoveCheckout { checkout }).boxed()
}

pub fn review(k: &Known) -> BoxedStrategy<ClientMsg> {
    (request_id(), checkout(k), base(), proptest::option::of(commit(k)))
        .prop_map(|(request_id, checkout, base, commit)| ClientMsg::Review { request_id, checkout, base, commit })
        .boxed()
}

pub fn review_context(k: &Known) -> BoxedStrategy<ClientMsg> {
    (request_id(), checkout(k), commit(k), checkout_path(k), line(), line())
        .prop_map(|(request_id, checkout, blob, path, start, end)| ClientMsg::ReviewContext {
            request_id,
            checkout,
            blob,
            path,
            start,
            end,
        })
        .boxed()
}

pub fn list_code(k: &Known) -> BoxedStrategy<ClientMsg> {
    (request_id(), checkout(k), version(k))
        .prop_map(|(request_id, checkout, version)| ClientMsg::ListCode { request_id, checkout, version })
        .boxed()
}

pub fn list_dir(k: &Known) -> BoxedStrategy<ClientMsg> {
    (request_id(), checkout(k), checkout_path(k))
        .prop_map(|(request_id, checkout, path)| ClientMsg::ListDir { request_id, checkout, path })
        .boxed()
}

pub fn read_code(k: &Known) -> BoxedStrategy<ClientMsg> {
    (request_id(), checkout(k), checkout_path(k), version(k))
        .prop_map(|(request_id, checkout, path, version)| ClientMsg::ReadCode { request_id, checkout, path, version })
        .boxed()
}

pub fn search_code(k: &Known) -> BoxedStrategy<ClientMsg> {
    let options = (any::<bool>(), any::<bool>(), any::<bool>(), prop_oneof![Just(String::new()), Just("*.rs".to_owned()), text()])
        .prop_map(|(regex, case, word, glob)| SearchOptions { regex, case, word, glob });
    let query = prop_oneof![3 => Just("answer".to_owned()), 1 => Just("(".to_owned()), 2 => text()];
    (request_id(), checkout(k), query, options)
        .prop_map(|(request_id, checkout, query, options)| ClientMsg::SearchCode { request_id, checkout, query, options })
        .boxed()
}

pub fn list_commits(k: &Known) -> BoxedStrategy<ClientMsg> {
    (request_id(), checkout(k))
        .prop_map(|(request_id, checkout)| ClientMsg::ListCommits { request_id, checkout })
        .boxed()
}

pub fn list_commit_files(k: &Known) -> BoxedStrategy<ClientMsg> {
    (checkout(k), commit(k))
        .prop_map(|(checkout, commit)| ClientMsg::ListCommitFiles { checkout, commit })
        .boxed()
}

pub fn list_sessions(k: &Known) -> BoxedStrategy<ClientMsg> {
    (request_id(), repository(k))
        .prop_map(|(request_id, repository)| ClientMsg::ListSessions { request_id, repository })
        .boxed()
}

pub fn read_session(k: &Known) -> BoxedStrategy<ClientMsg> {
    (request_id(), checkout(k), proptest::sample::select(HARNESSES), text())
        .prop_map(|(request_id, checkout, harness, session_id)| ClientMsg::ReadSession {
            request_id,
            checkout,
            harness: harness.to_owned(),
            session_id,
        })
        .boxed()
}

pub fn list_resumes() -> BoxedStrategy<ClientMsg> {
    request_id().prop_map(|request_id| ClientMsg::ListResumes { request_id }).boxed()
}

pub fn resume_session(k: &Known) -> BoxedStrategy<ClientMsg> {
    (request_id(), checkout(k), proptest::sample::select(HARNESSES), text())
        .prop_map(|(request_id, checkout, harness, session_id)| ClientMsg::ResumeSession {
            request_id,
            checkout,
            harness: harness.to_owned(),
            session_id,
        })
        .boxed()
}

pub fn review_comment(k: &Known) -> BoxedStrategy<ClientMsg> {
    let span = (proptest::option::of(line()), proptest::option::of(line()));
    let anchor = (base(), proptest::option::of(commit(k)), checkout_path(k), maybe_text(), span.clone(), span, proptest::collection::vec(text(), 0..3))
        .prop_map(|(base, commit, path, old_path, (old_start, old_end), (new_start, new_end), text)| ReviewAnchor {
            base,
            commit,
            path,
            old_path,
            old_start,
            old_end,
            new_start,
            new_end,
            text,
        });
    (checkout(k), pane(k), anchor, text(), request_id())
        .prop_map(|(checkout, recipient, anchor, body, request_id)| ClientMsg::ReviewComment {
            checkout,
            recipient,
            anchor: Box::new(anchor),
            body,
            request_id,
        })
        .boxed()
}

pub fn list_branches(k: &Known) -> BoxedStrategy<ClientMsg> {
    checkout(k).prop_map(|checkout| ClientMsg::ListBranches { checkout }).boxed()
}

pub fn list_files(k: &Known) -> BoxedStrategy<ClientMsg> {
    checkout(k).prop_map(|checkout| ClientMsg::ListFiles { checkout }).boxed()
}

pub fn switch_branch(k: &Known) -> BoxedStrategy<ClientMsg> {
    (checkout(k), branch()).prop_map(|(checkout, branch)| ClientMsg::SwitchBranch { checkout, branch }).boxed()
}

pub fn create_branch(k: &Known) -> BoxedStrategy<ClientMsg> {
    (checkout(k), branch()).prop_map(|(checkout, branch)| ClientMsg::CreateBranch { checkout, branch }).boxed()
}

pub fn delete_branch(k: &Known) -> BoxedStrategy<ClientMsg> {
    (checkout(k), branch(), any::<bool>())
        .prop_map(|(checkout, branch, force)| ClientMsg::DeleteBranch { checkout, branch, force })
        .boxed()
}

/// The fixture has no remote, so a fetch or a pull is Git's refusal, run
/// for real against a repository nothing else uses.
pub fn fetch(k: &Known) -> BoxedStrategy<ClientMsg> {
    checkout(k).prop_map(|checkout| ClientMsg::Fetch { checkout }).boxed()
}

pub fn pull(k: &Known) -> BoxedStrategy<ClientMsg> {
    checkout(k).prop_map(|checkout| ClientMsg::Pull { checkout }).boxed()
}
