//! Every write the store makes, failed at every row it writes and at its
//! commit, and failed again by a full disk or another writer's lock: what
//! is left is the store before the write or after it, never a mix, and a
//! failure is an error, never a panic.
//!
//! The rule is the store's own (`store/mod.rs`): "a write is a transaction
//! rather than a rewrite of everything". These are the writes where a
//! half would show: the pane table replaced "in one transaction" (Session
//! restore), a feature's slug "made unique by suffix inside the
//! transaction" and a capture written "in one transaction" (Features and
//! the decision board), a removed feature dropping its views "in the same
//! transaction" (saved views), and a transfer written "in one store
//! transaction". `WRITES` is every write but two, each of which finishes
//! when run again: a migration runs in one transaction with the
//! `user_version` it reaches, and the one-time import of legacy files
//! inserts with `OR IGNORE` and retires a file only once it is in.
//!
//! A known gap, allowed here: a write whose commit landed can still return
//! an error when a read it makes afterwards fails, such as the board it
//! reads back for its answer. Nothing is left half done, but the daemon
//! refuses a request whose write was saved, and a client that retries
//! writes it twice. The intended fix is that a write answers from what it
//! committed and logs the failed read.

use std::panic::{catch_unwind, AssertUnwindSafe};

use argus_protocol::{
    Actor, Capture, CapturedDecision, CapturedTask, DecisionState, DecisionText, DecisionWrite, DiagramWrite,
    FeatureMove, FeatureState, Reconciled, ReviewAnchor, ReviewBase, TaskPlace, TaskState, TaskWrite,
};
use proptest::prelude::*;

use super::super::faults::Fault;
use super::{chose, feature, kept_view, pane, task, ticket, under_way};
use super::*;

const BOARD: &str = "argus";
const REPO: &str = "/home/me/src/argus";
const WORKTREE: &str = "/home/me/src/argus-wt";

/// What the fixture made, for the writes to name.
struct Fx {
    pty: String,
    other: String,
    third: String,
    parent: i64,
    child: i64,
    sibling: i64,
    open: i64,
    accepted: i64,
    diagram: i64,
}

/// A board with something of everything a write can touch, made the same
/// way every time so two stores built from it are row for row the same.
fn fixture(s: &Store) -> Fx {
    s.add_project(&ProjectOverlay {
        name: "src".into(),
        root: PathBuf::from("/home/me/src"),
        workspace: "default".into(),
        exclude: vec!["target".into()],
    })
    .unwrap();
    s.add_repo("src", Path::new(REPO)).unwrap();
    s.exclude_repo(Path::new("/home/me/src/old")).unwrap();
    s.add_workspace("work").unwrap();
    s.set_open_workspace("work").unwrap();
    s.set_repository_used(Path::new(REPO), 7).unwrap();
    s.save_panes(&[pane(REPO, PaneKind::Shell, "shell"), pane(REPO, PaneKind::Agent, "agent")]).unwrap();
    s.save_consoles(2).unwrap();
    s.saw_sessions(&[sighting("s1")], 8).unwrap();

    let pty = s.add_feature(BOARD, &feature("The pty"), Some(REPO), Some("main"), 1, Some("s1")).unwrap().slug;
    let other = s.add_feature(BOARD, &feature("Second"), None, None, 2, None).unwrap().slug;
    let third = s.add_feature(BOARD, &feature("Third"), None, None, 2, None).unwrap().slug;
    let parent = s.add_task(BOARD, &pty, &task("parent"), 3, Some("s1")).unwrap().id;
    let child = s
        .add_task(BOARD, &pty, &TaskWrite { parent: Some(parent), ..task("child") }, 3, None)
        .unwrap()
        .id;
    let sibling = s.add_task(BOARD, &pty, &task("sibling"), 3, None).unwrap().id;
    s.add_task_criterion(BOARD, parent, "a full queue refuses").unwrap();
    s.add_task_criterion(BOARD, parent, "a drained one accepts").unwrap();
    s.add_task_note(BOARD, parent, "half way", 4, Some("s1"), Some(PaneId(3))).unwrap();
    s.move_task(BOARD, child, TaskState::Doing, Some("s1"), Some(PaneId(3))).unwrap();
    let accepted = s.add_decision(BOARD, &chose("sqlite"), Some(&pty), 5, None, None).unwrap();
    let open = s
        .add_decision(
            BOARD,
            &DecisionWrite { state: DecisionState::Open, ..chose("which queue") },
            Some(&pty),
            5,
            None,
            None,
        )
        .unwrap();
    let diagram = s
        .add_sequence_diagram(BOARD, &pty, &diagram_write("resize"), 6, None)
        .unwrap()
        .id;
    s.link_feature(BOARD, &pty, &ticket("linear", "ENG-1")).unwrap();
    s.add_feature_wait(BOARD, &pty, &other).unwrap();
    s.set_tracker_map(BOARD, "ENG is linear").unwrap();
    s.draft_tracker_map(BOARD, "ENG is jira", "pane #3", 7).unwrap();
    s.set_feature_scope(Path::new(REPO), BOARD, &pty).unwrap();
    s.set_artifact_feature_scope(Path::new(REPO), BOARD, &pty).unwrap();
    s.set_reconcile(BOARD, &pty, Some(&under_way(true))).unwrap();
    s.save_view(&kept_view("deps", Some(BOARD))).unwrap();
    s.add_review_comment(Path::new(REPO), anchor(), "why here?".into()).unwrap();
    Fx { pty, other, third, parent, child, sibling, open, accepted, diagram }
}

fn sighting(id: &str) -> Sighting {
    Sighting {
        harness: "claude".into(),
        session_id: id.into(),
        checkout_path: PathBuf::from(REPO),
        title: "agent".into(),
    }
}

fn diagram_write(title: &str) -> DiagramWrite {
    DiagramWrite { title: title.into(), body: "sequenceDiagram\n    A->>B: hi".into() }
}

fn anchor() -> ReviewAnchor {
    ReviewAnchor {
        base: ReviewBase::Unstaged,
        commit: None,
        path: "src/lib.rs".into(),
        old_path: None,
        old_start: Some(1),
        old_end: Some(2),
        new_start: Some(1),
        new_end: Some(3),
        text: vec!["fn main() {}".into()],
    }
}

type Run = fn(&Store, &Fx) -> Result<()>;

/// Every write the store makes, each as the daemon makes it, with the
/// fixture's rows to work on.
const WRITES: &[(&str, Run)] = &[
    // The board: features.
    ("add_feature", |s, _| s.add_feature(BOARD, &feature("The pty"), Some(REPO), None, 9, None).map(drop)),
    ("open_feature_in", |s, _| {
        s.open_feature_in(BOARD, &feature("From the worktree"), Path::new(WORKTREE), Some("wt"), 9, Some("s2"))
            .map(drop)
    }),
    ("capture_feature", |s, _| {
        let capture = Capture {
            title: "Bound the queue".into(),
            body: Some("A full queue refuses.".into()),
            tasks: vec![
                CapturedTask {
                    title: "refuse".into(),
                    subtasks: vec![CapturedTask { title: "say why".into(), ..Default::default() }],
                    decisions: vec![CapturedDecision { chose: "a bound".into(), ..Default::default() }],
                },
                CapturedTask { title: "drain".into(), ..Default::default() },
            ],
            decisions: vec![CapturedDecision {
                chose: "how big".into(),
                open: true,
                options: vec!["64".into(), "1024".into()],
                because: None,
            }],
            ..Default::default()
        };
        s.capture_feature(BOARD, None, &capture, 9).map(drop)
    }),
    ("capture_feature into", |s, f| {
        let capture = Capture {
            title: "more".into(),
            tasks: vec![CapturedTask { title: "and this".into(), ..Default::default() }],
            ..Default::default()
        };
        s.capture_feature(BOARD, Some(&f.pty), &capture, 9).map(drop)
    }),
    ("append_to_feature", |s, f| s.append_to_feature(BOARD, &f.pty, "more").map(drop)),
    ("remove_feature", |s, f| s.remove_feature(BOARD, &f.pty)),
    ("rename_feature", |s, f| s.rename_feature(BOARD, &f.pty, "The terminal")),
    ("set_feature_body", |s, f| s.set_feature_body(BOARD, &f.pty, "new body")),
    ("hold_feature", |s, f| s.hold_feature(BOARD, &f.pty, Some("waiting on review"))),
    ("add_feature_wait", |s, f| s.add_feature_wait(BOARD, &f.pty, &f.third)),
    ("remove_feature_wait", |s, f| s.remove_feature_wait(BOARD, &f.pty, &f.other)),
    ("link_feature", |s, f| s.link_feature(BOARD, &f.pty, &ticket("linear", "ENG-2"))),
    ("unlink_feature", |s, f| s.unlink_feature(BOARD, &f.pty, "ENG-1")),
    ("move_feature", |s, f| {
        let mv = FeatureMove {
            state: FeatureState::Done,
            detail: Some("green".into()),
            actor: Actor::Human,
            session: None,
            at: 9,
        };
        s.move_feature(BOARD, &f.pty, &mv).map(drop)
    }),
    ("set_feature_scope", |s, f| s.set_feature_scope(Path::new(WORKTREE), BOARD, &f.other)),
    ("set_artifact_feature_scope", |s, f| s.set_artifact_feature_scope(Path::new(WORKTREE), BOARD, &f.other)),
    ("transfer_artifact_feature_scope", |s, f| {
        s.transfer_artifact_feature_scope(BOARD, &f.pty, Path::new(REPO), Path::new(WORKTREE))
    }),
    // The board: tasks.
    ("add_task", |s, f| s.add_task(BOARD, &f.pty, &task("new"), 9, None).map(drop)),
    ("add_task under", |s, f| {
        s.add_task(BOARD, &f.pty, &TaskWrite { parent: Some(f.child), ..task("deeper") }, 9, None).map(drop)
    }),
    ("move_task", |s, f| s.move_task(BOARD, f.parent, TaskState::Done, Some("s1"), Some(PaneId(3)))),
    ("add_task_criterion", |s, f| s.add_task_criterion(BOARD, f.sibling, "it drains")),
    ("set_task_criterion", |s, f| s.set_task_criterion(BOARD, f.parent, 1, true)),
    ("remove_task_criterion", |s, f| s.remove_task_criterion(BOARD, f.parent, 0)),
    ("add_task_note", |s, f| s.add_task_note(BOARD, f.parent, "done now", 9, None, None)),
    ("retitle_task", |s, f| s.retitle_task(BOARD, f.parent, "the parent")),
    ("set_task_body", |s, f| s.set_task_body(BOARD, f.parent, "why".into())),
    ("remove_task", |s, f| s.remove_task(BOARD, f.parent)),
    ("reorder_task", |s, f| s.reorder_task(BOARD, f.sibling, 0)),
    ("place_task under", |s, f| s.place_task(BOARD, f.parent, &TaskPlace::Under(f.sibling)).map(drop)),
    ("place_task feature", |s, f| s.place_task(BOARD, f.parent, &TaskPlace::Feature(f.other.clone())).map(drop)),
    ("forget_panes", |s, _| s.forget_panes()),
    // The board: decisions, diagrams, the tracker map, reconciling.
    ("add_decision", |s, f| s.add_decision(BOARD, &chose("wal"), Some(&f.pty), 9, None, None).map(drop)),
    ("add_decision superseding", |s, f| {
        let write = DecisionWrite { supersedes: Some(f.accepted), ..chose("postgres") };
        s.add_decision(BOARD, &write, Some(&f.pty), 9, None, None).map(drop)
    }),
    ("set_decision_withdrawn", |s, f| s.set_decision_withdrawn(BOARD, f.accepted, Some(9))),
    ("resolve_decision", |s, f| s.resolve_decision(BOARD, f.open, "a ring", Some("bounded"))),
    ("rewrite_decision", |s, f| {
        let text = DecisionText { chose: "sqlite in wal".into(), ..Default::default() };
        s.rewrite_decision(BOARD, f.accepted, text)
    }),
    ("add_sequence_diagram", |s, f| s.add_sequence_diagram(BOARD, &f.pty, &diagram_write("drag"), 9, None).map(drop)),
    ("rewrite_sequence_diagram", |s, f| {
        s.rewrite_sequence_diagram(BOARD, &f.pty, f.diagram, &diagram_write("resize again"), 9, None)
    }),
    ("remove_sequence_diagram", |s, f| s.remove_sequence_diagram(BOARD, &f.pty, f.diagram)),
    ("set_tracker_map", |s, _| s.set_tracker_map(BOARD, "ENG is github")),
    ("draft_tracker_map", |s, _| s.draft_tracker_map(BOARD, "ENG is gitlab", "pane #4", 9)),
    ("approve_tracker_map", |s, _| s.approve_tracker_map(BOARD, "ENG is jira")),
    ("discard_tracker_map_draft", |s, _| s.discard_tracker_map_draft(BOARD)),
    ("set_reconcile", |s, f| s.set_reconcile(BOARD, &f.pty, Some(&under_way(false)))),
    ("finish_reconcile", |s, f| {
        let done = Reconciled { at: 9, by: "pane #32".into(), seen: Vec::new() };
        s.finish_reconcile(BOARD, &f.pty, None, &done)
    }),
    // Saved views and review comments.
    ("save_view", |s, _| s.save_view(&StoredView { built_at: 11, ..kept_view("deps", Some(BOARD)) })),
    ("save_view new", |s, _| s.save_view(&kept_view("ci", None))),
    ("delete_saved_view", |s, _| s.delete_saved_view("deps").map(drop)),
    ("mark_saved_view_stale", |s, _| s.mark_saved_view_stale("deps", true).map(drop)),
    ("add_review_comment", |s, _| s.add_review_comment(Path::new(REPO), anchor(), "and here".into()).map(drop)),
    // The panel and the session.
    ("add_project", |s, _| {
        s.add_project(&ProjectOverlay {
            name: "work".into(),
            root: PathBuf::from("/home/me/work"),
            workspace: "work".into(),
            exclude: Vec::new(),
        })
    }),
    ("remove_project", |s, _| s.remove_project("src", Some(Path::new("/home/me/src")))),
    ("add_repo", |s, _| s.add_repo("src", Path::new("/home/me/src/new"))),
    ("exclude_repo", |s, _| s.exclude_repo(Path::new("/home/me/src/gone"))),
    ("set_excluded_repos", |s, _| {
        s.set_excluded_repos(&[PathBuf::from("/home/me/src/a"), PathBuf::from("/home/me/src/b")])
    }),
    ("set_repository_used", |s, _| s.set_repository_used(Path::new(REPO), 9)),
    ("add_workspace", |s, _| s.add_workspace("play")),
    ("set_open_workspace", |s, _| s.set_open_workspace("default")),
    ("save_panes", |s, _| s.save_panes(&[pane(WORKTREE, PaneKind::Agent, "agent")])),
    ("save_consoles", |s, _| s.save_consoles(3)),
    ("saw_sessions", |s, _| s.saw_sessions(&[sighting("s1"), sighting("s2")], 9)),
];

/// Every row of every table, `sqlite_sequence` included, so a write that
/// was rolled back must have given back the ids it took as well.
fn everything(s: &Store) -> Vec<String> {
    let conn = s.conn();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut rows = Vec::new();
    for table in tables {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let width = stmt.column_count();
        let mut read: Vec<String> = stmt
            .query_map([], |r| {
                let cells: Vec<String> = (0..width).map(|i| cell(r.get_ref(i).unwrap())).collect();
                Ok(format!("{table}: {}", cells.join(" | ")))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        read.sort();
        rows.extend(read);
    }
    rows
}

fn cell(value: rusqlite::types::ValueRef<'_>) -> String {
    match value {
        rusqlite::types::ValueRef::Text(text) => format!("{:?}", String::from_utf8_lossy(text)),
        other => format!("{other:?}"),
    }
}

/// The store a write starts from and the one it leaves when nothing fails,
/// and how many steps it takes on the way.
struct Expected {
    before: Vec<String>,
    after: Vec<String>,
    steps: usize,
}

fn expected(run: Run) -> Result<Expected> {
    let s = store();
    let fx = fixture(&s);
    let before = everything(&s);
    let (done, steps) = s.steps_taken(|s| run(s, &fx));
    done?;
    Ok(Expected { before, after: everything(&s), steps })
}

/// What a fault left of a write that holds to the rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Left {
    /// None of it: the write failed, and a retry finished it.
    Nothing,
    /// All of it: the fault came too late to stop it.
    All,
}

/// Runs `run` against `s` with `fault` in place and checks what is left.
/// Returns why it does not hold, if it does not.
fn check(s: &Store, fx: &Fx, name: &str, run: Run, fault: Fault, want: &Expected) -> Result<Left, String> {
    let armed = s.inject(fault);
    let done = catch_unwind(AssertUnwindSafe(|| run(s, fx)));
    drop(armed);
    let Ok(done) = done else {
        return Err(format!("{name} under {fault:?} panicked"));
    };
    let left = everything(s);
    if left == want.after {
        // A write that landed may still report a failure from a read after
        // its commit; that is not half a write.
        return Ok(Left::All);
    }
    if left != want.before {
        let mixed: Vec<_> = left.iter().filter(|r| !want.before.contains(r) && !want.after.contains(r)).collect();
        let lost: Vec<_> = want.after.iter().filter(|r| !left.contains(r)).collect();
        return Err(format!(
            "{name} under {fault:?} left half a write.\nrows in neither: {mixed:#?}\nrows of the write missing: {lost:#?}"
        ));
    }
    if done.is_ok() && want.before != want.after {
        return Err(format!("{name} under {fault:?} wrote nothing and said it had"));
    }
    // Nothing was written, so the same write once the fault is gone
    // finishes it: a failed write leaves nothing in the way of a retry.
    run(s, fx).map_err(|e| format!("{name} after {fault:?}: the retry failed: {e:#}"))?;
    if everything(s) != want.after {
        return Err(format!("{name} after {fault:?}: the retry did not finish the write"));
    }
    Ok(Left::Nothing)
}

#[test]
fn every_write_is_whole_or_absent_when_failed_at_any_step_or_its_commit() {
    let mut failures = Vec::new();
    let (mut cases, mut stopped) = (0, 0);
    for &(name, run) in WRITES {
        let want = match expected(run) {
            Ok(want) if want.before != want.after => want,
            Ok(_) => {
                failures.push(format!("{name} changes nothing in the fixture, so it tests nothing"));
                continue;
            }
            Err(e) => {
                failures.push(format!("{name} fails with nothing failing it: {e:#}"));
                continue;
            }
        };
        let faults = (0..=want.steps + 1).map(Fault::AfterSteps).chain([Fault::Commit]);
        for fault in faults {
            cases += 1;
            let s = store();
            let fx = fixture(&s);
            match check(&s, &fx, name, run, fault, &want) {
                Ok(Left::Nothing) => stopped += 1,
                // Before the first step and at the commit, every write can be
                // stopped: if one is not, the fault never reached it.
                Ok(Left::All) if matches!(fault, Fault::AfterSteps(0) | Fault::Commit) => {
                    failures.push(format!("{name} under {fault:?} was not stopped"))
                }
                Ok(Left::All) => {}
                Err(why) => failures.push(why),
            }
        }
    }
    eprintln!("{cases} cases over {} writes; {stopped} stopped a write", WRITES.len());
    assert!(failures.is_empty(), "{} of {cases} cases:\n{}", failures.len(), failures.join("\n\n"));
}

/// An agent's feature is opened in the checkout the agent works in, and
/// that checkout points at it: stopped at any step or at its commit, the
/// open leaves no feature, or the feature with its checkout on it, never a
/// feature its own checkout is not on. The owner's decision of 2026-10-18,
/// recorded here.
#[test]
fn an_agents_feature_is_never_left_without_its_checkout() {
    let open = |s: &Store| {
        s.open_feature_in(BOARD, &feature("From the worktree"), Path::new(WORKTREE), Some("wt"), 9, Some("s2"))
    };
    let steps = {
        let s = store();
        fixture(&s);
        let (opened, steps) = s.steps_taken(open);
        opened.unwrap();
        steps
    };
    let mut stopped = 0;
    for fault in (0..=steps + 1).map(Fault::AfterSteps).chain([Fault::Commit]) {
        let s = store();
        fixture(&s);
        let armed = s.inject(fault);
        let _ = open(&s);
        drop(armed);
        let opened: Vec<String> = s
            .features(BOARD)
            .unwrap()
            .into_iter()
            .filter(|f| f.title == "From the worktree")
            .map(|f| f.slug)
            .collect();
        let pointed = s.artifact_feature_scope(Path::new(WORKTREE), BOARD).unwrap();
        match opened.as_slice() {
            [] => stopped += 1,
            [slug] => assert_eq!(pointed.as_ref(), Some(slug), "under {fault:?} the checkout is not on its feature"),
            more => panic!("under {fault:?} it opened {more:?}"),
        }
    }
    assert!(stopped > 0, "no fault stopped the open, so the test never saw one");
}

mod properties {
    use super::*;

    fn fault() -> impl Strategy<Value = Fault> {
        prop_oneof![
            4 => (0..60usize).prop_map(Fault::AfterSteps),
            1 => Just(Fault::Commit),
            2 => (0..4u32).prop_map(|pages| Fault::Full { pages }),
            1 => Just(Fault::Busy),
        ]
    }

    proptest! {
        /// The same rule against a store in a file, as the daemon keeps
        /// it, with the faults a disk and another process bring: a full
        /// disk and a write lock held elsewhere, beside an interrupt at any
        /// row and a failed commit.
        #[test]
        fn any_fault_leaves_a_write_whole_or_absent(write in 0..WRITES.len(), fault in fault()) {
            let (name, run) = WRITES[write];
            let want = expected(run).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let s = Store::open_at(&dir.path().join("runtime.db")).unwrap();
            let fx = fixture(&s);
            prop_assert_eq!(&everything(&s), &want.before, "a file store starts where a memory one does");
            check(&s, &fx, name, run, fault, &want).map_err(TestCaseError::fail)?;
        }
    }
}

/// What the seam stands on: each fault fails a write while it is in
/// place, and leaves the store working once it is gone.
mod seam {
    use super::*;

    fn fails(s: &Store, fault: Fault, write: impl Fn(&Store) -> Result<()>) -> String {
        let armed = s.inject(fault);
        let failed = write(s).expect_err("the write fails while the fault is in place");
        drop(armed);
        write(s).expect("the same write succeeds once the fault is gone");
        format!("{failed:#}")
    }

    #[test]
    fn each_fault_fails_a_write_until_it_is_removed() {
        let s = store();
        let fx = fixture(&s);
        let add = |s: &Store| s.add_task(BOARD, &fx.pty, &task("new"), 9, None).map(drop);
        // Failed where it prepares a statement, or between two steps of one.
        let stopped = |why: String| why.contains("not authorized") || why.contains("interrupted");
        assert!(stopped(fails(&s, Fault::AfterSteps(0), add)));
        assert!(stopped(fails(&s, Fault::AfterSteps(1), add)));
        // SQLITE_CONSTRAINT_COMMITHOOK: the commit was turned into a rollback.
        assert!(fails(&s, Fault::Commit, add).contains("531"));
        // A body larger than a page has to take one the disk no longer has.
        let big = |s: &Store| s.set_feature_body(BOARD, &fx.pty, &"x".repeat(argus_protocol::MAX_FEATURE_BODY_BYTES));
        assert!(fails(&s, Fault::Full { pages: 0 }, big).contains("full"));
    }

    #[test]
    fn a_lock_held_elsewhere_fails_a_write_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open_at(&dir.path().join("runtime.db")).unwrap();
        let fx = fixture(&s);
        let started = std::time::Instant::now();
        let failed = fails(&s, Fault::Busy, |s| s.add_task(BOARD, &fx.pty, &task("new"), 9, None).map(drop));
        assert!(failed.contains("locked"), "{failed}");
        assert!(started.elapsed() < BUSY_TIMEOUT, "it waited out the lock instead");
    }
}

/// The daemon's side of the same rule: a write the store fails is
/// answered with the failure, the connection lives on, and the request
/// works once the fault is gone (AGENTS.md: the client never predicts a
/// result, so the refusal is all it has to go on).
#[tokio::test]
async fn a_failed_write_is_refused_and_the_connection_lives_on() {
    use argus_protocol::{ClientMsg, FeatureAction, FeatureWrite, ServerMsg};

    use crate::test_host::transcript::Transcript;
    use crate::test_host::Host;

    let host = Host::start().await;
    let (project, checkout) = (host.project(), host.checkout());
    let mut t = Transcript::greeted(&host).await;
    let open = || ClientMsg::Feature {
        project,
        checkout,
        action: FeatureAction::Open(FeatureWrite { title: "Bound the queue".into(), body: None }),
        request_id: 1,
    };
    let opened = |m: &ServerMsg| matches!(m, ServerMsg::Decisions(_));
    let refused = |m: &ServerMsg| matches!(m, ServerMsg::Refused(r) if r.request_id == 1);

    // The faults that fail every write: before its first row, at its
    // commit, and behind a lock. One later in a write may land after the
    // last row's statement and leave it whole, which the rule allows.
    for fault in [Fault::AfterSteps(0), Fault::Commit, Fault::Busy] {
        let armed = host.daemon.store().inject(fault);
        let heard = t.ask(open(), refused).await;
        drop(armed);
        assert!(!heard.iter().any(opened), "{fault:?}: a failed open still pushed a board: {heard:#?}");
        let features: i64 =
            host.daemon.store().conn().query_row("SELECT COUNT(*) FROM feature", [], |r| r.get(0)).unwrap();
        assert_eq!(features, 0, "{fault:?} left a feature behind");
    }
    t.ask(open(), opened).await;
    host.stop().await;
}
