//! What each client message does, sent the way a connection hands it
//! on, through a connection with no socket.

use super::*;
use crate::conn::tests::Harness;
use crate::review_queue::ReviewAsk;
use argus_protocol::transcript::Conversation;
use argus_protocol::{CheckoutId, PaneId, RepositoryId, ReviewBase};

#[test]
fn a_named_request_is_answered_with_what_it_made() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::AddProject {
        path: other.path().to_string_lossy().to_string(),
        request_id: 3,
        name: None,
        skip: Vec::new(),
        init: false,
    });

    let made = h.replies().into_iter().find_map(|reply| match reply {
        ServerMsg::Created {
            request_id: 3,
            created: Some(argus_protocol::Created::Project(id)),
        } => Some(id),
        _ => None,
    });
    let made = made.expect("the named request is answered");
    assert!(h.daemon.snapshot().iter().any(|p| p.id == made));
}

#[test]
fn an_unnamed_request_is_not_answered() {
    // What a client from before named requests sends: it could not
    // read the answer, so it is never sent one.
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::AddProject {
        path: other.path().to_string_lossy().to_string(),
        request_id: 0,
        name: None,
        skip: Vec::new(),
        init: false,
    });
    assert!(!h
        .replies()
        .iter()
        .any(|reply| matches!(reply, ServerMsg::Created { .. })));
}

/// What the daemon answers request `id` with, waiting past anything else
/// it sends on the way.
async fn answer_to(h: &mut Harness, id: u64) -> Option<argus_protocol::Created> {
    loop {
        let reply = tokio::time::timeout(std::time::Duration::from_secs(10), h.rx.recv())
            .await
            .expect("an answer should arrive");
        if let Some(ServerMsg::Created { request_id, created }) = reply {
            if request_id == id {
                return created;
            }
        }
    }
}

#[tokio::test]
async fn a_new_repository_is_made_and_added_as_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let fresh = parent.path().join("fresh");
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::AddProject {
        path: fresh.to_string_lossy().to_string(),
        request_id: 5,
        name: Some("scratch".into()),
        skip: Vec::new(),
        init: true,
    });
    let Some(argus_protocol::Created::Project(made)) = answer_to(&mut h, 5).await else {
        panic!("the project is made");
    };
    assert!(fresh.join(".git").is_dir(), "git init ran");
    let project = h.daemon.snapshot().into_iter().find(|p| p.id == made).unwrap();
    assert_eq!((project.name.as_str(), project.repositories.len()), ("scratch", 1));
}

#[tokio::test]
async fn a_new_repository_that_already_exists_keeps_its_history() {
    let dir = tempfile::tempdir().unwrap();
    let existing = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(existing.path()).unwrap();
    let tree = repo.find_tree(repo.index().unwrap().write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    let head = Some(repo.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[]).unwrap());
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::AddProject {
        path: existing.path().to_string_lossy().to_string(),
        request_id: 6,
        name: Some("kept".into()),
        skip: Vec::new(),
        init: true,
    });
    assert!(matches!(answer_to(&mut h, 6).await, Some(argus_protocol::Created::Project(_))));
    let after = git2::Repository::open(existing.path()).unwrap().head().unwrap().target();
    assert_eq!(after, head);
}

#[tokio::test]
async fn a_preview_answers_by_its_request() {
    let dir = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir(folder.path().join("web")).unwrap();
    git2::Repository::init(folder.path().join("web")).unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::PreviewProject {
        request_id: 7,
        path: folder.path().to_string_lossy().to_string(),
        skip: Vec::new(),
    });
    let reply = tokio::time::timeout(std::time::Duration::from_secs(10), h.rx.recv())
        .await
        .expect("a preview should arrive");
    let Some(ServerMsg::ProjectPreview(preview)) = reply else {
        panic!("not a preview: {reply:?}");
    };
    assert_eq!((preview.request_id, preview.repositories), (7, vec!["web".to_string()]));
}

#[tokio::test]
async fn a_refused_named_request_is_answered_with_nothing_and_still_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::SpawnShell {
        checkout: CheckoutId(9999),
        request_id: 8,
    });
    let answer = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
        .await
        .expect("an answer should arrive");
    assert!(
        matches!(
            answer,
            Some(ServerMsg::Created {
                request_id: 8,
                created: None
            })
        ),
        "{answer:?}"
    );
    let error = h.error().await;
    assert!(error.contains("no such checkout"), "{error}");
}

#[tokio::test]
async fn a_note_a_decision_and_its_answer_reach_the_board_from_a_client() {
    use argus_protocol::{Capture, DecisionChange, DecisionState, DecisionWrite, FeatureAction};

    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let (project, checkout) = (h.daemon.snapshot()[0].id, h.checkout());
    let mut boards = h.daemon.subscribe_decisions();
    let mut lists = h.daemon.subscribe_tasks();

    let note = "Bound the queue\nRefuse when full.\n- measure the backlog\n? which bound\n  a count";
    h.send(ClientMsg::Feature {
        project,
        checkout,
        action: FeatureAction::Capture {
            into: None,
            capture: Box::new(Capture::parse(note)),
        },
        request_id: 0,
    });
    let board = boards.try_recv().expect("the feature a note opened is pushed");
    let feature = board.features[0].clone();
    assert_eq!((feature.number, feature.body.as_str()), (1, "Refuse when full."));
    let question = board.decisions[0].clone();
    assert!(question.open() && question.options == ["a count"]);
    let list = lists.try_recv().expect("and so are its tasks");
    assert_eq!((list.feature.as_deref(), list.tasks.len()), (Some(feature.slug.as_str()), 1));

    h.send(ClientMsg::Feature {
        project,
        checkout,
        action: FeatureAction::Capture {
            into: Some(feature.slug.clone()),
            capture: Box::new(Capture::parse("any first line\n- drain it")),
        },
        request_id: 0,
    });
    let board = boards.try_recv().unwrap();
    assert_eq!(board.features.len(), 1, "a note added to a feature opens none");
    assert_eq!(board.features[0].title, "Bound the queue");
    assert_eq!(lists.try_recv().unwrap().tasks.len(), 2);

    h.send(ClientMsg::Decide {
        project,
        checkout,
        feature: feature.slug.clone(),
        write: DecisionWrite {
            chose: "refuse rather than drop".into(),
            state: DecisionState::Rejected,
            task: Some(list.tasks[0].id),
            ..Default::default()
        },
    });
    let board = boards.try_recv().unwrap();
    assert_eq!(board.decisions[1].state, DecisionState::Rejected);
    assert_eq!(board.decisions[1].task, Some(list.tasks[0].id));

    h.send(ClientMsg::ChangeDecision {
        project,
        checkout,
        feature: feature.slug.clone(),
        change: DecisionChange::Resolve {
            id: question.id,
            answer: "a count".into(),
            because: None,
        },
    });
    assert!(boards.try_recv().unwrap().decisions[0].stands());
    assert!(h.replies().is_empty(), "each is answered by the push, to every client");

    // A refusal is reported and pushes nothing: the panel keeps
    // showing what is true.
    h.send(ClientMsg::ChangeDecision {
        project,
        checkout,
        feature: feature.slug.clone(),
        change: DecisionChange::Resolve {
            id: question.id,
            answer: "a byte size".into(),
            because: None,
        },
    });
    let error = h.error().await;
    assert!(error.contains("not an open question"), "{error}");
    h.send(ClientMsg::Feature {
        project,
        checkout,
        action: FeatureAction::Capture {
            into: None,
            capture: Box::new(Capture::parse("Half a note\nstray prose\n- a task\n  stray prose")),
        },
        request_id: 0,
    });
    let error = h.error().await;
    assert!(error.contains("some lines read as nothing"), "{error}");
    h.send(ClientMsg::Decide {
        project,
        checkout: CheckoutId(9999),
        feature: feature.slug,
        write: DecisionWrite { chose: "x".into(), ..Default::default() },
    });
    let error = h.error().await;
    assert!(error.contains("checkout is not in this project"), "{error}");
    assert!(boards.try_recv().is_err());
}

#[tokio::test]
async fn every_git_mutation_reports_its_refusal() {
    // Seven of these go through `spawn_reporting` and DeleteBranch through
    // a task of its own, so this is the test that says each is wired to
    // report: a bogus id has to come back as a message rather than as a
    // keypress that did nothing.
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let gone = CheckoutId(9999);
    let branch = || "nope".to_string();

    let sent = [
        ClientMsg::SwitchBranch {
            checkout: gone,
            branch: branch(),
        },
        ClientMsg::CreateBranch {
            checkout: gone,
            branch: branch(),
        },
        ClientMsg::DeleteBranch {
            checkout: gone,
            branch: branch(),
            force: false,
        },
        ClientMsg::Fetch { checkout: gone },
        ClientMsg::Pull { checkout: gone },
        ClientMsg::CreateWorktree {
            checkout: gone,
            branch: branch(),
            request_id: 0,
        },
        ClientMsg::RemoveCheckout { checkout: gone },
        // Refused before any subprocess, as a relative path.
        ClientMsg::InitRepository {
            project: argus_protocol::ProjectId(9999),
            path: "nope".into(),
            request_id: 0,
        },
    ];
    let expected = sent.len();
    for msg in sent {
        h.send(msg);
    }

    let mut errors = Vec::new();
    for _ in 0..expected {
        errors.push(h.error().await);
    }
    let gone = errors.iter().filter(|e| e.contains("no such checkout")).count();
    assert_eq!(gone, expected - 1, "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("must be absolute")), "{errors:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn a_blocked_pane_launch_does_not_block_the_connection_task() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let started = std::time::Instant::now();

    let reply = dispatch::Reply::new(&tx, Asked::of(&ClientMsg::SpawnConsole { request_id: 0 }), false);
    dispatch::spawn_pane(&reply, || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(PaneId(1))
    })
    .unwrap();

    assert!(
        started.elapsed() < std::time::Duration::from_millis(100),
        "pane launch held the connection task for {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn subscribing_sends_a_snapshot_and_arms_the_damage_stream() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    let pane = h.daemon.spawn_shell(checkout).unwrap();

    h.send(ClientMsg::Subscribe { pane });

    assert!(matches!(
        h.replies().first(),
        Some(ServerMsg::PaneSnapshot { .. })
    ));
    assert!(
        h.conn.subs.tasks.contains_key(&pane),
        "damage must flow after a subscribe"
    );

    h.send(ClientMsg::Unsubscribe { pane });
    assert!(!h.conn.subs.tasks.contains_key(&pane));

    let _ = h.daemon.close_pane(pane);
}

#[tokio::test]
async fn history_is_answered_in_the_form_the_client_reads() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let pane = h.daemon.spawn_shell(h.checkout()).unwrap();

    h.send(ClientMsg::Scrollback {
        pane,
        offset: 1,
        top: None,
    });
    assert!(matches!(
        h.replies().first(),
        Some(ServerMsg::ScrollbackRows { .. })
    ));

    h.conn.subs.runs = true;
    h.send(ClientMsg::Scrollback {
        pane,
        offset: 1,
        top: None,
    });
    assert!(matches!(
        h.replies().first(),
        Some(ServerMsg::ScrollbackRuns { rows, top: Some(_), .. }) if *rows > 0
    ), "and numbers its first row, for the next request to ask by");

    let _ = h.daemon.close_pane(pane);
}

#[tokio::test]
async fn subscribing_to_a_pane_that_is_gone_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::Subscribe { pane: PaneId(9999) });
    assert!(!h.error().await.is_empty());
    assert!(h.conn.subs.tasks.is_empty());
}

#[tokio::test]
async fn a_review_request_answers_with_that_checkouts_diff() {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    drop(repo);

    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    h.send(ClientMsg::Review {
        request_id: 42,
        checkout,
        base: ReviewBase::Unstaged,
        commit: None,
    });

    // The diff runs on a blocking thread, so the reply is not immediate.
    let reply = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
        .await
        .expect("the diff should arrive")
        .expect("channel open");
    match reply {
        ServerMsg::Review(r) => {
            assert_eq!(r.checkout, checkout);
            assert_eq!(r.request_id, 42);
            assert_eq!(r.base, ReviewBase::Unstaged);
            assert_eq!(r.files[0].path, "a.txt");
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// The next message the daemon sends this connection, waited for.
async fn answer(h: &mut Harness) -> ServerMsg {
    tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
        .await
        .expect("an answer")
        .expect("channel open")
}

#[tokio::test]
async fn the_code_stage_reads_a_versions_files_one_file_and_a_search() {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.rs"), "fn brief() {}\n").unwrap();
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    let version = argus_protocol::CodeVersion::WorkingTree;

    h.send(ClientMsg::ListCode { request_id: 1, checkout, version: version.clone() });
    match answer(&mut h).await {
        ServerMsg::CodeTree(tree) => assert_eq!((tree.request_id, tree.files), (1, vec!["a.rs".to_string()])),
        other => panic!("unexpected {other:?}"),
    }

    h.send(ClientMsg::ReadCode { request_id: 2, checkout, path: "a.rs".into(), version: version.clone() });
    match answer(&mut h).await {
        ServerMsg::Code(file) => {
            assert_eq!(file.lines[0].text, "fn brief() {}");
            assert_eq!(file.symbols[0].name, "brief");
        }
        other => panic!("unexpected {other:?}"),
    }

    h.send(ClientMsg::SearchCode { request_id: 3, checkout, query: "brief".into(), options: Default::default() });
    assert!(matches!(answer(&mut h).await, ServerMsg::CodeMatches(m) if m.request_id == 3 && m.matches.len() == 1));

    h.send(ClientMsg::ReadCode { request_id: 4, checkout, path: "../out".into(), version });
    assert!(matches!(answer(&mut h).await, ServerMsg::CodeFailed { request_id: 4, .. }));
}

#[tokio::test]
async fn a_fold_is_read_from_the_blob_the_review_named() {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    let wait = std::time::Duration::from_secs(5);
    h.send(ClientMsg::Review {
        request_id: 1,
        checkout,
        base: ReviewBase::Unstaged,
        commit: None,
    });
    let Some(ServerMsg::Review(review)) = tokio::time::timeout(wait, h.rx.recv()).await.unwrap() else {
        panic!("no review");
    };
    let blob = review.files[0].new_blob.clone().expect("a blob to read folds from");

    h.send(ClientMsg::ReviewContext {
        request_id: 2,
        checkout,
        blob,
        path: "a.txt".into(),
        start: 2,
        end: 3,
    });
    match tokio::time::timeout(wait, h.rx.recv()).await.unwrap() {
        Some(ServerMsg::ReviewContext(context)) => {
            assert_eq!((context.request_id, context.checkout), (2, checkout));
            let texts: Vec<&str> = context.lines.iter().map(|l| l.text.as_str()).collect();
            assert_eq!(texts, ["two", "three"]);
        }
        other => panic!("unexpected {other:?}"),
    }

    h.send(ClientMsg::ReviewContext {
        request_id: 3,
        checkout,
        blob: "nonsense".into(),
        path: "a.txt".into(),
        start: 1,
        end: 1,
    });
    assert!(matches!(
        tokio::time::timeout(wait, h.rx.recv()).await.unwrap(),
        Some(ServerMsg::ReviewContextFailed { request_id: 3, .. })
    ));
}

/// Another client's review holding the daemon's turn until the sender is
/// sent to or dropped.
async fn hold_the_review_turn(
    h: &Harness,
    checkout: CheckoutId,
) -> (crate::review_queue::ReviewLane<()>, std::sync::mpsc::Sender<()>) {
    let mut other = h.daemon.review_lane();
    let (release, held) = std::sync::mpsc::channel::<()>();
    let (started, mut generating) = tokio::sync::mpsc::unbounded_channel::<()>();
    other.submit(
        ReviewAsk::new(checkout, ReviewBase::Unstaged, None),
        move || {
            let _ = started.send(());
            let _ = held.recv();
        },
        |_| {},
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), generating.recv())
        .await
        .expect("the other review takes the turn");
    (other, release)
}

#[tokio::test]
async fn a_review_asked_again_while_queued_is_answered_under_both_ids() {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    let (_other, release) = hold_the_review_turn(&h, checkout).await;

    for request_id in [1, 2] {
        h.send(ClientMsg::Review {
            request_id,
            checkout,
            base: ReviewBase::Unstaged,
            commit: None,
        });
    }
    release.send(()).unwrap();

    let mut answered = Vec::new();
    for _ in 0..2 {
        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
            .await
            .expect("each ask should be answered")
            .expect("channel open");
        match reply {
            ServerMsg::Review(review) => answered.push(review),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!((answered[0].request_id, answered[1].request_id), (1, 2));
    assert_eq!(answered[0].files, answered[1].files, "one diff answers both");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), h.rx.recv())
            .await
            .is_err(),
        "the joined ask is read once"
    );
}

/// The history's stuck preview: a cursor crossing two commits asked for
/// both, and the second ask cancelled the first.
#[tokio::test]
async fn two_commits_diffs_asked_back_to_back_are_both_answered() {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    let mut commits = Vec::new();
    for text in ["one\n", "two\n"] {
        std::fs::write(dir.path().join("a.txt"), text).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        let oid = repo.commit(Some("HEAD"), &sig, &sig, text, &tree, &parents).unwrap();
        commits.push(oid.to_string());
    }
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    let (_other, release) = hold_the_review_turn(&h, checkout).await;

    for (request_id, commit) in [(1, &commits[0]), (2, &commits[1])] {
        h.send(ClientMsg::Review {
            request_id,
            checkout,
            base: ReviewBase::Commit,
            commit: Some(commit.clone()),
        });
    }
    release.send(()).unwrap();

    let mut answered = Vec::new();
    for _ in 0..2 {
        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
            .await
            .expect("each commit should be answered")
            .expect("channel open");
        match reply {
            ServerMsg::Review(review) => {
                answered.push((review.request_id, review.commit.expect("a commit diff").oid))
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    answered.sort();
    assert_eq!(answered, [(1, commits[0].clone()), (2, commits[1].clone())]);
}

/// A review opens by asking for both of its sides at once; the second must
/// not cancel the first, or the dialog waits on it forever.
#[tokio::test]
async fn both_sides_asked_together_are_both_answered() {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    for (request_id, base) in [(1, ReviewBase::Unstaged), (2, ReviewBase::Staged)] {
        h.send(ClientMsg::Review {
            request_id,
            checkout,
            base,
            commit: None,
        });
    }

    let mut answered = Vec::new();
    for _ in 0..2 {
        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
            .await
            .expect("each side should be answered")
            .expect("channel open");
        match reply {
            ServerMsg::Review(r) => answered.push((r.request_id, r.base)),
            other => panic!("unexpected {other:?}"),
        }
    }
    answered.sort_by_key(|(id, _)| *id);
    assert_eq!(answered, [(1, ReviewBase::Unstaged), (2, ReviewBase::Staged)]);
}

#[tokio::test]
async fn a_review_of_a_checkout_that_is_gone_is_refused_under_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::Review {
        request_id: 1,
        checkout: CheckoutId(9999),
        base: ReviewBase::Unstaged,
        commit: None,
    });
    let reply = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
        .await
        .expect("the refusal should arrive")
        .expect("channel open");
    match reply {
        ServerMsg::ReviewFailed {
            request_id: 1,
            checkout: CheckoutId(9999),
            message,
        } => assert!(message.contains("no such checkout"), "{message}"),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn a_refused_make_is_answered_once_by_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::refused_by_id(dir.path());
    h.send(ClientMsg::SpawnShell {
        checkout: CheckoutId(9999),
        request_id: 8,
    });
    match answer(&mut h).await {
        ServerMsg::Refused(refusal) => {
            assert_eq!((refusal.request_id, refusal.read), (8, false));
            assert!(refusal.reason.contains("no such checkout"), "{}", refusal.reason);
        }
        other => panic!("unexpected {other:?}"),
    }
    // Whatever else the spawn's thread sent went with the refusal.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(h.replies().is_empty(), "no Created and no Error beside it");
}

#[tokio::test]
async fn a_refused_feature_change_is_answered_by_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::refused_by_id(dir.path());
    let (project, checkout) = (h.daemon.snapshot()[0].id, h.checkout());
    h.send(ClientMsg::Feature {
        project,
        checkout,
        action: argus_protocol::FeatureAction::Drop { slug: "nope".into() },
        request_id: 5,
    });
    let refusal = h.refusal().await;
    assert_eq!((refusal.request_id, refusal.read), (5, false));
}

#[tokio::test]
async fn a_read_of_a_checkout_that_is_gone_is_refused_by_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::refused_by_id(dir.path());
    let gone = CheckoutId(9999);
    h.send(ClientMsg::ListCode { request_id: 1, checkout: gone, version: argus_protocol::CodeVersion::WorkingTree });
    h.send(ClientMsg::ListCommits { request_id: 2, checkout: gone });
    h.send(ClientMsg::Review { request_id: 3, checkout: gone, base: ReviewBase::Unstaged, commit: None });
    let mut refused = Vec::new();
    for _ in 0..3 {
        let refusal = h.refusal().await;
        assert!(refusal.read && refusal.reason.contains("no such checkout"), "{refusal:?}");
        refused.push(refusal.request_id);
    }
    refused.sort();
    assert_eq!(refused, [1, 2, 3]);
}

#[tokio::test]
async fn a_code_read_that_cannot_be_answered_is_refused_by_its_id() {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    let mut h = Harness::refused_by_id(dir.path());
    let checkout = h.checkout();
    h.send(ClientMsg::ReadCode {
        request_id: 4,
        checkout,
        path: "../out".into(),
        version: argus_protocol::CodeVersion::WorkingTree,
    });
    let refusal = h.refusal().await;
    assert_eq!((refusal.request_id, refusal.read), (4, true));
}

#[tokio::test]
async fn an_unnamed_request_is_still_refused_as_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::refused_by_id(dir.path());
    h.send(ClientMsg::Fetch { checkout: CheckoutId(9999) });
    let error = h.error().await;
    assert!(error.contains("no such checkout"), "{error}");
}

#[tokio::test]
async fn a_gone_checkouts_code_read_is_a_code_failure_to_an_older_client() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::ReadCode {
        request_id: 6,
        checkout: CheckoutId(9999),
        path: "a.rs".into(),
        version: argus_protocol::CodeVersion::WorkingTree,
    });
    assert!(matches!(
        answer(&mut h).await,
        ServerMsg::CodeFailed { request_id: 6, checkout: CheckoutId(9999), .. }
    ));
}

#[tokio::test]
async fn a_gone_checkouts_commit_files_are_a_commit_files_failure() {
    let dir = tempfile::tempdir().unwrap();
    for mut h in [Harness::new(dir.path()), Harness::refused_by_id(dir.path())] {
        h.send(ClientMsg::ListCommitFiles { checkout: CheckoutId(9999), commit: "abc".into() });
        match answer(&mut h).await {
            ServerMsg::CommitFilesFailed { checkout: CheckoutId(9999), commit, message } => {
                assert_eq!(commit, "abc");
                assert!(message.contains("no such checkout"), "{message}");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[tokio::test]
async fn input_and_resize_for_a_dead_pane_do_not_take_the_connection_down() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::Input {
        pane: PaneId(9999),
        bytes: b"hello".to_vec(),
    });
    h.send(ClientMsg::Resize {
        pane: PaneId(9999),
        rows: 10,
        cols: 40,
    });
    assert_eq!(h.replies().len(), 2, "one error each, and still running");
}

#[tokio::test]
async fn opening_an_editor_on_a_path_outside_the_checkout_is_refused_here_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    h.send(ClientMsg::OpenInEditor {
        checkout,
        path: "../escape.rs".to_string(),
        line: None,
        external: false,
        command: None,
        request_id: 0,
    });
    let error = h.error().await;
    assert!(error.contains("inside the checkout"), "{error}");
}

#[tokio::test]
async fn switching_to_a_workspace_that_does_not_exist_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::OpenWorkspace {
        workspace: argus_protocol::WorkspaceId(9999),
    });
    assert!(!h.error().await.is_empty());
}

#[tokio::test]
async fn two_panes_stream_at_once() {
    // A floating editor must not cost the client sight of the agent
    // behind it, so one connection carries more than one subscription.
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    let a = h.daemon.spawn_shell(checkout).unwrap();
    let b = h.daemon.spawn_shell(checkout).unwrap();

    h.send(ClientMsg::Subscribe { pane: a });
    h.send(ClientMsg::Subscribe { pane: b });
    assert_eq!(h.conn.subs.tasks.len(), 2);

    // Both snapshots arrive, and neither subscription displaced the
    // other.
    let panes: Vec<PaneId> = h
        .replies()
        .into_iter()
        .filter_map(|m| match m {
            ServerMsg::PaneSnapshot { pane, .. } => Some(pane),
            _ => None,
        })
        .collect();
    assert!(panes.contains(&a) && panes.contains(&b), "{panes:?}");

    h.send(ClientMsg::Unsubscribe { pane: a });
    assert_eq!(h.conn.subs.tasks.len(), 1, "only the one named is dropped");
    assert!(h.conn.subs.tasks.contains_key(&b));

    let _ = h.daemon.close_pane(a);
    let _ = h.daemon.close_pane(b);
}

#[tokio::test]
async fn subscribing_twice_to_one_pane_does_not_double_up() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    let pane = h.daemon.spawn_shell(checkout).unwrap();

    h.send(ClientMsg::Subscribe { pane });
    h.send(ClientMsg::Subscribe { pane });
    assert_eq!(h.conn.subs.tasks.len(), 1);

    let _ = h.daemon.close_pane(pane);
}

#[tokio::test]
async fn the_resumes_name_each_harness_command_and_one_it_cannot_take_is_refused() {
    // The default templates: claude's resumes by id, and what the client
    // is told is the command that would run.
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let checkout = h.checkout();
    h.send(ClientMsg::ListResumes { request_id: 2 });
    let resumes = h.replies().into_iter().find_map(|reply| match reply {
        ServerMsg::Resumes { request_id: 2, resumes } => Some(resumes),
        _ => None,
    });
    let resumes = resumes.expect("the resumes are answered at once");
    let claude = resumes.iter().find(|r| r.harness == "claude").expect("claude resumes");
    assert!(claude.argv.ends_with(&["--resume".to_string(), "{session_id}".to_string()]), "{claude:?}");
    assert!(!resumes.iter().any(|r| r.harness == "generic"), "generic has no resume_id");

    h.send(ClientMsg::ResumeSession {
        request_id: 6,
        checkout,
        harness: "generic".into(),
        session_id: "s1".into(),
    });
    let answer = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
        .await
        .expect("an answer should arrive");
    assert!(matches!(answer, Some(ServerMsg::Created { request_id: 6, created: None })), "{answer:?}");
    let error = h.error().await;
    assert!(error.contains("no agent template can resume a generic session"), "{error}");

    // On Windows an agent runs through `cmd.exe /C`, so an id is never
    // put on its command line with anything cmd would read.
    h.send(ClientMsg::ResumeSession {
        request_id: 7,
        checkout,
        harness: "claude".into(),
        session_id: "s1 & calc".into(),
    });
    let answer = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
        .await
        .expect("an answer should arrive");
    assert!(matches!(answer, Some(ServerMsg::Created { request_id: 7, created: None })), "{answer:?}");
    let error = h.error().await;
    assert!(error.contains("not a session id"), "{error}");
}

#[tokio::test]
async fn a_repositorys_sessions_and_what_one_said_are_answered_to_the_asker() {
    let dir = tempfile::tempdir().unwrap();
    let kept = dir.path().join(".herdr");
    std::fs::create_dir_all(&kept).unwrap();
    std::fs::write(
        kept.join("s1.jsonl"),
        "{\"kind\":\"user\",\"text\":\"Tidy the notes\"}\n{\"kind\":\"agent\",\"text\":\"Tidied.\"}\n",
    )
    .unwrap();
    let herdr: crate::config::HarnessConfig = toml::from_str(
        "name = \"herdr\"\ntranscript = { path = \"{checkout}/.herdr/{session_id}.jsonl\" }\n",
    )
    .unwrap();
    let mut h = Harness::with_harnesses(dir.path(), vec![herdr]);
    let repository = h.daemon.snapshot()[0].repositories[0].id;

    // Answered from a blocking thread, which may read the real home's
    // transcripts for the built-in harnesses too.
    async fn answer(h: &mut Harness) -> ServerMsg {
        tokio::time::timeout(std::time::Duration::from_secs(60), h.rx.recv())
            .await
            .expect("an answer should arrive")
            .unwrap()
    }
    h.send(ClientMsg::ListSessions { request_id: 3, repository });
    let sessions = match answer(&mut h).await {
        ServerMsg::Sessions { request_id: 3, repository: r, sessions } if r == repository => sessions,
        other => panic!("expected sessions, got {other:?}"),
    };
    let herdr = sessions.iter().find(|s| s.harness == "herdr").expect("the herdr session");
    assert_eq!(herdr.session_id, "s1");
    assert_eq!(herdr.title.as_deref(), Some("Tidy the notes"));
    assert_eq!(herdr.turns, Some(1));

    h.send(ClientMsg::ReadSession {
        request_id: 4,
        checkout: herdr.checkout,
        harness: "herdr".into(),
        session_id: "s1".into(),
    });
    match answer(&mut h).await {
        ServerMsg::SessionTranscript { request_id: 4, session_id, conversation } => {
            assert_eq!(session_id, "s1");
            assert!(
                matches!(&*conversation, Conversation::Read(t) if t.entries.len() == 2),
                "{conversation:?}"
            );
        }
        other => panic!("expected the transcript, got {other:?}"),
    }

    h.send(ClientMsg::ListSessions { request_id: 5, repository: RepositoryId(9999) });
    assert!(h.error().await.contains("no such repository"));
}

#[test]
fn a_description_of_the_extensions_is_answered_to_the_client_that_asked() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.daemon.admit_extension("docker");
    let about = argus_protocol::ExtAbout { summary: "Docker containers".into(), guide: None };
    h.daemon.publish_about("docker", about.clone()).unwrap();

    h.send(ClientMsg::DescribeExtensions);

    let described = h.replies().into_iter().find_map(|reply| match reply {
        ServerMsg::ExtensionsDescribed(described) => Some(described),
        _ => None,
    });
    let described = described.expect("the description is answered");
    assert_eq!(described.len(), 1);
    assert_eq!((described[0].name.as_str(), described[0].about.as_ref()), ("docker", Some(&about)));
}

#[tokio::test]
async fn a_view_and_the_watches_take_effect_through_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());

    h.send(ClientMsg::View(argus_protocol::View { active: true, ..Default::default() }));
    assert!(h.daemon.focus_in(&h.daemon.snapshot()).is_some(), "the view is taken");

    h.send(ClientMsg::WatchFocus);
    assert!(matches!(h.replies().as_slice(), [ServerMsg::Focus(Some(_))]));
    assert!(h.conn.topics.joined::<topics::Focus>());

    h.send(ClientMsg::WatchViews);
    assert!(h.replies().is_empty(), "no views to replay");
    assert!(h.conn.topics.joined::<topics::Views>());
}

#[test]
fn a_press_that_cannot_be_relayed_is_answered_to_the_client_that_pressed() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let press = argus_protocol::ViewPress {
        view: "ci".into(),
        action: "rerun".into(),
        rows: vec![],
        form: None,
        answers: vec![],
    };

    h.send(ClientMsg::PressView { pane: argus_protocol::PaneId(7), press });

    match h.replies().as_slice() {
        [ServerMsg::ViewPressFailed { pane, view, action, reason }] => {
            assert_eq!((pane.0, view.as_str(), action.as_str()), (7, "ci", "rerun"));
            assert!(!reason.is_empty());
        }
        other => panic!("expected the failure, got {other:?}"),
    }
}

#[tokio::test]
async fn a_greeting_is_answered_then_followed_by_what_the_client_takes() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());

    assert!(!h.send(ClientMsg::Hello(argus_protocol::Hello::this_build())));

    assert!(matches!(
        h.replies().as_slice(),
        [
            ServerMsg::Hello(_),
            ServerMsg::Consoles(_),
            ServerMsg::ExtensionViews(_),
            ServerMsg::ExtensionCommands(_),
        ]
    ));
    assert!(h.conn.subs.runs);
    assert!(h.conn.peer.can(argus_protocol::CELL_RUNS));
}

#[tokio::test]
async fn a_greeting_without_extensions_is_sent_none_of_theirs() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let hello = argus_protocol::Hello::this_build().without(argus_protocol::EXTENSIONS);

    h.send(ClientMsg::Hello(hello));

    assert!(matches!(h.replies().as_slice(), [ServerMsg::Hello(_), ServerMsg::Consoles(_)]));
}

#[tokio::test]
async fn a_second_greeting_without_extensions_leaves_their_topic() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::Hello(argus_protocol::Hello::this_build()));
    assert!(h.conn.topics.joined::<topics::Extensions>());

    let hello = argus_protocol::Hello::this_build().without(argus_protocol::EXTENSIONS);
    h.send(ClientMsg::Hello(hello));

    assert!(!h.conn.topics.joined::<topics::Extensions>());
}

#[tokio::test]
async fn a_greeting_without_consoles_is_sent_none() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    let hello = argus_protocol::Hello::this_build().without(argus_protocol::CONSOLES);

    h.send(ClientMsg::Hello(hello));

    assert!(matches!(
        h.replies().as_slice(),
        [ServerMsg::Hello(_), ServerMsg::ExtensionViews(_), ServerMsg::ExtensionCommands(_)]
    ));
}

/// Whether a dialog raised now finds a client to put it to.
fn askable(h: &Harness) -> bool {
    let dialog = argus_protocol::ExtDialog {
        title: "t".into(),
        kind: argus_protocol::ExtDialogKind::Prompt { message: "m".into(), initial: String::new() },
        answers: None,
    };
    h.daemon.raise_dialog("docker", dialog).is_ok()
}

#[tokio::test]
async fn only_a_greeting_that_takes_extensions_and_dialogs_is_put_a_dialog() {
    use argus_protocol::{Hello, EXTENSIONS, EXTENSION_DIALOGS};
    let cases = [
        (Hello::this_build(), true),
        (Hello::this_build().without(EXTENSION_DIALOGS), false),
        (Hello::this_build().without(EXTENSIONS), false),
    ];
    for (hello, asks) in cases {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.daemon.admit_extension("docker");

        h.send(ClientMsg::Hello(hello.clone()));

        assert_eq!(askable(&h), asks, "{hello:?}");
    }
}

#[tokio::test]
async fn the_saved_view_messages_take_effect_only_for_a_client_that_listed_the_capability() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());

    h.send(ClientMsg::WatchSavedViews);
    h.send(ClientMsg::MarkSavedViewStale { id: "deps".into() });
    assert!(!h.conn.topics.joined::<topics::SavedViews>(), "a client that never greeted is not watching");
    assert!(h.replies().is_empty(), "and is not answered");

    h.send(ClientMsg::Hello(argus_protocol::Hello::this_build().without(argus_protocol::SAVED_VIEWS)));
    h.replies();
    h.send(ClientMsg::WatchSavedViews);
    h.send(ClientMsg::DeleteSavedView { id: "deps".into() });
    assert!(!h.conn.topics.joined::<topics::SavedViews>());
    assert!(h.replies().is_empty(), "one that greeted without it is no different");
}

#[tokio::test]
async fn watching_saved_views_replays_them_and_a_refused_request_is_answered_to_the_asker() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new(dir.path());
    h.send(ClientMsg::Hello(argus_protocol::Hello::this_build()));
    h.replies();

    h.send(ClientMsg::WatchSavedViews);
    assert!(matches!(h.replies().as_slice(), [ServerMsg::SavedViews(all)] if all.is_empty()));
    assert!(h.conn.topics.joined::<topics::SavedViews>());

    let press = argus_protocol::ViewPress {
        view: "deps".into(),
        action: "rerun".into(),
        rows: vec![],
        form: None,
        answers: vec![],
    };
    for msg in [
        ClientMsg::RefreshSavedView { id: "deps".into() },
        ClientMsg::MarkSavedViewStale { id: "deps".into() },
        ClientMsg::PressSavedView { id: "deps".into(), press },
    ] {
        h.send(msg);
        match h.replies().as_slice() {
            [ServerMsg::SavedViewFailed { id, reason }] => {
                assert_eq!(id, "deps");
                assert!(!reason.is_empty());
            }
            other => panic!("expected the failure, got {other:?}"),
        }
    }
    // Deleting what is not there is not a failure: it is gone either way.
    h.send(ClientMsg::DeleteSavedView { id: "deps".into() });
    assert!(h.replies().is_empty());
}
