//! A checkout as a client reads and changes it: a change reviewed with its
//! history and a file, and a worktree made for a branch, worked in, and
//! removed while another client watches.

use argus_protocol::{ClientMsg, CodeVersion, Created, ReviewBase, ServerMsg};

use crate::test_host::transcript::Transcript;
use crate::test_host::Host;

#[tokio::test]
async fn a_change_reviewed() {
    let host = Host::start().await;
    std::fs::write(host.repo().join("src/lib.rs"), "pub fn answer() -> u32 {\n    43\n}\n").unwrap();
    std::fs::write(host.repo().join("notes.txt"), "untracked\n").unwrap();
    let checkout = host.checkout();
    let mut t = Transcript::greeted(&host).await;

    t.ask(
        ClientMsg::Review { request_id: 1, checkout, base: ReviewBase::Unstaged, commit: None },
        |m| matches!(m, ServerMsg::Review(_)),
    )
    .await;
    t.ask(ClientMsg::ListCommits { request_id: 2, checkout }, |m| matches!(m, ServerMsg::Commits { .. }))
        .await;
    let read = |path: &str| ClientMsg::ReadCode { request_id: 3, checkout, path: path.into(), version: CodeVersion::Head };
    t.ask(read("src/lib.rs"), |m| matches!(m, ServerMsg::Code(_))).await;
    // A path that leaves the checkout is refused.
    t.ask(read("../outside.txt"), |m| matches!(m, ServerMsg::Refused(_))).await;

    insta::assert_snapshot!(t.finish());
    host.stop().await;
}

#[tokio::test]
async fn a_worktree_worked_in_and_removed() {
    let host = Host::start().await;
    let mut t = Transcript::greeted(&host).await;
    // Every client is told of a change, not only the one that asked.
    let mut other = Transcript::greeted(&host).await;
    let created: &dyn Fn(&ServerMsg) -> bool = &|m| matches!(m, ServerMsg::Created { .. });
    let tree: &dyn Fn(&ServerMsg) -> bool = &|m| matches!(m, ServerMsg::Tree(_));

    let made = t
        .ask_each(
            ClientMsg::CreateWorktree { checkout: host.checkout(), branch: "side".into(), request_id: 1 },
            &[created, tree],
        )
        .await;
    let worktree = made
        .iter()
        .find_map(|m| match m {
            ServerMsg::Created { created: Some(Created::Checkout(id)), .. } => Some(*id),
            _ => None,
        })
        .expect("a worktree request is answered with its checkout");
    let made = t
        .ask_each(ClientMsg::SpawnShell { checkout: worktree, request_id: 2 }, &[created, tree])
        .await;
    let pane = made
        .iter()
        .find_map(|m| match m {
            ServerMsg::Created { created: Some(Created::Pane(pane)), .. } => Some(*pane),
            _ => None,
        })
        .expect("a shell request is answered with its pane");

    // Removed with a shell still open in it. Git's removal is off the
    // message loop, so the step waits for the tree without the worktree.
    t.ask(ClientMsg::RemoveCheckout { checkout: worktree }, |m| match m {
        ServerMsg::Tree(projects) => projects[0].repositories[0].checkouts.len() == 1,
        _ => false,
    })
    .await;
    t.note(&format!("the shell's process was killed: {}", host.daemon.processes.killed(pane)));
    t.note(&format!("the worktree's directory is still there: {}", host.repo().join(".argus/worktrees/side").exists()));

    t.note("what the other client was sent meanwhile:");
    let mut text = t.finish();
    other.quiet().await;
    text.push_str(&other.finish());
    insta::assert_snapshot!(text);
    host.stop().await;
}
