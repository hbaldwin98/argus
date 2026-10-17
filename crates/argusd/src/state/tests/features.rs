//! A change to a feature, applied as a person and as an agent: the one
//! path both take, the board it pushes, and the rules that differ by who
//! asked.

use super::*;
use argus_protocol::{
    ArtifactScope, DecisionBoard, DecisionWrite, Feature, FeatureAction, FeatureState,
    FeatureWrite, LinkWrite, TaskAction, TaskWrite,
};

/// A daemon whose one checkout has an agent in it: the person's address
/// for the board and the agent's pane, which name the same one.
fn person_and_agent(dir: &std::path::Path) -> (Arc<Daemon>, ProjectId, CheckoutId, PaneId) {
    let d = daemon_with_claude_aliases(dir, &["claude"]);
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    (d, project, checkout, agent)
}

fn open(title: &str) -> FeatureAction {
    FeatureAction::Open(FeatureWrite {
        title: title.into(),
        body: None,
    })
}

/// The one board a change pushed. Every change pushes exactly one, so a
/// second is a change pushed twice and none is one that went nowhere.
fn pushed(rx: &mut crate::state::pushes::Pushed<BoardNews, DecisionBoard>) -> DecisionBoard {
    let board = rx.try_recv().expect("the change is pushed to every client");
    assert!(rx.try_recv().is_err(), "and pushed once");
    board
}

fn feature<'a>(board: &'a DecisionBoard, slug: &str) -> &'a Feature {
    board.features.iter().find(|f| f.slug == slug).unwrap()
}

#[tokio::test]
async fn each_change_both_sides_make_pushes_the_same_board_from_either() {
    let dir = tempfile::tempdir().unwrap();
    let (d, project, checkout, agent) = person_and_agent(dir.path());
    let mut rx = d.subscribe_decisions();
    for title in ["by the person", "by the agent", "first"] {
        d.feature_action_for_client(project, checkout, open(title)).unwrap();
        pushed(&mut rx);
    }
    let link = |url: &str| LinkWrite {
        url: url.into(),
        ..LinkWrite::default()
    };
    type Step = (fn(&str) -> FeatureAction, fn(&Feature) -> bool);
    let steps: Vec<(&str, Step)> = vec![
        ("retitle", (|slug| FeatureAction::Retitle { slug: slug.into(), title: "renamed".into() },
            |f| f.title == "renamed")),
        ("rewrite", (|slug| FeatureAction::Rewrite { slug: slug.into(), body: "what it is for".into() },
            |f| f.body == "what it is for")),
        ("hold", (|slug| FeatureAction::Hold { slug: slug.into(), reason: "until asked".into() },
            |f| f.held.as_deref() == Some("until asked"))),
        ("unhold", (|slug| FeatureAction::Unhold { slug: slug.into() }, |f| f.held.is_none())),
        ("wait", (|slug| FeatureAction::Wait { slug: slug.into(), on: "first".into() },
            |f| f.waits_on == ["first"])),
        ("unwait", (|slug| FeatureAction::Unwait { slug: slug.into(), on: "first".into() },
            |f| f.waits_on.is_empty())),
        ("link", (|slug| FeatureAction::Link {
                slug: slug.into(),
                link: LinkWrite {
                    url: "https://linear.app/acme/issue/ENG-88/trim".into(),
                    ..LinkWrite::default()
                },
            },
            |f| f.links.iter().map(|l| l.key.as_str()).eq(["ENG-88"]))),
        ("unlink", (|slug| FeatureAction::Unlink { slug: slug.into(), key: "eng-88".into() },
            |f| f.links.is_empty())),
        ("done", (|slug| FeatureAction::Done { slug: slug.into(), detail: None },
            |f| f.state == FeatureState::Done)),
        ("reopen", (|slug| FeatureAction::Reopen { slug: slug.into(), detail: None },
            |f| f.state == FeatureState::Open)),
    ];

    for (name, (action, holds)) in steps {
        d.feature_action_for_client(project, checkout, action("by-the-person"))
            .unwrap_or_else(|e| panic!("{name} as a person: {e}"));
        assert!(holds(feature(&pushed(&mut rx), "by-the-person")), "{name} as a person");

        let answer = d
            .feature_action_for_agent(agent, None, action("by-the-agent"), ArtifactScope::default())
            .unwrap_or_else(|e| panic!("{name} as an agent: {e}"));
        let board = pushed(&mut rx);
        assert!(holds(feature(&board, "by-the-agent")), "{name} as an agent");
        assert_eq!(answer.features, board.features, "{name}: the agent is answered with the board pushed");
    }

    let refusals = [
        FeatureAction::Wait { slug: "first".into(), on: "first".into() },
        FeatureAction::Link { slug: "first".into(), link: link("file:///etc/passwd") },
        open("  "),
    ];
    for refused in refusals {
        assert!(d.feature_action_for_client(project, checkout, refused.clone()).is_err(), "{refused:?}");
        assert!(d.feature_action_for_agent(agent, None, refused.clone(), ArtifactScope::default()).is_err());
    }
    assert!(rx.try_recv().is_err(), "a refusal pushes nothing: the view keeps showing what is true");
    close_all(&d);
}

#[tokio::test]
async fn an_agents_feature_starts_where_it_was_opened_and_a_persons_has_no_home_yet() {
    let dir = tempfile::tempdir().unwrap();
    let (d, project, checkout, agent) = person_and_agent(dir.path());
    let mut rx = d.subscribe_decisions();

    d.feature_action_for_client(project, checkout, open("written down")).unwrap();
    let theirs = feature(&pushed(&mut rx), "written-down").clone();
    assert_eq!((theirs.origin_checkout, theirs.checkouts.len()), (None, 0));

    let answer = d
        .feature_action_for_agent(agent, Some("session-1"), open("cut here"), ArtifactScope::default())
        .unwrap();
    assert_eq!(answer.current.as_deref(), Some("cut-here"), "the agent's checkout is on it");
    let mine = feature(&pushed(&mut rx), "cut-here").clone();
    assert!(mine.origin_checkout.is_some());
    assert_eq!((mine.checkouts.len(), mine.session.as_deref()), (1, Some("session-1")));
    close_all(&d);
}

#[tokio::test]
async fn an_agents_move_is_recorded_as_its_own_and_only_a_person_says_why() {
    // Acceptance is a person's call; an agent carrying it out must not
    // make the history say the person moved it themselves.
    let dir = tempfile::tempdir().unwrap();
    let (d, project, checkout, agent) = person_and_agent(dir.path());
    let slug = open_feature(&d, agent, "finished work");
    let done = |detail: Option<&str>| FeatureAction::Done {
        slug: slug.clone(),
        detail: detail.map(str::to_string),
    };
    let key = d.address_for_agent(agent, ArtifactScope::default()).unwrap().key;
    let last = |d: &Daemon| d.store.feature_events(&key, &slug).unwrap().pop().unwrap();

    d.feature_action_for_agent(agent, Some("session-1"), done(None), ArtifactScope::default())
        .unwrap();
    let event = last(&d);
    assert_eq!((event.actor.as_str(), event.session.as_deref()), ("agent", Some("session-1")));
    let refused = d
        .feature_action_for_agent(agent, None, done(Some("looks right")), ArtifactScope::default())
        .unwrap_err();
    assert!(refused.to_string().contains("person's"), "{refused}");

    d.feature_action_for_client(project, checkout, done(Some("looks right"))).unwrap();
    let event = last(&d);
    assert_eq!(
        (event.actor.as_str(), event.session, event.detail.as_deref()),
        ("human", None, Some("looks right"))
    );
    close_all(&d);
}

#[tokio::test]
async fn an_agent_drops_only_an_empty_feature_and_a_person_drops_any() {
    let dir = tempfile::tempdir().unwrap();
    let (d, project, checkout, agent) = person_and_agent(dir.path());
    let slug = open_feature(&d, agent, "mistaken work");
    let drop = || FeatureAction::Drop { slug: slug.clone() };
    let still_there = |d: &Daemon| {
        d.decision_board(project, checkout).unwrap().features.iter().any(|f| f.slug == slug)
    };
    d.part_action_for_agent(
        agent,
        None,
        TaskAction::Add(TaskWrite {
            title: "someone's task".into(),
            external: None,
            parent: None,
        }),
        ArtifactScope::default(),
    )
    .unwrap();

    let refused = d
        .feature_action_for_agent(agent, None, drop(), ArtifactScope::default())
        .unwrap_err();
    assert!(refused.to_string().contains("1 task"), "{refused}");
    assert!(still_there(&d), "work under it is not an agent's to delete whole");

    let mut rx = d.subscribe_decisions();
    d.feature_action_for_client(project, checkout, drop()).unwrap();
    assert!(!pushed(&mut rx).features.iter().any(|f| f.slug == slug), "a person removes it whole");

    let empty = open_feature(&d, agent, "opened by mistake");
    pushed(&mut rx);
    d.feature_action_for_agent(agent, None, FeatureAction::Drop { slug: empty.clone() }, ArtifactScope::default())
        .unwrap();
    assert!(!pushed(&mut rx).features.iter().any(|f| f.slug == empty), "empty, an agent's drop takes it");
    close_all(&d);
}

#[tokio::test]
async fn an_agents_checkout_moves_are_its_own_and_a_transfer_or_a_note_is_the_views() {
    let dir = tempfile::tempdir().unwrap();
    let (d, project, checkout, agent) = person_and_agent(dir.path());
    let slug = open_feature(&d, agent, "work");
    let mut rx = d.subscribe_decisions();

    let agents_own = [
        FeatureAction::Select { slug: slug.clone() },
        FeatureAction::Append { text: "a paragraph".into() },
    ];
    for action in agents_own {
        assert!(d.feature_action_for_client(project, checkout, action.clone()).is_err(), "{action:?}");
    }
    let views = [
        FeatureAction::Transfer { slug: slug.clone(), to: checkout },
        FeatureAction::Capture {
            into: None,
            capture: Box::new(argus_protocol::Capture::parse("A note")),
        },
    ];
    for action in views {
        assert!(
            d.feature_action_for_agent(agent, None, action.clone(), ArtifactScope::default()).is_err(),
            "{action:?}"
        );
    }
    assert!(rx.try_recv().is_err(), "a refusal pushes nothing");
    close_all(&d);
}

#[tokio::test]
async fn a_feature_document_grows_and_a_checkout_can_go_back_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, agent) = person_and_agent(dir.path());
    let first = open_feature(&d, agent, "notes storage");
    let append = FeatureAction::Append { text: "the key has to outlive the ids".into() };
    d.feature_action_for_agent(agent, None, append, ArtifactScope::default())
        .unwrap();
    open_feature(&d, agent, "the pty deadlock");

    let select = |slug: &str| FeatureAction::Select { slug: slug.into() };
    let board = d
        .feature_action_for_agent(agent, None, select(&first), ArtifactScope::default())
        .unwrap();
    assert_eq!(board.current.as_deref(), Some(first.as_str()));
    let document = &board.features.iter().find(|f| f.slug == first).unwrap().body;
    assert_eq!(document, "the key has to outlive the ids");

    assert!(
        d.feature_action_for_agent(agent, None, select("no-such-feature"), ArtifactScope::default())
            .is_err(),
        "a checkout cannot be pointed at a feature that does not exist"
    );
    let blank = FeatureAction::Append { text: "  ".into() };
    assert!(d.feature_action_for_agent(agent, None, blank, ArtifactScope::default()).is_err());
    close_all(&d);
}

#[tokio::test]
async fn an_agent_on_no_feature_has_nothing_to_append_to() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, agent) = person_and_agent(dir.path());
    let append = FeatureAction::Append { text: "a stray note".into() };
    let refused = d
        .feature_action_for_agent(agent, None, append, ArtifactScope::default())
        .unwrap_err();
    assert!(refused.to_string().contains("not on a feature"), "{refused}");
    close_all(&d);
}

#[tokio::test]
async fn repository_features_cross_branches_and_can_transfer_assignment() {
    let dir = tempfile::tempdir().unwrap();
    let _repo = real_repo(dir.path());
    let d = daemon_with_fake_claude(dir.path());
    let project = d.snapshot()[0].id;
    let source_checkout = only_checkout(&d);
    d.create_worktree(source_checkout, "topic".to_string())
        .await
        .unwrap();
    let destination_checkout = d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .find(|checkout| checkout.id != source_checkout)
        .unwrap()
        .id;
    let source_agent = d.spawn_agent(source_checkout, "claude").unwrap();
    let destination_agent = d.spawn_agent(destination_checkout, "claude").unwrap();

    let slug = open_feature(&d, source_agent, "shared work");
    let destination_board = d
        .feature_board_for_agent(destination_agent, ArtifactScope::default())
        .unwrap();
    assert_eq!(destination_board.features.len(), 1);
    assert_eq!(destination_board.current, None);
    let source_branch = d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .find(|checkout| checkout.id == source_checkout)
        .and_then(|checkout| checkout.git.as_ref()?.branch.clone());
    assert!(source_branch.is_some());
    assert_eq!(destination_board.features[0].origin_branch, source_branch, "a feature keeps the branch it was opened on");

    let transfer = FeatureAction::Transfer {
        slug: slug.clone(),
        to: destination_checkout,
    };
    d.feature_action_for_client(project, source_checkout, transfer.clone())
        .unwrap();
    let source_board = d
        .feature_board_for_agent(source_agent, ArtifactScope::default())
        .unwrap();
    let destination_board = d
        .feature_board_for_agent(destination_agent, ArtifactScope::default())
        .unwrap();
    assert_eq!(source_board.current, None);
    assert_eq!(destination_board.current.as_deref(), Some(slug.as_str()));
    assert_eq!(destination_board.features[0].checkouts.len(), 1);
    assert!(destination_board.features[0].checkouts[0].contains("topic"));
    assert!(d
        .feature_action_for_client(project, source_checkout, transfer)
        .is_err());
    close_all(&d);
}

#[tokio::test]
async fn a_pushed_board_names_its_repository_so_another_of_the_project_keeps_its_own() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    real_repo(a.path());
    real_repo(b.path());
    let d = daemon_with_repositories(&[
        a.path().to_str().unwrap(),
        b.path().to_str().unwrap(),
    ]);
    let tree = d.snapshot();
    let project = &tree[0];
    let checkout_of = |index: usize| {
        let repository = &project.repositories[index];
        (repository.id, repository.checkouts[0].id)
    };
    let (repo_a, checkout_a) = checkout_of(0);
    let (repo_b, checkout_b) = checkout_of(1);
    let mut rx = d.subscribe_decisions();

    d.feature_action_for_client(project.id, checkout_a, open("only in a"))
        .unwrap();

    let pushed = pushed(&mut rx);
    assert_eq!(pushed.repositories, [repo_a]);
    assert!(pushed.is_for(&project.name, repo_a));
    assert!(
        !pushed.is_for(&project.name, repo_b),
        "a client reading b's board keeps it, though the project's name matches"
    );
    let asked = d.decision_board(project.id, checkout_b).unwrap();
    assert_eq!(asked.repositories, [repo_b], "an answer names its repository too");
    assert!(asked.features.is_empty());
}

#[tokio::test]
async fn a_workspace_write_leaves_every_repositorys_board_on_screen_alone() {
    // A push says which repositories it is for, not which board it is, and
    // a client holds only its repository's board: a workspace board pushed
    // under its repositories would replace each one's own.
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let d = daemon_with_two_agent_checkouts(first.path(), second.path());
    let tree = d.snapshot();
    let project = &tree[0];
    let agent = d
        .spawn_agent(project.repositories[0].checkouts[0].id, "claude")
        .unwrap();
    let address = d.address_for_agent(agent, ArtifactScope::Workspace).unwrap();
    assert!(!address.is_pushed());
    let mut boards = d.subscribe_decisions();
    let mut tasks = d.subscribe_tasks();

    d.feature_action_for_agent(agent, None, open("workspace migration"), ArtifactScope::Workspace)
        .unwrap();
    d.part_action_for_agent(
        agent,
        None,
        TaskAction::Add(TaskWrite {
            title: "update both crates".into(),
            external: None,
            parent: None,
        }),
        ArtifactScope::Workspace,
    )
    .unwrap();

    assert!(boards.try_recv().is_err(), "no board is pushed for a workspace write");
    assert!(tasks.try_recv().is_err(), "nor a task list");
    close_all(&d);
}

#[tokio::test]
async fn a_feature_is_not_transferred_to_another_repositorys_checkout() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    real_repo(a.path());
    real_repo(b.path());
    let d = daemon_with_repositories(&[a.path().to_str().unwrap(), b.path().to_str().unwrap()]);
    let tree = d.snapshot();
    let project = &tree[0];
    let (in_a, in_b) = (project.repositories[0].checkouts[0].id, project.repositories[1].checkouts[0].id);
    d.feature_action_for_client(project.id, in_a, open("only in a")).unwrap();
    let slug = d.decision_board(project.id, in_a).unwrap().features[0].slug.clone();

    let refused = d
        .feature_action_for_client(project.id, in_a, FeatureAction::Transfer { slug, to: in_b })
        .unwrap_err();
    assert!(refused.to_string().contains("not in the source repository"), "{refused}");
    close_all(&d);
}

#[tokio::test]
async fn a_read_of_a_features_tasks_or_diagrams_names_its_repository() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    real_repo(a.path());
    real_repo(b.path());
    let d = daemon_with_repositories(&[
        a.path().to_str().unwrap(),
        b.path().to_str().unwrap(),
    ]);
    let tree = d.snapshot();
    let project = &tree[0];
    let (repo_a, checkout_a) = (project.repositories[0].id, project.repositories[0].checkouts[0].id);
    d.feature_action_for_client(project.id, checkout_a, open("only in a"))
        .unwrap();
    let slug = d.decision_board(project.id, checkout_a).unwrap().features[0].slug.clone();

    let tasks = d
        .part_action_for_client(project.id, checkout_a, &slug, argus_protocol::TaskAction::List)
        .unwrap();
    assert_eq!(tasks.repositories, [repo_a], "the daemon's read says whose board, not the dispatcher");
    let diagrams = d
        .part_action_for_client(project.id, checkout_a, &slug, argus_protocol::DiagramAction::List)
        .unwrap();
    assert_eq!(diagrams.repositories, [repo_a]);
    close_all(&d);
}

#[tokio::test]
async fn the_feature_endpoint_answers_a_change_with_the_board_and_a_bad_one_with_its_status() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, agent) = person_and_agent(dir.path());
    d.start_hook_server().unwrap();
    let post = |body: &'static str| {
        let d = d.clone();
        async move {
            String::from_utf8_lossy(&post_agent_hook(&d, agent, Endpoint::Feature, body).await)
                .to_string()
        }
    };

    let opened = post(r#"{"Open":{"title":"over the wire","body":null}}"#).await;
    assert!(opened.starts_with("HTTP/1.1 200 OK"), "{opened}");
    assert!(opened.contains(r#""current":"over-the-wire""#), "{opened}");
    // A helper from before `detail` sends a move without one.
    let done = post(r#"{"Done":{"slug":"over-the-wire"}}"#).await;
    assert!(done.starts_with("HTTP/1.1 200 OK"), "{done}");

    let garbage = post("hello").await;
    assert!(garbage.starts_with("HTTP/1.1 400"), "{garbage}");
    let refused = post(r#"{"Select":{"slug":"no-such-feature"}}"#).await;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    close_all(&d);
}

#[tokio::test]
async fn a_decision_filed_under_a_feature_keeps_an_agent_from_dropping_it() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, agent) = person_and_agent(dir.path());
    let slug = open_feature(&d, agent, "decided");
    d.record_agent_decision(
        agent,
        None,
        DecisionWrite {
            chose: "keep it".into(),
            ..Default::default()
        },
        ArtifactScope::default(),
    )
    .unwrap();
    let refused = d
        .feature_action_for_agent(agent, None, FeatureAction::Drop { slug }, ArtifactScope::default())
        .unwrap_err();
    assert!(refused.to_string().contains("1 decision(s)"), "{refused}");
    close_all(&d);
}

fn diagram() -> argus_protocol::DiagramAction {
    argus_protocol::DiagramAction::Add(argus_protocol::DiagramWrite {
        title: "flow".into(),
        body: "sequenceDiagram\n    A->>B: go".into(),
    })
}

#[tokio::test]
async fn a_diagram_alone_keeps_an_agent_from_dropping_its_feature() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, agent) = person_and_agent(dir.path());
    let slug = open_feature(&d, agent, "drawn");
    d.part_action_for_agent(agent, None, diagram(), ArtifactScope::default()).unwrap();
    let refused = d
        .feature_action_for_agent(agent, None, FeatureAction::Drop { slug }, ArtifactScope::default())
        .unwrap_err();
    assert!(refused.to_string().contains("1 diagram(s)"), "{refused}");
    close_all(&d);
}

#[tokio::test]
async fn an_agent_reads_only_its_features_decisions_and_none_taken_back() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, agent) = person_and_agent(dir.path());
    let decide = |chose: &str| {
        d.record_agent_decision(
            agent,
            None,
            DecisionWrite { chose: chose.into(), ..Default::default() },
            ArtifactScope::default(),
        )
        .unwrap()
    };
    let mine = open_feature(&d, agent, "mine");
    decide("kept");
    let taken_back = decide("taken back");
    d.change_decision_for_agent(
        agent,
        argus_protocol::DecisionChange::Withdraw { id: taken_back.id },
        ArtifactScope::default(),
    )
    .unwrap();
    open_feature(&d, agent, "another");
    decide("elsewhere");

    let board = d
        .feature_action_for_agent(agent, None, FeatureAction::Select { slug: mine }, ArtifactScope::default())
        .unwrap();
    let read: Vec<&str> = board.decisions.iter().map(|d| d.chose.as_str()).collect();
    assert_eq!(read, ["kept"]);
    close_all(&d);
}

#[tokio::test]
async fn an_agent_on_no_feature_reads_empty_parts_and_is_told_how_to_add_one() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, agent) = person_and_agent(dir.path());
    let tasks = d
        .part_action_for_agent(agent, None, TaskAction::List, ArtifactScope::default())
        .unwrap();
    assert!(tasks.feature.is_none() && tasks.tasks.is_empty());
    let diagrams = d
        .part_action_for_agent(agent, None, argus_protocol::DiagramAction::List, ArtifactScope::default())
        .unwrap();
    assert!(diagrams.feature.is_none() && diagrams.diagrams.is_empty());

    let adding = d.part_action_for_agent(agent, None, diagram(), ArtifactScope::default()).unwrap_err();
    assert!(adding.to_string().contains("before adding diagrams"), "{adding}");
    let removing = d
        .part_action_for_agent(agent, None, argus_protocol::DiagramAction::Remove { id: 1 }, ArtifactScope::default())
        .unwrap_err();
    assert_eq!(removing.to_string(), "this checkout is not on a feature yet", "only an add is told to open one");
    let ticking = d
        .part_action_for_agent(
            agent,
            None,
            TaskAction::Add(TaskWrite { title: "t".into(), external: None, parent: None }),
            ArtifactScope::default(),
        )
        .unwrap_err();
    assert!(ticking.to_string().contains("before adding tasks"), "{ticking}");
    close_all(&d);
}

#[tokio::test]
async fn a_diagram_change_is_pushed_and_a_read_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, agent) = person_and_agent(dir.path());
    open_feature(&d, agent, "drawn");
    let mut boards = d.pushes.subscribe_boards();
    let pushed_diagrams = |boards: &mut tokio::sync::broadcast::Receiver<BoardNews>| {
        let mut count = 0;
        while let Ok(news) = boards.try_recv() {
            if let BoardNews::Diagrams(list) = news {
                count += list.diagrams.len().max(1);
            }
        }
        count
    };

    d.part_action_for_agent(agent, None, diagram(), ArtifactScope::default()).unwrap();
    assert_eq!(pushed_diagrams(&mut boards), 1, "a client watching the feature sees the new diagram");
    d.part_action_for_agent(agent, None, argus_protocol::DiagramAction::List, ArtifactScope::default())
        .unwrap();
    d.part_action_for_agent(agent, None, TaskAction::List, ArtifactScope::default()).unwrap();
    assert_eq!(pushed_diagrams(&mut boards), 0, "a read changes nothing to push");
    close_all(&d);
}

#[tokio::test]
async fn an_agents_draft_and_a_persons_approval_reach_the_one_tracker_map() {
    let dir = tempfile::tempdir().unwrap();
    let (d, project, checkout, agent) = person_and_agent(dir.path());
    let map = d.tracker_map_for_agent(agent, "Linear: issues", ArtifactScope::default()).unwrap();
    assert_eq!(map.draft.map(|draft| draft.body).as_deref(), Some("Linear: issues"));

    d.change_tracker_map_for_client(project, checkout, argus_protocol::MapChange::Approve("Linear: issues".into()))
        .unwrap();
    let map = d.tracker_map_for_agent(agent, "", ArtifactScope::default()).unwrap();
    assert_eq!((map.body.as_str(), map.draft.is_none()), ("Linear: issues", true));
    close_all(&d);
}
