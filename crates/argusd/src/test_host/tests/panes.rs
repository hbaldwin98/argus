//! A pane's life as a client lives it: started, watched, typed into,
//! resized, renamed and killed.

use argus_protocol::{ClientMsg, Created, PaneId, ServerMsg};

use crate::test_host::transcript::Transcript;
use crate::test_host::Host;

#[tokio::test]
async fn a_pane_from_start_to_end() {
    let host = Host::start().await;
    let mut t = Transcript::greeted(&host).await;

    let made = t
        .ask_each(
            ClientMsg::SpawnShell { checkout: host.checkout(), request_id: 1 },
            &[&|m| matches!(m, ServerMsg::Created { .. }), &|m| matches!(m, ServerMsg::Tree(_))],
        )
        .await;
    let pane = made
        .iter()
        .find_map(|m| match m {
            ServerMsg::Created { created: Some(Created::Pane(pane)), .. } => Some(*pane),
            _ => None,
        })
        .expect("a shell request is answered with its pane");
    let started = host.daemon.processes.started(pane).expect("the pane's process was started");
    t.note(&format!(
        "started {} in {}",
        started.program.as_deref().unwrap_or("the default shell"),
        if started.cwd == host.repo() { "the checkout" } else { "somewhere else" },
    ));

    t.ask(ClientMsg::Subscribe { pane }, |m| matches!(m, ServerMsg::PaneRows { .. })).await;
    t.tell(ClientMsg::Input { pane, bytes: b"ls\r".to_vec() }).await;
    t.tell(ClientMsg::Paste { pane, text: "two\nlines".into() }).await;
    let written = host.daemon.processes.written(pane);
    t.note(&format!("the pane's process read {:?}", String::from_utf8_lossy(&written)));

    // A watched pane's new size reaches its watcher in the next snapshot.
    t.tell(ClientMsg::Resize { pane, rows: 5, cols: 4 }).await;
    t.ask(ClientMsg::Subscribe { pane }, |m| matches!(m, ServerMsg::PaneRows { .. })).await;

    t.ask(ClientMsg::RenamePane { pane, title: "build".into() }, |m| matches!(m, ServerMsg::Tree(_)))
        .await;
    // A pane that is not there is an error, not a dropped connection.
    t.ask(ClientMsg::Subscribe { pane: PaneId(999) }, |m| matches!(m, ServerMsg::Error { .. }))
        .await;

    t.ask(ClientMsg::Kill { pane }, |m| matches!(m, ServerMsg::Tree(_))).await;
    t.note(&format!("the pane's process was killed: {}", host.daemon.processes.killed(pane)));
    // The kill ends the process; its end lands on a tree the pane already
    // left, so no client is sent anything for it.
    t.note("the pane's process exits");
    host.daemon.exit(pane, None);
    t.quiet().await;

    insta::assert_snapshot!(t.finish());
    host.stop().await;
}
