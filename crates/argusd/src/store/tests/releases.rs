//! Stores written by released builds, opened by this one.
//!
//! The migration tests beside this build their old stores from today's
//! `SCHEMA_Vn` constants, so they cannot see a constant edited after it
//! shipped, or rows an old build wrote differently from how today's code
//! would. Each fixture here was written by its release's own `Store`;
//! `README.md` §"Publishing a Release" says when one is added. Paths the
//! migrations read off the disk are placeholders, filled in with a real
//! repository before the store is opened.

use std::path::PathBuf;

use super::*;

const RELEASES: &[&str] = &["v0.10.0", "v0.16.0", "v0.22.11", "v0.28.2", "v0.33.0"];

/// A copy of the release's store, since opening one migrates it in place.
fn copied(release: &str) -> (tempfile::TempDir, PathBuf) {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/stores")
        .join(format!("runtime-{release}.db"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runtime.db");
    std::fs::copy(&fixture, &path).unwrap_or_else(|e| panic!("{}: {e}", fixture.display()));
    (dir, path)
}

fn opened(release: &str) -> (tempfile::TempDir, Store) {
    let (dir, path) = copied(release);
    let store = Store::open_at(&path).unwrap_or_else(|e| panic!("{release}: {e:#}"));
    (dir, store)
}

/// Rewrites every text value in the store, before it is opened, so a
/// fixture's placeholder paths name directories that exist here.
fn fill_in(path: &Path, placeholders: &[(&str, String)]) {
    let conn = Connection::open(path).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for table in tables {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let width = stmt.column_count();
        let rows: Vec<Vec<rusqlite::types::Value>> = stmt
            .query_map([], |r| (0..width).map(|i| r.get(i)).collect())
            .unwrap()
            .map(Result::unwrap)
            .collect();
        drop(stmt);
        let filled: Vec<Vec<rusqlite::types::Value>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| match value {
                        rusqlite::types::Value::Text(text) => {
                            let mut text = text.clone();
                            for (placeholder, real) in placeholders {
                                text = text.replace(placeholder, real);
                            }
                            rusqlite::types::Value::Text(text)
                        }
                        other => other.clone(),
                    })
                    .collect()
            })
            .collect();
        if filled == rows {
            continue;
        }
        conn.execute(&format!("DELETE FROM \"{table}\""), []).unwrap();
        let slots = vec!["?"; width].join(", ");
        let mut insert = conn.prepare(&format!("INSERT INTO \"{table}\" VALUES ({slots})")).unwrap();
        for row in filled {
            insert.execute(rusqlite::params_from_iter(row)).unwrap();
        }
    }
}

/// A repository with one commit on `main` and a linked worktree on
/// `fixture-branch`, laid out the way the v0.16.0 fixture's was.
fn repository_and_worktree(root: &Path) -> (PathBuf, PathBuf) {
    let repo_path = root.join("repos/fixture-repo");
    let worktree_path = root.join("repos/fixture-repo-wt");
    let mut init = git2::RepositoryInitOptions::new();
    init.initial_head("main");
    let repo = git2::Repository::init_opts(&repo_path, &init).unwrap();
    let signature = git2::Signature::now("fixture", "fixture@example.com").unwrap();
    let tree = repo.find_tree(repo.index().unwrap().write_tree().unwrap()).unwrap();
    let commit = repo.commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[]).unwrap();
    let branch = repo.branch("fixture-branch", &repo.find_commit(commit).unwrap(), false).unwrap();
    let mut add = git2::WorktreeAddOptions::new();
    add.reference(Some(branch.get()));
    repo.worktree("fixture-repo-wt", &worktree_path, Some(&add)).unwrap();
    (repo_path, worktree_path)
}

/// v0.16.0 with its repository and worktree real, so the migrations that
/// read Git directories off the disk find them.
fn opened_v0_16_0() -> (tempfile::TempDir, Store, PathBuf, PathBuf) {
    let (dir, path) = copied("v0.16.0");
    let (repo, worktree) = repository_and_worktree(dir.path());
    // A key holds a Git directory as the platform spells it; a checkout is
    // stored the way the store writes every path.
    let key = |path: &Path| path.canonicalize().unwrap().to_string_lossy().into_owned();
    let git_dir = repo.join(".git");
    fill_in(
        &path,
        &[
            ("@worktree-git-dir@", key(&git_dir.join("worktrees/fixture-repo-wt"))),
            ("@git-dir@", key(&git_dir)),
            ("@worktree@", path_text(&worktree)),
            ("@repo@", path_text(&repo)),
            ("@root@", path_text(dir.path())),
        ],
    );
    let store = Store::open_at(&path).unwrap();
    (dir, store, repo, worktree)
}

/// Every table's columns and keys and every index's columns, as lines
/// that compare: what a reader of the store depends on, without the
/// statement text an ALTER or a reworded comment would change.
fn schema(conn: &Connection) -> Vec<String> {
    let strings = |sql: &str, params: &[&str]| -> Vec<String> {
        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map(rusqlite::params_from_iter(params), |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let mut lines = Vec::new();
    let objects = strings(
        "SELECT type || ' ' || name || ' ' || tbl_name FROM sqlite_master \
         WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        &[],
    );
    for object in objects {
        let mut parts = object.split(' ');
        let (kind, name, table) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
        match kind {
            "table" => {
                for column in strings(
                    "SELECT cid || ' ' || name || ' ' || type || ' notnull=' || \"notnull\" \
                     || ' default=' || COALESCE(dflt_value, '-') || ' pk=' || pk \
                     FROM pragma_table_info(?1) ORDER BY cid",
                    &[name],
                ) {
                    lines.push(format!("table {name}: {column}"));
                }
                for key in strings(
                    "SELECT \"from\" || ' -> ' || \"table\" || '.' || COALESCE(\"to\", '?') \
                     || ' ' || on_delete FROM pragma_foreign_key_list(?1) ORDER BY id, seq",
                    &[name],
                ) {
                    lines.push(format!("table {name}: references {key}"));
                }
            }
            "index" => {
                let columns = strings("SELECT name FROM pragma_index_info(?1) ORDER BY seqno", &[name]);
                lines.push(format!("index {name} on {table}({})", columns.join(", ")));
            }
            _ => lines.push(format!("{kind} {name} on {table}")),
        }
    }
    lines
}

/// Every row of every table, for telling whether an open changed any.
fn rows(conn: &Connection) -> Vec<String> {
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
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
                let cells: Vec<String> = (0..width)
                    .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                    .collect();
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

#[test]
fn a_store_from_every_release_opens_at_this_version() {
    for release in RELEASES {
        let (_dir, s) = opened(release);
        let version: i64 = s.conn().pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, SCHEMA_VERSION, "{release}");
    }
}

#[test]
fn a_store_from_any_release_ends_with_the_schema_a_new_one_has() {
    let new = schema(&Store::in_memory().unwrap().conn());
    for release in RELEASES {
        let (_dir, s) = opened(release);
        let migrated = schema(&s.conn());
        let missing: Vec<_> = new.iter().filter(|l| !migrated.contains(l)).collect();
        let extra: Vec<_> = migrated.iter().filter(|l| !new.contains(l)).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{release} migrated to another schema\n  missing: {missing:#?}\n  extra: {extra:#?}"
        );
    }
}

#[test]
fn a_store_from_any_rung_of_the_ladder_ends_with_the_schema_a_new_one_has() {
    let new = schema(&Store::in_memory().unwrap().conn());
    assert!(new.iter().any(|l| l.starts_with("table feature: ")), "{new:#?}");
    for version in 1..SCHEMA_VERSION as usize {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime.db");
        drop(legacy_connection(&path, version));
        let s = Store::open_at(&path).unwrap();
        assert_eq!(schema(&s.conn()), new, "from version {version}");
    }
}

#[test]
fn a_migrated_store_passes_sqlites_own_checks() {
    for release in RELEASES {
        let (_dir, s) = opened(release);
        let conn = s.conn();
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(integrity, "ok", "{release}");
        let broken: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(broken, 0, "{release} has rows pointing at nothing");
    }
}

#[test]
fn reopening_a_migrated_store_changes_nothing() {
    for release in RELEASES {
        let (dir, s) = opened(release);
        let first = rows(&s.conn());
        drop(s);
        let again = Store::open_at(&dir.path().join("runtime.db")).unwrap();
        assert_eq!(rows(&again.conn()), first, "{release}");
    }
}

#[test]
fn v0_10_0_keeps_its_panes_overlays_and_review_comments() {
    let (_dir, s) = opened("v0.10.0");

    let panes = s.panes().unwrap();
    let row = |title: &str| panes.iter().find(|p| p.title == title).unwrap_or_else(|| panic!("{title}: {panes:#?}"));
    assert_eq!(panes.len(), 5);
    let agent = row("Fixture agent renamed");
    assert_eq!(agent.checkout_path, PathBuf::from("C:/fixture/repo"));
    assert_eq!(agent.kind, PaneKind::Agent);
    assert_eq!(agent.template.as_deref(), Some("claude"));
    assert_eq!(agent.status, PaneStatus::NeedsReview);
    assert_eq!(agent.note.as_deref(), Some("fixture agent note"));
    assert_eq!(agent.harness.as_deref(), Some("claude"));
    assert_eq!(agent.harness_session_id.as_deref(), Some("fixture-session-0001"));
    let worktree = row("codex");
    assert_eq!(worktree.checkout_path, PathBuf::from("C:/fixture/repo-worktrees/feature-one"));
    assert_eq!(worktree.status, PaneStatus::Waiting);
    assert_eq!(worktree.harness_session_id.as_deref(), Some("fixture-session-0002"));
    assert_eq!(row("Fixture exited shell").status, PaneStatus::Exited { code: Some(1) });
    assert_eq!(row("Fixture shell").kind, PaneKind::Shell);
    assert_eq!(row("Fixture editor").kind, PaneKind::Editor);

    let overlays = s.overlays().unwrap();
    let [(project, repos)] = overlays.projects.as_slice() else {
        panic!("{:#?}", overlays.projects);
    };
    assert_eq!((project.name.as_str(), project.workspace.as_str()), ("fixture-proj", "fixture-ws"));
    assert_eq!(project.root, PathBuf::from("C:/fixture"));
    assert_eq!(repos, &[PathBuf::from("C:/fixture/extra-repo")]);
    assert_eq!(overlays.hidden, ["fixture-hidden-proj"]);
    assert_eq!(
        overlays.repos,
        [("fixture-config-proj".to_string(), PathBuf::from("C:/fixture-config/added-repo"))]
    );
    assert_eq!(overlays.workspaces, ["fixture-ws", "fixture-empty-ws"]);
    assert_eq!(
        overlays.excluded,
        [PathBuf::from("C:/fixture/excluded-repo"), PathBuf::from("C:/fixture/excluded-backslash")]
    );
    assert_eq!(overlays.open_workspace.as_deref(), Some("fixture-ws"));

    let comments = s.review_comments(Path::new("C:/fixture/repo")).unwrap();
    let bodies: Vec<&str> = comments.iter().map(|c| c.body.as_str()).collect();
    assert_eq!(bodies, ["Fixture unstaged comment", "Fixture staged comment"]);
    let staged = &comments[1].anchor;
    assert_eq!(staged.old_path.as_deref(), Some("src/old_lib.rs"));
    assert_eq!((staged.old_start, staged.old_end, staged.new_start), (Some(3), Some(5), None));
    let worktree = s.review_comments(Path::new("C:/fixture/repo-worktrees/feature-one")).unwrap();
    let [comment] = worktree.as_slice() else { panic!("{worktree:#?}") };
    assert_eq!(comment.anchor.commit.as_deref(), Some("0123456789abcdef0123456789abcdef01234567"));
    assert_eq!(comment.anchor.text, [" context", "+added"]);
}

/// The board key v0.22.11 and v0.28.2 were given for a repository at
/// `C:/fixture/repo`: written on Windows, so the verbatim path that
/// `repository_common_dir` canonicalizes to there. It is only a string to
/// every migration after them, none of which looks at the disk.
const FIXTURE_REPOSITORY: &str = "repository\0\\\\?\\C:\\fixture\\repo\\.git";

#[test]
fn v0_22_11_keeps_its_features_numbered_in_the_order_they_were_filed() {
    let (_dir, s) = opened("v0.22.11");
    let features = s.features(FIXTURE_REPOSITORY).unwrap();
    let listed: Vec<(i64, &str, FeatureState)> =
        features.iter().map(|f| (f.number, f.slug.as_str(), f.state)).collect();
    assert_eq!(
        listed,
        [
            (1, "fixture-feature-one", FeatureState::Open),
            (2, "fixture-feature-two", FeatureState::Done),
            (3, "fixture-feature-one-2", FeatureState::Open),
        ]
    );
    let one = &features[0];
    assert_eq!(one.body, "Fixture brief for feature one.\n\nFixture appended paragraph.");
    assert_eq!(one.origin_branch.as_deref(), Some("fixture-feature-one"));
    assert_eq!(one.checkouts, ["C:/fixture/worktrees/repo/fixture-feature-one"]);
    let renamed = &features[2];
    assert_eq!(renamed.title, "Fixture feature three (renamed)");
    assert_eq!(renamed.body, "Fixture body replaced by a human.");
    assert!(features.iter().all(|f| f.held.is_none() && f.waits_on.is_empty()));

    let events = s.feature_events(FIXTURE_REPOSITORY, "fixture-feature-one-2").unwrap();
    let moves: Vec<(FeatureState, &str)> = events.iter().map(|e| (e.state, e.actor.as_str())).collect();
    assert_eq!(moves, [(FeatureState::Done, "human"), (FeatureState::Open, "agent")]);

    let workspace = s.features("workspace\0fixture-ws").unwrap();
    let [feature] = workspace.as_slice() else { panic!("{workspace:#?}") };
    assert_eq!(feature.slug, "fixture-workspace-feature");
    assert_eq!(feature.checkouts, ["C:/fixture/other-repo"]);
}

#[test]
fn v0_22_11_keeps_its_decisions_and_what_superseded_them() {
    let (_dir, s) = opened("v0.22.11");
    let decisions = s.decisions(FIXTURE_REPOSITORY).unwrap();
    let by = |chose: &str| decisions.iter().find(|d| d.chose == chose).unwrap_or_else(|| panic!("{chose}"));
    assert_eq!(decisions.len(), 5);
    let root = by("Fixture root decision");
    assert_eq!(root.over.as_deref(), Some("Fixture rejected alternative"));
    assert_eq!(root.because.as_deref(), Some("Fixture reason"));
    let child = by("Fixture child decision");
    let superseding = by("Fixture superseding decision");
    assert_eq!(child.parent, Some(root.id));
    assert_eq!(child.superseded_by, Some(superseding.id));
    assert_eq!(superseding.parent, Some(root.id));
    assert_eq!(by("Fixture unfiled decision").feature, None);
    assert!(decisions.iter().all(|d| d.state == DecisionState::Accepted && d.withdrawn_at.is_none()));
}

#[test]
fn v0_22_11_keeps_its_tasks_in_their_order_and_their_bodies_as_written() {
    let (_dir, s) = opened("v0.22.11");
    let tasks = s.tasks(FIXTURE_REPOSITORY, "fixture-feature-one").unwrap();
    let listed: Vec<(&str, TaskState, Option<i64>, i64)> =
        tasks.iter().map(|t| (t.title.as_str(), t.state, t.parent, t.position)).collect();
    let one = tasks.iter().find(|t| t.title == "Fixture task one").unwrap();
    assert_eq!(
        listed,
        [
            ("Fixture task three (retitled)", TaskState::Done, None, 0),
            ("Fixture task one", TaskState::Doing, None, 1),
            ("Fixture subtask", TaskState::Todo, Some(one.id), 0),
        ]
    );
    assert_eq!(one.claimed_by.as_deref(), Some("fixture-session-0001"));
    assert_eq!(one.external.as_deref(), Some("LIN-101"));
    // Criteria arrived as their own column later; an older task's checklist
    // stays in its body, where it was written, rather than being parsed out.
    assert_eq!(
        one.body.as_deref(),
        Some("Fixture task notes.\n\n- [ ] Fixture criterion A\n- [x] Fixture criterion B")
    );
    assert!(one.criteria.is_empty());
}

#[test]
fn v0_28_2_keeps_holds_waits_links_and_the_numbers_it_spent() {
    let (_dir, s) = opened("v0.28.2");
    let features = s.features(FIXTURE_REPOSITORY).unwrap();
    let numbers: Vec<(i64, &str)> = features.iter().map(|f| (f.number, f.slug.as_str())).collect();
    assert_eq!(
        numbers,
        [
            (1, "fixture-feature-one"),
            (2, "fixture-feature-two"),
            (3, "fixture-feature-three"),
            (4, "fixture-feature-four"),
            (5, "fixture-feature-one-2"),
            (7, "fixture-captured-feature"),
        ],
        "6 went to a feature since removed"
    );
    let by = |slug: &str| features.iter().find(|f| f.slug == slug).unwrap();
    assert_eq!(by("fixture-feature-two").held.as_deref(), Some("Fixture hold: waiting on design"));
    assert_eq!(by("fixture-feature-three").waits_on, ["fixture-feature-one"]);
    assert_eq!(by("fixture-feature-four").state, FeatureState::Done);
    let links: Vec<(&str, &str, &str)> = by("fixture-feature-one")
        .links
        .iter()
        .map(|l| (l.tracker.as_str(), l.key.as_str(), l.role.as_str()))
        .collect();
    assert_eq!(links, [("Linear", "FIX-1", "the work"), ("Jira", "FIXJ-2", "")]);

    let next = s
        .add_feature(FIXTURE_REPOSITORY, &feature("After the upgrade"), None, None, 1, None)
        .unwrap();
    assert_eq!(next.number, 8, "a number once handed out is never handed out again");
}

#[test]
fn v0_28_2_keeps_criteria_notes_claims_and_open_questions() {
    let (_dir, s) = opened("v0.28.2");
    let tasks = s.tasks(FIXTURE_REPOSITORY, "fixture-feature-one").unwrap();
    let one = tasks.iter().find(|t| t.title == "Fixture task one").unwrap();
    let criteria: Vec<(&str, bool)> = one.criteria.iter().map(|c| (c.text.as_str(), c.done)).collect();
    assert_eq!(criteria, [("Fixture criterion A", true), ("Fixture criterion B", false)]);
    let notes: Vec<&str> = one.notes.iter().map(|n| n.text.as_str()).collect();
    assert_eq!(notes, ["Fixture note on task one", "Fixture second note"]);
    let two = tasks.iter().find(|t| t.title == "Fixture task two").unwrap();
    assert_eq!((two.state, two.claimed_by.as_deref()), (TaskState::Doing, Some("fixture-session-0001")));

    let decisions = s.decisions(FIXTURE_REPOSITORY).unwrap();
    let by = |chose: &str| decisions.iter().find(|d| d.chose == chose).unwrap_or_else(|| panic!("{chose}"));
    let question = by("Fixture open question?");
    assert_eq!(question.state, DecisionState::Open);
    assert_eq!(question.options, ["Yes", "No"]);
    assert_eq!(question.task, Some(two.id));
    let resolved = by("Fixture question resolved?");
    assert_eq!((resolved.state, resolved.detail.as_deref()), (DecisionState::Accepted, Some("Option A")));
    assert_eq!(by("Fixture rejected answer").state, DecisionState::Rejected);
    assert!(by("Fixture withdrawn decision").withdrawn_at.is_some());
    let first = by("Fixture decision: use SQLite");
    assert_eq!(first.superseded_by, Some(by("Fixture decision: use SQLite in WAL mode").id));
    assert_eq!(first.consequences.len(), 2);
    assert_eq!(by("Fixture child decision, rewritten").detail.as_deref(), Some("Fixture detail"));
}

#[test]
fn v0_28_2_keeps_reconciles_tracker_maps_and_seen_sessions() {
    let (_dir, s) = opened("v0.28.2");
    let (asked, _) = s.reconcile(FIXTURE_REPOSITORY, "fixture-feature-one").unwrap();
    let asked = asked.expect("the reconcile still waiting on a human");
    assert!(asked.proposed);
    assert_eq!(asked.lines.len(), 1);
    let (_, done) = s.reconcile(FIXTURE_REPOSITORY, "fixture-feature-two").unwrap();
    assert_eq!(done.expect("the finished reconcile").seen.len(), 2);

    let map = s.tracker_map(FIXTURE_REPOSITORY).unwrap();
    assert_eq!(map.body, "Fixture map: Linear FIX-* is the work.");
    assert_eq!(map.draft.map(|d| d.body).as_deref(), Some("Fixture draft map"));
    let approved = s.tracker_map("workspace\0fixture-ws").unwrap();
    assert_eq!((approved.body.as_str(), approved.draft.is_none()), ("Fixture workspace map approved", true));

    let diagrams = s.sequence_diagrams(FIXTURE_REPOSITORY, "fixture-feature-one").unwrap();
    let titles: Vec<&str> = diagrams.iter().map(|d| d.title.as_str()).collect();
    assert_eq!(titles, ["Fixture flow", "Fixture flow two, rewritten"]);

    let seen = s.seen_sessions().unwrap();
    let [session] = seen.as_slice() else { panic!("{seen:#?}") };
    assert_eq!(
        (session.title.as_str(), session.started, session.ended),
        ("Fixture opencode session", 1_760_000_100, 1_760_000_200)
    );
    assert_eq!(s.consoles().unwrap(), 2);
    assert_eq!(s.repositories_used().unwrap().get(Path::new("C:/fixture/repo")), Some(&1_760_000_000));
}

/// The board the running daemon reads for a checkout.
fn board_of(checkout: &Path) -> String {
    format!("repository\0{}", crate::git::repository_common_dir(checkout).unwrap().to_string_lossy())
}

#[test]
fn v0_16_0_boards_from_a_repository_its_worktree_and_its_project_name_become_one() {
    let (_dir, s, repo, worktree) = opened_v0_16_0();
    let board = board_of(&repo);
    assert_eq!(board_of(&worktree), board);

    let features = s.features(&board).unwrap();
    let mut titles: Vec<&str> = features.iter().map(|f| f.title.as_str()).collect();
    titles.sort();
    assert_eq!(
        titles,
        [
            "Fixture feature one",
            "Fixture feature one",
            "Fixture feature one",
            "Fixture feature one",
            "Fixture feature two legacy renamed",
            "Fixture feature two main renamed",
            "Fixture feature two wt renamed",
        ],
        "the primary checkout's board, the worktree's, and the one filed under the project's name"
    );
    let mut slugs: Vec<&str> = features.iter().map(|f| f.slug.as_str()).collect();
    slugs.sort();
    slugs.dedup();
    assert_eq!(slugs.len(), features.len());
    assert!(slugs.iter().all(|slug| argus_protocol::is_slug(slug)), "{slugs:?}");

    let counted: usize = ["repository\0C:/fixture/repo", "workspace\0fixture-ws", board.as_str()]
        .iter()
        .map(|key| s.features(key).unwrap().len())
        .sum();
    assert_eq!(counted, 13, "every feature the release held is on a board something reads");
}

#[test]
fn v0_16_0_an_unfiled_decision_under_the_projects_name_goes_where_its_features_did() {
    let (_dir, s, repo, _worktree) = opened_v0_16_0();
    let decisions = s.decisions(&board_of(&repo)).unwrap();
    assert!(decisions.iter().any(|d| d.chose == "Fixture unfiled decision legacy"), "{decisions:#?}");
    assert!(s.decisions("fixture-proj").unwrap().is_empty());
}

#[test]
fn v0_16_0_work_follows_its_feature_through_every_rename() {
    let (_dir, s, repo, _worktree) = opened_v0_16_0();
    let board = board_of(&repo);
    let decisions = s.decisions(&board).unwrap();
    for tag in ["main", "wt", "legacy"] {
        let root = decisions
            .iter()
            .find(|d| d.chose == format!("Fixture root decision {tag}"))
            .unwrap_or_else(|| panic!("{tag}: {decisions:#?}"));
        let slug = root.feature.as_deref().unwrap();
        let tasks = s.tasks(&board, slug).unwrap();
        let titles: Vec<&str> = tasks.iter().map(|t| t.title.as_str()).collect();
        assert!(
            titles.contains(&format!("Fixture task todo {tag}").as_str()),
            "{tag}'s decisions are on {slug}, whose tasks are {titles:?}"
        );
        let child = decisions.iter().find(|d| d.chose == format!("Fixture child decision {tag}")).unwrap();
        let superseding = decisions.iter().find(|d| d.chose == format!("Fixture superseding decision {tag}")).unwrap();
        assert_eq!((child.parent, child.superseded_by), (Some(root.id), Some(superseding.id)), "{tag}");
    }
}

#[test]
fn v0_16_0_a_worktree_still_points_at_the_feature_it_was_working_on() {
    let (_dir, s, repo, worktree) = opened_v0_16_0();
    let board = board_of(&repo);
    let decisions = s.decisions(&board).unwrap();
    let working_on = |tag: &str| {
        decisions
            .iter()
            .find(|d| d.chose == format!("Fixture root decision {tag}"))
            .and_then(|d| d.feature.clone())
            .unwrap()
    };
    assert_eq!(s.artifact_feature_scope(&repo, &board).unwrap(), Some(working_on("main")));
    assert_eq!(s.artifact_feature_scope(&worktree, &board).unwrap(), Some(working_on("wt")));
}

#[test]
fn v0_33_0_keeps_its_saved_views_and_the_features_they_are_attached_to() {
    let (_dir, s) = opened("v0.33.0");
    let views = s.saved_views().unwrap();
    let [view] = views.as_slice() else { panic!("{views:#?}") };
    assert_eq!((view.id.as_str(), view.view.title.as_str()), ("fixture-view", "Fixture view"));
    assert_eq!((view.board.as_deref(), view.feature), (Some(FIXTURE_REPOSITORY), Some(1)));
    assert_eq!(view.attached, [(FIXTURE_REPOSITORY.to_string(), 1), (FIXTURE_REPOSITORY.to_string(), 2)]);
    assert_eq!(view.sources, [argus_protocol::ViewSource::File { path: "Cargo.lock".into() }]);
    assert_eq!(view.built_commit.as_deref(), Some("0123456789abcdef0123456789abcdef01234567"));
    assert!(view.stale);
    assert_eq!(view.saved_by, Some(PaneId(7)), "cleared by a starting daemon, not by the store");
    assert_eq!(s.features(FIXTURE_REPOSITORY).unwrap().len(), 6, "and the board it was saved beside");
}
