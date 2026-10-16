//! What each topic's channel keeps and replays: the tree's order.

use super::*;

fn report() -> AgentTelemetry {
    AgentTelemetry { context_tokens: Some(1), ..Default::default() }
}

#[test]
fn a_report_published_after_a_tree_is_never_sent_before_it() {
    let pushes = Pushes::new();
    let (_, mut rx) = pushes.follow_tree(Vec::new);

    pushes.tree(Vec::new);
    pushes.telemetry(|| Some((PaneId(1), report())));

    assert!(matches!(rx.try_recv(), Ok(TreeNews::Tree(_))));
    assert!(matches!(rx.try_recv(), Ok(TreeNews::Telemetry { pane: PaneId(1), .. })));
}

#[test]
fn a_tree_read_before_a_report_is_sent_before_it() {
    // The tree is read, then a report is merged and sent while the tree is
    // still on its way: without the tree's order the report would land
    // first and the tree after it would wind the numbers back.
    let pushes = std::sync::Arc::new(Pushes::new());
    let (_, mut rx) = pushes.follow_tree(Vec::new);
    let read = std::sync::Arc::new(std::sync::Barrier::new(2));

    let tree = {
        let (pushes, read) = (pushes.clone(), read.clone());
        std::thread::spawn(move || {
            pushes.tree(|| {
                read.wait();
                std::thread::sleep(std::time::Duration::from_millis(100));
                Vec::new()
            })
        })
    };
    read.wait();
    pushes.telemetry(|| Some((PaneId(1), report())));
    tree.join().unwrap();

    assert!(matches!(rx.try_recv(), Ok(TreeNews::Tree(_))), "the tree read first is sent first");
    assert!(matches!(rx.try_recv(), Ok(TreeNews::Telemetry { .. })));
}

#[test]
fn a_tree_published_while_one_is_followed_is_neither_lost_nor_sent_before_it() {
    // A tree is published while a connection reads its opening tree: taken
    // outside the order, it would land between the read and the subscribe
    // and never reach the connection.
    let pushes = std::sync::Arc::new(Pushes::new());
    let read = std::sync::Arc::new(std::sync::Barrier::new(2));

    let follow = {
        let (pushes, read) = (pushes.clone(), read.clone());
        std::thread::spawn(move || {
            pushes.follow_tree(|| {
                read.wait();
                std::thread::sleep(std::time::Duration::from_millis(100));
                Vec::new()
            })
        })
    };
    read.wait();
    let id = argus_protocol::ProjectId(7);
    let published = ProjectInfo { id, name: "published".into(), root: None, repositories: Vec::new() };
    pushes.tree(|| vec![published]);
    let (opening, mut rx) = follow.join().unwrap();

    assert!(opening.is_empty(), "the opening tree is the one read before");
    let Ok(TreeNews::Tree(tree)) = rx.try_recv() else { panic!("the tree published meanwhile follows it") };
    assert_eq!(tree.iter().map(|p| p.id).collect::<Vec<_>>(), [id]);
    assert!(rx.try_recv().is_err(), "and nothing else");
}
