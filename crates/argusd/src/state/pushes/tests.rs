//! What each topic's channel keeps and replays: the tree's order, and the
//! latest board of each key.

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

fn decisions(repository: u64) -> BoardNews {
    BoardNews::Decisions(Box::new(DecisionBoard {
        repositories: vec![RepositoryId(repository)],
        project: Some(ProjectId(repository)),
        ..Default::default()
    }))
}

/// Repository `repository`'s task list for one feature, `version` naming
/// which push of it this is.
fn tasks(repository: u64, version: usize) -> BoardNews {
    BoardNews::Tasks(Box::new(TaskList {
        project_name: format!("v{version}"),
        feature: Some("pushes".into()),
        tasks: Vec::new(),
        repositories: vec![RepositoryId(repository)],
    }))
}

/// What each kept board is, as `(kind, repository, version)`.
fn kept(boards: &[BoardNews]) -> Vec<(&'static str, u64, String)> {
    boards
        .iter()
        .map(|board| match board {
            BoardNews::Decisions(b) => ("decisions", b.repositories[0].0, String::new()),
            BoardNews::Tasks(l) => ("tasks", l.repositories[0].0, l.project_name.clone()),
            BoardNews::Diagrams(l) => ("diagrams", l.repositories[0].0, l.project_name.clone()),
        })
        .collect()
}

#[test]
fn a_board_pushed_before_a_flood_is_still_there_for_a_client_that_fell_behind() {
    // A lagged board push used to be dropped with nothing to bring the
    // client level: repository A's board stayed stale until its next change.
    let pushes = Pushes::new();
    let mut behind = pushes.subscribe_boards();

    pushes.board(decisions(1));
    for version in 0..BOARDS_DEPTH + 10 {
        pushes.board(tasks(2, version));
    }

    assert!(matches!(behind.try_recv(), Err(broadcast::error::TryRecvError::Lagged(_))));
    let last = format!("v{}", BOARDS_DEPTH + 9);
    assert_eq!(kept(&pushes.follow_boards().0), [("decisions", 1, String::new()), ("tasks", 2, last)]);
}

#[test]
fn a_catch_up_never_sends_a_board_older_than_the_one_it_replayed() {
    let pushes = Pushes::new();
    let mut behind = pushes.subscribe_boards();
    for version in 0..BOARDS_DEPTH + 10 {
        pushes.board(tasks(2, version));
    }
    assert!(matches!(behind.try_recv(), Err(broadcast::error::TryRecvError::Lagged(_))));

    // What the connection does on a lag: the latest, then a fresh receiver.
    let (latest, mut fresh) = pushes.follow_boards();
    assert_eq!(kept(&latest), [("tasks", 2, format!("v{}", BOARDS_DEPTH + 9))]);
    assert!(fresh.try_recv().is_err(), "nothing older follows the replay");
    pushes.board(tasks(2, 1000));
    let BoardNews::Tasks(list) = fresh.try_recv().unwrap() else { panic!("the next push") };
    assert_eq!(list.project_name, "v1000");
}

#[test]
fn a_kept_board_is_replaced_by_the_next_push_of_its_key() {
    let pushes = Pushes::new();
    pushes.board(tasks(2, 1));
    pushes.board(decisions(1));
    pushes.board(tasks(2, 2));
    assert_eq!(
        kept(&pushes.follow_boards().0),
        [("decisions", 1, String::new()), ("tasks", 2, "v2".into())],
        "replaced, and moved behind the boards pushed since"
    );
}

#[test]
fn only_so_many_boards_are_kept_for_catching_up() {
    let pushes = Pushes::new();
    for repository in 0..=REMEMBERED_BOARDS as u64 {
        pushes.board(decisions(repository));
    }
    let kept = kept(&pushes.follow_boards().0);
    assert_eq!(kept.len(), REMEMBERED_BOARDS);
    assert_eq!(kept[0].1, 1, "the oldest is gone");
}
