//! The board rules, applied to a hand-made address over the in-memory
//! store: no pane and no tree, so a test says only which board it is and
//! who asked. Resolving a pane or a checkout to an address is tested apart,
//! in `hook_lifecycle` and `pane_api`.

use super::*;
use crate::state::asker::Asker;
use crate::state::board_address::BoardAddress;
use crate::state::board_parts::BoardTarget;
use argus_protocol::{
    ArtifactScope, Decision, DecisionChange, DecisionWrite, FeatureAction, FeatureWrite,
    TaskAction, TaskWrite,
};

const AGENT: PaneId = PaneId(7);

/// A daemon with nothing in its tree, and the board an agent working in
/// `/repo` would address.
fn board() -> (Arc<Daemon>, BoardAddress) {
    let d = Daemon::new(ConfigFile::default());
    let at = BoardAddress {
        scope: ArtifactScope::RepositoryBranch,
        project: ProjectId(1),
        project_name: "proj".to_string(),
        key: "repository\0/repo/.git".to_string(),
        repositories: vec![RepositoryId(1)],
        checkout_path: Some(PathBuf::from("/repo")),
    };
    (d, at)
}

fn agent(session: Option<&str>) -> Asker<'_> {
    Asker::Agent {
        pane: AGENT,
        session,
    }
}

/// Opens a feature as an agent does, which points its checkout at it.
fn open_feature(d: &Daemon, at: &BoardAddress, title: &str) -> String {
    let open = FeatureAction::Open(FeatureWrite {
        title: title.into(),
        body: None,
    });
    d.apply_feature(at, agent(None), None, open).unwrap();
    d.feature_board(at, None)
        .unwrap()
        .current
        .expect("opening a feature works on it")
}

/// Records a decision as an agent does: under the feature it names, or
/// else the one its checkout is on.
fn decide(
    d: &Daemon,
    at: &BoardAddress,
    session: Option<&str>,
    named: Option<&str>,
    write: DecisionWrite,
) -> anyhow::Result<Decision> {
    let feature = d.feature_for_agent(at, named)?;
    d.add_decision(at, agent(session), feature.as_deref(), write)
}

/// Changes a decision as an agent does, under the feature its checkout is on.
fn change(d: &Daemon, at: &BoardAddress, change: DecisionChange) -> anyhow::Result<()> {
    let feature = d.feature_for_agent(at, None)?;
    d.change_decision(at, feature.as_deref(), change)
}

/// A change from the feature view, which names its feature outright.
fn from_view<P: FeaturePart>(
    d: &Daemon,
    at: &BoardAddress,
    feature: &str,
    action: P,
) -> anyhow::Result<P::List> {
    let target = BoardTarget {
        address: at,
        feature: feature.to_string(),
    };
    d.apply_part(&target, action, Asker::Person)
}

fn chose(text: &str) -> DecisionWrite {
    DecisionWrite {
        chose: text.into(),
        ..Default::default()
    }
}

#[test]
fn a_withdrawn_decision_leaves_what_agents_read_and_stays_in_the_history() {
    let (d, at) = board();
    open_feature(&d, &at, "elsewhere");
    let elsewhere = decide(&d, &at, None, None, chose("filed under another feature"))
        .unwrap()
        .id;
    open_feature(&d, &at, "here");
    let mistaken = decide(&d, &at, None, None, chose("a mistake")).unwrap().id;
    let read = |d: &Daemon| d.agent_decisions(&at, None).unwrap().decisions.len();
    assert_eq!(read(&d), 1);

    change(&d, &at, DecisionChange::Withdraw { id: mistaken }).unwrap();
    assert_eq!(read(&d), 0, "agents read what still stands");
    let history = d.decision_board_at(&at).unwrap();
    let kept = history.decisions.iter().find(|x| x.id == mistaken).unwrap();
    assert!(kept.withdrawn(), "and the view keeps it, marked");

    change(&d, &at, DecisionChange::Restore { id: mistaken }).unwrap();
    assert_eq!(read(&d), 1, "restoring undoes it");

    let refused = change(&d, &at, DecisionChange::Withdraw { id: elsewhere }).unwrap_err();
    assert!(refused.to_string().contains("not under this feature"), "{refused}");
}

#[test]
fn an_agents_tracker_map_is_a_draft_until_the_person_approves_it() {
    use argus_protocol::MapChange;

    let (d, at) = board();

    let map = d
        .agent_tracker_map(&at, agent(None), "Linear: a feature is an issue")
        .unwrap();
    assert_eq!(map.body, "", "a draft is not the map");
    assert_eq!(map.draft.unwrap().by, format!("pane #{}", AGENT.0));
    assert_eq!(d.agent_tracker_map(&at, agent(None), "  ").unwrap().body, "", "a blank body reads");

    d.change_tracker_map(&at, Asker::Person, MapChange::Approve("Linear: issues".into()))
        .unwrap();
    let board = d.decision_board_at(&at).unwrap();
    assert_eq!((board.tracker_map.body.as_str(), board.tracker_map.draft), ("Linear: issues", None));
    assert_eq!(d.agent_tracker_map(&at, agent(None), "").unwrap().body, "Linear: issues");

    let long = "x".repeat(argus_protocol::MAX_TRACKER_MAP_BYTES + 1);
    assert!(d.agent_tracker_map(&at, agent(None), &long).is_err());
}

#[test]
fn what_becomes_of_a_draft_of_the_map_is_the_persons_to_say() {
    use argus_protocol::MapChange;

    let (d, at) = board();
    d.agent_tracker_map(&at, agent(None), "Linear: a feature is an issue")
        .unwrap();

    for change in [MapChange::Approve("Linear: issues".into()), MapChange::Discard] {
        let refused = d.change_tracker_map(&at, agent(None), change).unwrap_err();
        assert!(refused.to_string().contains("person's"), "{refused}");
    }
    assert!(d.decision_board_at(&at).unwrap().tracker_map.draft.is_some(), "the draft waits");
}

#[test]
fn an_agent_reads_and_writes_a_feature_by_name_without_moving_its_checkout() {
    // Several features sharing one checkout: reaching one by moving the
    // checkout's pointer would move every other agent's with it.
    let (d, at) = board();
    let other = open_feature(&d, &at, "other work");
    let mine = open_feature(&d, &at, "my work");
    let named = Some(other.as_str());

    let board = d.feature_board(&at, named).unwrap();
    assert_eq!(board.current.as_deref(), Some(other.as_str()), "a read of it by name");

    d.agent_part_action(
        &at,
        agent(None),
        named,
        TaskAction::Add(TaskWrite {
            title: "for the other feature".into(),
            external: None,
            parent: None,
        }),
    )
    .unwrap();
    let theirs = d
        .agent_part_action(&at, agent(None), named, TaskAction::List)
        .unwrap();
    assert_eq!(theirs.tasks.len(), 1, "a write to it by name");
    let own = d
        .agent_part_action(&at, agent(None), None, TaskAction::List)
        .unwrap();
    assert!(own.tasks.is_empty(), "and not to the checkout's");

    decide(&d, &at, None, named, chose("keep them apart")).unwrap();
    assert_eq!(d.agent_decisions(&at, named).unwrap().decisions.len(), 1);

    assert_eq!(
        d.feature_board(&at, None).unwrap().current.as_deref(),
        Some(mine.as_str()),
        "the checkout stays on its own feature"
    );

    let refused = d.feature_board(&at, Some("no-such-feature")).unwrap_err();
    assert!(refused.to_string().contains("no-such-feature"), "{refused}");
}

#[test]
fn a_decision_has_nowhere_to_go_until_the_checkout_is_on_a_feature() {
    let (d, at) = board();

    let refused = decide(&d, &at, None, None, chose("sqlite"))
        .unwrap_err()
        .to_string();
    assert!(refused.contains("not on a feature"), "{refused}");
    assert!(d.decision_board_at(&at).unwrap().decisions.is_empty());

    open_feature(&d, &at, "notes storage");
    assert!(decide(&d, &at, None, None, chose("sqlite")).is_ok());
}

#[test]
fn an_agent_reads_its_own_features_decisions_and_not_the_projects() {
    let (d, at) = board();

    open_feature(&d, &at, "notes storage");
    decide(&d, &at, None, None, chose("sqlite")).unwrap();
    // The same checkout moves on to something else, the way an agent that
    // finished one feature and started another does.
    let second = open_feature(&d, &at, "the pty deadlock");
    decide(&d, &at, None, None, chose("one reader thread")).unwrap();

    let board = d.agent_decisions(&at, None).unwrap();
    assert_eq!(
        board
            .decisions
            .iter()
            .map(|d| d.chose.as_str())
            .collect::<Vec<_>>(),
        ["one reader thread"],
        "the board an agent reads is the feature it is on, not the project"
    );
    let features = d.feature_board(&at, None).unwrap();
    assert_eq!(features.current.as_deref(), Some(second.as_str()));
    assert_eq!(features.features.len(), 2, "the others are still offered");
    // The project-wide board is what the client draws, and keeps both.
    assert_eq!(d.decision_board_at(&at).unwrap().decisions.len(), 2);
}

// --- the decision board -------------------------------------------------

#[test]
fn a_decision_lands_on_its_projects_board_wherever_the_agent_was() {
    let (d, at) = board();
    open_feature(&d, &at, "notes storage");

    let root = decide(
        &d,
        &at,
        Some("sess-1"),
        None,
        DecisionWrite {
            chose: "sqlite".into(),
            over: Some("a file per feature".into()),
            because: Some("both need migrations".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let child = decide(
        &d,
        &at,
        Some("sess-1"),
        None,
        DecisionWrite {
            chose: "wal mode".into(),
            under: Some(root.id),
            ..Default::default()
        },
    )
    .unwrap();

    let board = d.decision_board_at(&at).unwrap();
    assert_eq!(board.name, at.project_name);
    assert_eq!(
        board
            .tree()
            .iter()
            .map(|(depth, d)| (*depth, d.chose.as_str()))
            .collect::<Vec<_>>(),
        [(0, "sqlite"), (1, "wal mode")]
    );
    // Attribution is the point of allowing the write at all.
    assert_eq!(child.session.as_deref(), Some("sess-1"));
    assert!(
        child.checkout.is_some(),
        "and which checkout it was made in"
    );
}

#[test]
fn a_recorded_decision_is_pushed_at_every_attached_client() {
    let (d, at) = board();
    open_feature(&d, &at, "notes storage");
    // Subscribed before the write, the way a connection is: a client that
    // has to ask again has already shown the operator a stale board.
    let mut rx = d.subscribe_decisions();

    decide(&d, &at, Some("sess-1"), None, chose("sqlite")).unwrap();

    let board = rx.try_recv().expect("the write is pushed, not waited for");
    assert_eq!(board.name, at.project_name);
    assert_eq!(board.project, Some(at.project));
    assert_eq!(
        board
            .decisions
            .iter()
            .map(|d| d.chose.as_str())
            .collect::<Vec<_>>(),
        ["sqlite"]
    );
}

#[test]
fn superseding_leaves_the_decision_it_replaced_on_the_board() {
    let (d, at) = board();
    open_feature(&d, &at, "notes storage");

    let old = decide(&d, &at, None, None, chose("key notes by id")).unwrap();
    let new = decide(
        &d,
        &at,
        None,
        None,
        DecisionWrite {
            chose: "key notes by path".into(),
            because: Some("ids are handed out fresh every start".into()),
            supersedes: Some(old.id),
            ..Default::default()
        },
    )
    .unwrap();

    let board = d.decision_board_at(&at).unwrap();
    assert_eq!(
        board.decisions.len(),
        2,
        "the old one is marked, not removed"
    );
    let old = board.decisions.iter().find(|d| d.id == old.id).unwrap();
    assert_eq!(old.superseded_by, Some(new.id));
}

#[test]
fn tasks_belong_to_the_feature_the_checkout_is_on() {
    use argus_protocol::TaskState;

    let (d, at) = board();

    let refused = d
        .agent_part_action(
            &at,
            agent(None),
            None,
            TaskAction::Add(TaskWrite {
                title: "port the parser".into(),
                external: None,
                parent: None,
            }),
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("not on a feature"), "{refused}");

    let pty = open_feature(&d, &at, "streaming the pty");
    let list = d
        .agent_part_action(
            &at,
            agent(Some("sess-1")),
            None,
            TaskAction::Add(TaskWrite {
                title: "backpressure on the reader".into(),
                external: Some("ORION-412".into()),
                parent: None,
            }),
        )
        .unwrap();
    assert_eq!(list.feature.as_deref(), Some(pty.as_str()));
    let id = list.tasks[0].id;
    assert_eq!(list.tasks[0].external.as_deref(), Some("ORION-412"));

    let list = d
        .agent_part_action(
            &at,
            agent(Some("sess-1")),
            None,
            TaskAction::Add(TaskWrite {
                title: "bound the queue".into(),
                external: None,
                parent: Some(id),
            }),
        )
        .unwrap();
    assert_eq!(
        list.tasks.iter().map(|task| task.id).collect::<Vec<_>>(),
        [id, list.tasks[1].id]
    );
    assert_eq!(list.tasks[1].parent, Some(id));

    let list = d
        .agent_part_action(
            &at,
            agent(Some("sess-1")),
            None,
            TaskAction::Move {
                id,
                state: TaskState::Doing,
            },
        )
        .unwrap();
    assert_eq!(list.tasks[0].claimed_by.as_deref(), Some("sess-1"));
    let list = d
        .agent_part_action(
            &at,
            agent(None),
            None,
            TaskAction::SetBody {
                id,
                body: "Accept sustained output.\nVerify bounded buffering.".into(),
            },
        )
        .unwrap();
    assert_eq!(
        list.tasks[0].body.as_deref(),
        Some("Accept sustained output.\nVerify bounded buffering.")
    );

    // Moving the checkout to another feature moves what it can see and
    // what it can touch, together.
    let notes = open_feature(&d, &at, "notes storage");
    let list = d
        .agent_part_action(&at, agent(None), None, TaskAction::List)
        .unwrap();
    assert_eq!(list.feature.as_deref(), Some(notes.as_str()));
    assert!(
        list.tasks.is_empty(),
        "another feature's tasks are not this one's"
    );

    let refused = d
        .agent_part_action(
            &at,
            agent(Some("sess-1")),
            None,
            TaskAction::Move {
                id,
                state: TaskState::Done,
            },
        )
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("task") && refused.contains("not under this feature"),
        "a stale id cannot tick off another feature's work: {refused}"
    );
    let refused = d
        .agent_part_action(
            &at,
            agent(None),
            None,
            TaskAction::SetBody {
                id,
                body: "rewrite the wrong task".into(),
            },
        )
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("task") && refused.contains("not under this feature"),
        "a stale id cannot rewrite another feature's brief: {refused}"
    );

    let refused = d
        .agent_part_action(
            &at,
            agent(None),
            None,
            TaskAction::Place {
                id,
                place: argus_protocol::TaskPlace::Feature(notes.clone()),
            },
        )
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("not under this feature"),
        "a stale id cannot carry another feature's task off: {refused}"
    );
}

#[test]
fn an_agent_moves_a_task_to_another_feature_and_back_and_both_lists_are_pushed() {
    use argus_protocol::TaskPlace;

    let (d, at) = board();
    let pty = open_feature(&d, &at, "the pty");
    let add = |title: &str| {
        d.agent_part_action(
            &at,
            agent(None),
            None,
            TaskAction::Add(TaskWrite {
                title: title.into(),
                external: None,
                parent: None,
            }),
        )
        .unwrap()
        .tasks
        .last()
        .unwrap()
        .id
    };
    let (first, filed_here) = (add("stays"), add("filed in the wrong place"));
    let wire = open_feature(&d, &at, "the wire");

    let mut pushed = d.subscribe_tasks();
    let left = d
        .agent_part_action(
            &at,
            agent(None),
            Some(&pty),
            TaskAction::Place {
                id: filed_here,
                place: TaskPlace::Feature(wire.clone()),
            },
        )
        .unwrap();
    assert_eq!(left.tasks.iter().map(|t| t.id).collect::<Vec<_>>(), [first]);
    let lists: Vec<_> = std::iter::from_fn(|| pushed.try_recv().ok())
        .map(|list| (list.feature.unwrap(), list.tasks.len()))
        .collect();
    assert_eq!(
        lists,
        [(pty.clone(), 1), (wire.clone(), 1)],
        "a client watching either list sees the move"
    );

    // Undone the same way: moved back, and put where it was.
    d.agent_part_action(
        &at,
        agent(None),
        Some(&wire),
        TaskAction::Place {
            id: filed_here,
            place: TaskPlace::Feature(pty.clone()),
        },
    )
    .unwrap();
    let back = d
        .agent_part_action(
            &at,
            agent(None),
            Some(&pty),
            TaskAction::Reorder {
                id: filed_here,
                to: 1,
            },
        )
        .unwrap();
    assert_eq!(
        back.tasks.iter().map(|t| t.id).collect::<Vec<_>>(),
        [first, filed_here],
        "an agent may set the order too"
    );
}

#[test]
fn a_client_cannot_touch_a_row_under_a_feature_it_did_not_name() {
    use argus_protocol::{DiagramAction, DiagramWrite, TaskState};

    let (d, at) = board();
    let pty = open_feature(&d, &at, "streaming the pty");
    let task = d
        .agent_part_action(
            &at,
            agent(None),
            None,
            TaskAction::Add(TaskWrite {
                title: "backpressure on the reader".into(),
                external: None,
                parent: None,
            }),
        )
        .unwrap()
        .tasks[0]
        .id;
    let diagram = d
        .agent_part_action(
            &at,
            agent(None),
            None,
            DiagramAction::Add(DiagramWrite {
                title: "reader".into(),
                body: "sequenceDiagram\n    A->>B: bytes".into(),
            }),
        )
        .unwrap()
        .diagrams[0]
        .id;
    let notes = open_feature(&d, &at, "notes storage");

    // A client holding the other feature's view names ids from a list it
    // is no longer showing; the same guard an agent meets refuses it.
    let refused = from_view(
        &d,
        &at,
        &notes,
        TaskAction::Move {
            id: task,
            state: TaskState::Done,
        },
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("task") && refused.contains("not under this feature"), "{refused}");
    let refused = from_view(&d, &at, &notes, DiagramAction::Remove { id: diagram })
        .unwrap_err()
        .to_string();
    assert!(refused.contains("diagram") && refused.contains("not under this feature"), "{refused}");

    // Named under its own feature, the same change goes through.
    let list = from_view(
        &d,
        &at,
        &pty,
        TaskAction::Move {
            id: task,
            state: TaskState::Done,
        },
    )
    .unwrap();
    assert_eq!(list.tasks[0].state, TaskState::Done);
}

#[test]
fn diagrams_belong_to_the_feature_the_checkout_is_on() {
    use argus_protocol::{DiagramAction, DiagramWrite};

    let (d, at) = board();

    let refused = d
        .agent_part_action(
            &at,
            agent(None),
            None,
            DiagramAction::Add(DiagramWrite {
                title: "hook path".into(),
                body: "sequenceDiagram\n    A->>B: ping".into(),
            }),
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("not on a feature"), "{refused}");

    let slug = open_feature(&d, &at, "diagram hooks");
    let list = d
        .agent_part_action(
            &at,
            agent(Some("sess-1")),
            None,
            DiagramAction::Add(DiagramWrite {
                title: "list and add".into(),
                body: "sequenceDiagram\n    Hook->>Daemon: POST /diagrams".into(),
            }),
        )
        .unwrap();
    assert_eq!(list.feature.as_deref(), Some(slug.as_str()));
    let id = list.diagrams[0].id;
    assert_eq!(list.diagrams[0].title, "list and add");

    let list = d
        .agent_part_action(
            &at,
            agent(Some("sess-2")),
            None,
            DiagramAction::Rewrite {
                id,
                write: DiagramWrite {
                    title: "list, add and rewrite".into(),
                    body: "sequenceDiagram\n    Hook->>Daemon: POST /diagrams %% decision #1".into(),
                },
            },
        )
        .unwrap();
    assert_eq!(list.diagrams.len(), 1, "a rewrite replaces the flow in place");
    assert_eq!((list.diagrams[0].id, list.diagrams[0].title.as_str()), (id, "list, add and rewrite"));

    let list = d
        .agent_part_action(&at, agent(None), None, DiagramAction::Remove { id })
        .unwrap();
    assert!(list.diagrams.is_empty(), "drop removes the row");
}

// --- open decisions, and what a task says about its own progress ----------

#[test]
fn a_task_says_which_pane_took_it_up_and_a_person_names_none() {
    use argus_protocol::TaskState;

    let (d, at) = board();
    let slug = open_feature(&d, &at, "the pty");
    let as_agent = |action: TaskAction| d.agent_part_action(&at, agent(Some("sess-1")), None, action);
    let as_person = |action: TaskAction| from_view(&d, &at, &slug, action);
    let id = as_agent(TaskAction::Add(TaskWrite { title: "port the parser".into(), ..Default::default() }))
        .unwrap()
        .tasks[0]
        .id;

    let taken = as_agent(TaskAction::Move { id, state: TaskState::Doing }).unwrap().tasks.remove(0);
    assert_eq!((taken.claimed_by.as_deref(), taken.claimed_pane), (Some("sess-1"), Some(AGENT)));
    as_agent(TaskAction::Note { id, text: "lexer done".into() }).unwrap();
    let noted = as_person(TaskAction::Note { id, text: "ship the lexer first".into() })
        .unwrap()
        .tasks
        .remove(0);
    let who: Vec<_> = noted.notes.iter().map(|n| (n.session.as_deref(), n.pane)).collect();
    assert_eq!(who, [(Some("sess-1"), Some(AGENT)), (None, None)]);

    let released = as_agent(TaskAction::Move { id, state: TaskState::Done }).unwrap().tasks.remove(0);
    assert_eq!((released.claimed_by, released.claimed_pane), (None, None));
    let by_hand = as_person(TaskAction::Move { id, state: TaskState::Doing }).unwrap().tasks.remove(0);
    assert_eq!((by_hand.claimed_by, by_hand.claimed_pane), (None, None), "a person claims nothing");

    // Criteria travel the same path, and a place that is not there is
    // refused in words rather than ignored.
    as_agent(TaskAction::AddCriterion { id, text: "round-trips a fixture".into() }).unwrap();
    let checked = as_person(TaskAction::SetCriterion { id, index: 0, done: true }).unwrap();
    assert!(checked.tasks[0].criteria[0].done);
    let error = as_agent(TaskAction::SetCriterion { id, index: 1, done: true }).unwrap_err().to_string();
    assert_eq!(error, "task 1 has no criterion 2; it has 1");

    // And like every write naming a row, only under the feature asked about.
    open_feature(&d, &at, "the wire");
    for elsewhere in [
        TaskAction::Note { id, text: "from the wrong feature".into() },
        TaskAction::AddCriterion { id, text: "from the wrong feature".into() },
        TaskAction::RemoveCriterion { id, index: 0 },
    ] {
        let error = as_agent(elsewhere).unwrap_err().to_string();
        assert!(error.contains("is not under this feature"), "{error}");
    }
}

#[test]
fn a_person_decides_and_changes_decisions_only_under_the_feature_they_name() {
    use argus_protocol::{DecisionState, DecisionText};

    let (d, at) = board();
    let pty = open_feature(&d, &at, "the pty");
    let wire = open_feature(&d, &at, "the wire");
    let mut pushed = d.subscribe_decisions();

    let question = DecisionWrite {
        chose: "which framing?".into(),
        state: DecisionState::Open,
        options: vec!["length-prefixed".into(), "delimited".into()],
        ..Default::default()
    };
    d.add_decision(&at, Asker::Person, Some(&wire), question.clone()).unwrap();
    let board = pushed.try_recv().expect("a decision from the view is pushed like any other");
    let asked = board.decisions[0].clone();
    assert_eq!((asked.feature.as_deref(), &asked.session, &asked.checkout), (Some(wire.as_str()), &None, &None));
    assert_eq!(board.features.iter().map(|f| f.number).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(board.standing_for(Some(&wire)), (0, 1));

    let refused = d.add_decision(&at, Asker::Person, Some("gone"), question).unwrap_err().to_string();
    assert!(refused.contains("no feature gone"), "{refused}");

    let reword = DecisionChange::Rewrite {
        id: asked.id,
        text: DecisionText { chose: "how are frames delimited?".into(), ..DecisionText::from(&asked) },
    };
    let wrong_feature = d
        .change_decision(&at, Some(&pty), reword.clone())
        .unwrap_err()
        .to_string();
    assert!(wrong_feature.contains("is not under this feature"), "{wrong_feature}");
    d.change_decision(&at, Some(&wire), reword).unwrap();
    let resolve = DecisionChange::Resolve { id: asked.id, answer: "delimited".into(), because: None };
    d.change_decision(&at, Some(&wire), resolve).unwrap();

    let made = d.decision_board_at(&at).unwrap().decisions.remove(0);
    assert!(made.stands());
    assert_eq!(
        (made.chose.as_str(), made.detail.as_deref(), made.over.as_deref()),
        ("how are frames delimited?", Some("delimited"), Some("length-prefixed"))
    );
}

#[test]
fn a_reconcile_turn_is_refused_from_the_side_it_is_not_for() {
    use crate::state::reconcile::Turn;
    use argus_protocol::{ProposalWrite, ProposedLine, ReconcileAction, ReconcileChange};

    let (d, at) = board();
    let slug = open_feature(&d, &at, "notes storage");
    let propose = || ReconcileChange::Propose {
        proposal: ProposalWrite {
            asked: "tidy the tickets".into(),
            lines: vec![ProposedLine {
                to: "Jira".into(),
                key: "ARG-1".into(),
                does: "close".into(),
                ..ProposedLine::default()
            }],
            noticed: Vec::new(),
        },
    };
    d.apply_reconcile(&at, agent(None), &slug, Turn::Agent(propose()))
        .unwrap();
    let before = d.store.reconcile(&at.key, &slug).unwrap();
    assert!(before.0.is_some());

    let refused = d
        .apply_reconcile(&at, Asker::Person, &slug, Turn::Agent(propose()))
        .unwrap_err();
    assert!(refused.to_string().contains("an agent reads"), "{refused}");
    let refused = d
        .apply_reconcile(&at, agent(None), &slug, Turn::Person(ReconcileAction::Discard))
        .unwrap_err();
    assert!(refused.to_string().contains("the person asks"), "{refused}");
    assert_eq!(
        d.store.reconcile(&at.key, &slug).unwrap(),
        before,
        "neither refusal touched the reconcile"
    );
}
