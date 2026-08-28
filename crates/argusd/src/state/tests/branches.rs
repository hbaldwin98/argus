//! Branch rows: what the tree caches about them and what the writes
//! to Git do to it.

use super::*;
#[tokio::test]
async fn creating_a_worktree_adds_it_to_its_repository() {
    let (dir, d) = daemon_on_a_repo();
    let base = d.snapshot()[0].repositories[0].checkouts[0].id;

    d.create_worktree(base, "feature-x".to_string())
        .await
        .unwrap();

    let snapshot = d.snapshot();
    let checkouts = &snapshot[0].repositories[0].checkouts;
    assert_eq!(checkouts.len(), 2);
    assert_eq!(checkouts[1].name, "feature-x");
    assert!(!checkouts[1].primary);
    let path = std::path::Path::new(&checkouts[1].path);
    assert_eq!(head_of(path), "feature-x");
    assert!(path.starts_with(dir.path()));
}

#[tokio::test]
async fn a_fetch_brings_the_remote_only_branches_into_the_tree() {
    let (dir, d) = daemon_on_a_repo();
    let (_upstream, url) = remote_holding("from-elsewhere");
    git2::Repository::open(dir.path())
        .unwrap()
        .remote("origin", &url)
        .unwrap();

    d.fetch(only_checkout(&d)).await.unwrap();

    let remote = &d.snapshot()[0].repositories[0].remote_branches;
    assert!(
        remote.iter().any(|b| b == "origin/from-elsewhere"),
        "the fetch is what makes the row appear; got {remote:?}"
    );
}

#[tokio::test]
async fn a_worktree_for_a_remote_only_branch_starts_from_the_remote() {
    // Otherwise the row said `origin/x` and gave you a branch of that
    // name off this checkout's HEAD, which is not the work you asked
    // for. The two repositories share no history, so the commit id is
    // proof of where the branch came from.
    let (dir, d) = daemon_on_a_repo();
    let (upstream, url) = remote_holding("from-elsewhere");
    git2::Repository::open(dir.path())
        .unwrap()
        .remote("origin", &url)
        .unwrap();
    d.fetch(only_checkout(&d)).await.unwrap();

    d.create_worktree(only_checkout(&d), "from-elsewhere".to_string())
        .await
        .unwrap();

    let made = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
    assert_eq!(head_of(&made), "from-elsewhere");
    let there = git2::Repository::open(upstream.path())
        .unwrap()
        .find_branch("from-elsewhere", git2::BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap()
        .id();
    let here = git2::Repository::open(&made)
        .unwrap()
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id();
    assert_eq!(here, there, "the worktree should hold the remote's work");
}

#[tokio::test]
async fn a_pull_fast_forwards_the_checkout_onto_its_upstream() {
    let upstream = tempfile::tempdir().unwrap();
    init_repo(upstream.path());
    let url = upstream.path().to_string_lossy().replace('\\', "/");
    let clone = tempfile::tempdir().unwrap();
    let local = clone.path().join("work");
    git2::build::RepoBuilder::new().clone(&url, &local).unwrap();
    let d = daemon_with_primary(&local.to_string_lossy());

    // Work lands upstream after the clone was taken.
    let repo = git2::Repository::open(upstream.path()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let tree = head.tree().unwrap();
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    let moved = repo
        .commit(Some("HEAD"), &sig, &sig, "later", &tree, &[&head])
        .unwrap();

    d.pull(only_checkout(&d)).await.unwrap();

    let here = git2::Repository::open(&local)
        .unwrap()
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id();
    assert_eq!(here, moved);
}

#[tokio::test]
async fn deleting_a_branch_takes_it_off_the_repository() {
    let (dir, d) = daemon_on_a_repo();
    branch_off_head(dir.path(), "doomed");
    d.refresh_branches();
    assert!(
        d.snapshot()[0].repositories[0]
            .branches
            .iter()
            .any(|b| b == "doomed"),
        "the branch has to be there to be deleted"
    );

    d.delete_branch(only_checkout(&d), "doomed", false)
        .await
        .unwrap();

    assert!(
        !d.snapshot()[0].repositories[0]
            .branches
            .iter()
            .any(|b| b == "doomed"),
        "and the row goes with it, without waiting for the next poll"
    );
}

#[tokio::test]
async fn a_branch_holding_commits_nothing_else_has_is_put_back_to_the_user() {
    // `-d` first: the row you delete from says nothing about whether
    // those commits survive anywhere, so the refusal stands until the
    // user answers it themselves.
    let (dir, d) = daemon_on_a_repo();
    commit_on_a_branch(dir.path(), "spike");

    let outcome = d
        .delete_branch(only_checkout(&d), "spike", false)
        .await
        .unwrap();

    assert_eq!(outcome, BranchDeletion::NotMerged);
    assert!(
        git2::Repository::open(dir.path())
            .unwrap()
            .find_branch("spike", git2::BranchType::Local)
            .is_ok(),
        "a refused deletion leaves the branch alone"
    );
}

#[tokio::test]
async fn forcing_deletes_the_unmerged_branch() {
    let (dir, d) = daemon_on_a_repo();
    commit_on_a_branch(dir.path(), "spike");
    d.refresh_branches();

    let outcome = d
        .delete_branch(only_checkout(&d), "spike", true)
        .await
        .unwrap();

    assert_eq!(outcome, BranchDeletion::Deleted);
    assert!(
        git2::Repository::open(dir.path())
            .unwrap()
            .find_branch("spike", git2::BranchType::Local)
            .is_err(),
        "-D takes the branch even though nothing else holds its commits"
    );
    assert!(
        !d.snapshot()[0].repositories[0]
            .branches
            .iter()
            .any(|b| b == "spike"),
        "and the row goes with it"
    );
}

#[tokio::test]
async fn a_refusal_that_forcing_would_not_fix_is_still_an_error() {
    // The branch isn't there at all: `-D` has nothing more to offer
    // than `-d` did, so git's message is the answer.
    let (_dir, d) = daemon_on_a_repo();

    let err = d
        .delete_branch(only_checkout(&d), "never-existed", false)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("never-existed"), "got {err:?}");
}

#[tokio::test]
async fn the_main_branch_is_not_deletable_from_its_own_row() {
    let (dir, d) = daemon_on_a_repo();
    branch_off_head(dir.path(), "main");

    let err = d
        .delete_branch(only_checkout(&d), "main", false)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("main branch"), "got {err:?}");
}

/// The daemon, plus the id of a linked worktree it just made.
async fn daemon_with_a_worktree(name: &str) -> (tempfile::TempDir, Arc<Daemon>, CheckoutId) {
    let (dir, d) = daemon_on_a_repo();
    d.create_worktree(only_checkout(&d), name.to_string())
        .await
        .unwrap();
    let id = d.snapshot()[0].repositories[0].checkouts[1].id;
    (dir, d, id)
}

#[tokio::test]
async fn removing_a_worktree_takes_its_directory_and_its_row_with_it() {
    let (_dir, d, worktree) = daemon_with_a_worktree("doomed").await;
    let path = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);

    d.remove_checkout(worktree).await.unwrap();

    assert_eq!(d.snapshot()[0].repositories[0].checkouts.len(), 1);
    assert!(!path.exists(), "the working directory should be gone");
}

#[tokio::test]
async fn a_removal_git_would_refuse_keeps_the_panes_it_would_have_killed() {
    // The point of checking before killing: a locked worktree is a
    // refusal git only reports once it runs, and by then the agents that
    // were working in it are already dead.
    let (dir, d, worktree) = daemon_with_a_worktree("locked-up").await;
    let pane = d.spawn_shell(worktree).unwrap();
    git2::Repository::open(dir.path())
        .unwrap()
        .find_worktree("locked-up")
        .unwrap()
        .lock(Some("held by hand"))
        .unwrap();

    let err = d.remove_checkout(worktree).await.unwrap_err().to_string();

    assert!(err.contains("locked"), "got {err:?}");
    let snapshot = d.snapshot();
    let checkouts = &snapshot[0].repositories[0].checkouts;
    assert_eq!(checkouts.len(), 2, "the checkout stays");
    assert_eq!(checkouts[1].panes.len(), 1, "and so does what was running");
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn removing_a_checkout_whose_directory_is_already_gone_clears_it() {
    // `git worktree remove` refuses a path it cannot find, which would
    // strand the row for a directory the user deleted by hand.
    let (dir, d, worktree) = daemon_with_a_worktree("deleted").await;
    let path = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
    std::fs::remove_dir_all(&path).unwrap();

    d.remove_checkout(worktree).await.unwrap();

    assert_eq!(d.snapshot()[0].repositories[0].checkouts.len(), 1);
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(
        repo.worktrees().unwrap().len(),
        0,
        "the registration should have been pruned too"
    );
}

#[tokio::test]
async fn the_primary_checkout_is_never_removable() {
    let (dir, d) = daemon_on_a_repo();

    assert!(d.remove_checkout(only_checkout(&d)).await.is_err());

    assert!(dir.path().join("a.txt").exists());
    assert_eq!(d.snapshot()[0].repositories[0].checkouts.len(), 1);
}

#[tokio::test]
async fn creating_a_branch_moves_this_checkout_onto_it() {
    // Unlike `create_worktree`, which puts the branch in a directory of
    // its own and leaves this checkout where it was.
    let (dir, d) = daemon_on_a_repo();
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

    d.create_branch(checkout, "feature/x").await.unwrap();

    assert_eq!(head_of(dir.path()), "feature/x");
    assert_eq!(
        d.snapshot()[0].repositories[0].checkouts.len(),
        1,
        "no new checkout — that is what a worktree is for"
    );
}

#[tokio::test]
async fn the_checkouts_name_follows_the_branch_it_moves_to() {
    let (_dir, d) = daemon_on_a_repo();
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

    d.create_branch(checkout, "feature/x").await.unwrap();

    assert_eq!(
        d.snapshot()[0].repositories[0].checkouts[0].name,
        "feature/x"
    );
}

#[test]
fn the_checkouts_name_follows_a_branch_switch_made_outside_argus() {
    let (dir, d) = daemon_on_a_repo();
    let repo = git2::Repository::open(dir.path()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("outside", &head, false).unwrap();
    repo.set_head("refs/heads/outside").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();

    // Nothing told the daemon, so the poll is what finds it — the same
    // step `start_git_poll` runs every two seconds. `snapshot` reads the
    // cache that poll fills and never git itself, because it is taken
    // under the lock keystrokes need.
    d.refresh_git_status();

    assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].name, "outside");
}

#[test]
fn a_tree_snapshot_reads_no_git_of_its_own() {
    // The guarantee the status cache exists for: `snapshot` runs under
    // the daemon's one lock, and `write_pane` needs that same lock to
    // find the pty a keystroke belongs to. Reading git there put several
    // milliseconds of blocking I/O per checkout in front of the next
    // key. Asserted by moving the repo out from under the daemon: a
    // snapshot that still consulted git would lose the branch name.
    let (dir, d) = daemon_on_a_repo();
    d.refresh_git_status();
    let named = d.snapshot()[0].repositories[0].checkouts[0].name.clone();

    std::fs::remove_dir_all(dir.path().join(".git")).unwrap();

    assert_eq!(
        d.snapshot()[0].repositories[0].checkouts[0].name,
        named,
        "the snapshot went back to git instead of using the cache"
    );
}

#[test]
fn startup_names_checkouts_from_head_without_walking_the_workdir() {
    // Daemon construction used to run a full `git::status` on every
    // checkout before the process listened. Untracked files made that
    // a workdir walk of every repository under a project root. HEAD is
    // enough to name the row; dirty counts arrive on the first poll.
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("untracked.txt"), "x").unwrap();
    let d = daemon_with_primary(&dir.path().to_string_lossy());

    let checkout = &d.snapshot()[0].repositories[0].checkouts[0];
    assert_eq!(checkout.name, head_of(dir.path()));
    assert_eq!(
        checkout.git.as_ref().map(|g| g.dirty),
        Some(false),
        "startup must not walk the workdir for untracked files"
    );

    d.refresh_git_status();
    assert!(
        d.snapshot()[0].repositories[0].checkouts[0]
            .git
            .as_ref()
            .is_some_and(|g| g.dirty),
        "the poll still sees the untracked file"
    );
}

#[tokio::test]
async fn switching_moves_between_branches_that_already_exist() {
    let (dir, d) = daemon_on_a_repo();
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
    let start = head_of(dir.path());
    d.create_branch(checkout, "other").await.unwrap();

    d.switch_branch(checkout, &start).await.unwrap();

    assert_eq!(head_of(dir.path()), start);
    assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].name, start);
}

#[tokio::test]
async fn switching_pushes_a_new_tree_so_every_client_sees_the_move() {
    let (_dir, d) = daemon_on_a_repo();
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
    let mut rx = d.subscribe_tree();

    d.create_branch(checkout, "feature/x").await.unwrap();

    let tree = rx.try_recv().expect("clients need to be told");
    assert_eq!(tree[0].repositories[0].checkouts[0].name, "feature/x");
}

#[tokio::test]
async fn switching_to_a_branch_that_does_not_exist_reports_gits_own_words() {
    let (_dir, d) = daemon_on_a_repo();
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

    let err = d
        .switch_branch(checkout, "no-such-branch")
        .await
        .unwrap_err()
        .to_string();

    assert!(
        !err.is_empty(),
        "git's refusal is what the user needs to read"
    );
}

#[tokio::test]
async fn creating_a_branch_that_already_exists_is_refused() {
    let (_dir, d) = daemon_on_a_repo();
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
    d.create_branch(checkout, "taken").await.unwrap();

    assert!(d.create_branch(checkout, "taken").await.is_err());
}

#[tokio::test]
async fn an_empty_or_flag_like_branch_name_never_reaches_git() {
    // A leading dash would be parsed as an option rather than a name.
    let (_dir, d) = daemon_on_a_repo();
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

    for bad in ["", "   ", "--force", "-b"] {
        assert!(
            d.create_branch(checkout, bad).await.is_err(),
            "{bad:?} should be refused"
        );
    }
}

#[tokio::test]
async fn a_worktree_branch_name_is_checked_as_strictly_as_a_branch_switch() {
    // The name is both a git argument and the directory Argus builds
    // from it, so a rooted or climbing one would put the worktree
    // wherever it said rather than under the worktrees root.
    let (dir, d) = daemon_on_a_repo();
    let base = only_checkout(&d);
    let escaped = dir.path().parent().unwrap().join("escaped");

    for bad in [
        "",
        "   ",
        "-b",
        "--force",
        "..",
        "../escaped",
        r"..\escaped",
        "/escaped",
        r"C:\escaped",
    ] {
        assert!(
            d.create_worktree(base, bad.to_string()).await.is_err(),
            "{bad:?} should be refused"
        );
    }

    assert!(!escaped.exists(), "a worktree landed outside the root");
    assert_eq!(
        d.snapshot()[0].repositories[0].checkouts.len(),
        1,
        "nothing should have been added"
    );
}

#[tokio::test]
async fn a_branch_name_with_a_slash_still_nests_under_the_worktrees_root() {
    let (dir, d) = daemon_on_a_repo();

    d.create_worktree(only_checkout(&d), "feat/nested".to_string())
        .await
        .unwrap();

    let path = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
    assert!(path.starts_with(dir.path().join(".argus").join("worktrees")));
    assert_eq!(head_of(&path), "feat/nested");
}

#[tokio::test]
async fn a_branch_switch_made_outside_argus_reaches_clients_without_waiting_for_the_poll() {
    // The poll would find this too, two seconds later. The watch is
    // what makes an agent's commit or switch show up as it happens.
    let (dir, d) = daemon_on_a_repo();
    d.refresh_git_status();
    let mut tree = d.subscribe_tree();
    d.start_git_watch();
    // The first sync of the watched set happens on the interval's
    // immediate first tick; give it the scheduler slot it needs.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let repo = git2::Repository::open(dir.path()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("from-a-shell", &head, false).unwrap();
    repo.set_head("refs/heads/from-a-shell").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let named = loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(!left.is_zero(), "the watch never reported the switch");
        let Ok(Ok(projects)) = tokio::time::timeout(left, tree.recv()).await else {
            panic!("the watch never reported the switch");
        };
        let name = projects[0].repositories[0].checkouts[0].name.clone();
        if name == "from-a-shell" {
            break name;
        }
    };

    assert_eq!(named, "from-a-shell");
}

#[tokio::test]
async fn a_configured_worktree_root_is_where_worktrees_go() {
    let elsewhere = tempfile::tempdir().unwrap();
    let (dir, d) = daemon_on_a_repo_with(Some(&elsewhere.path().to_string_lossy()), &[]);

    d.create_worktree(only_checkout(&d), "over-there".to_string())
        .await
        .unwrap();

    let made = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
    let repo_name = dir.path().file_name().unwrap();
    assert_eq!(
        made,
        elsewhere.path().join(repo_name).join("over-there"),
        "one directory per repository under the root, so two repos can share a branch name"
    );
    assert!(!dir.path().join(".argus").exists(), "not the default root");
}

#[tokio::test]
async fn setup_commands_run_in_the_worktree_that_was_just_made() {
    // `git tag` is a command every machine running these tests has, and
    // it leaves something a test can read back.
    let (_dir, d) = daemon_on_a_repo_with(None, &["git tag setup-ran"]);

    d.create_worktree(only_checkout(&d), "with-setup".to_string())
        .await
        .unwrap();

    let made = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
    let repo = git2::Repository::open(&made).unwrap();
    let tags = repo.tag_names(None).unwrap();
    assert!(
        tags.iter().flatten().any(|t| t == "setup-ran"),
        "the setup command should have run in {}",
        made.display()
    );
}

#[tokio::test]
async fn a_setup_command_that_fails_is_reported_without_taking_the_worktree_with_it() {
    let (_dir, d) = daemon_on_a_repo_with(None, &["git not-a-git-command"]);

    let err = d
        .create_worktree(only_checkout(&d), "half-set-up".to_string())
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("not-a-git-command"), "got {err:?}");
    let snapshot = d.snapshot();
    let checkouts = &snapshot[0].repositories[0].checkouts;
    assert_eq!(checkouts.len(), 2, "the worktree is still there to fix");
    assert!(PathBuf::from(&checkouts[1].path).is_dir());
}

#[tokio::test]
async fn a_branch_with_no_checkout_is_listed_on_its_repository() {
    let (dir, d) = daemon_on_a_repo();
    let checkout = only_checkout(&d);
    let on_it = head_of(dir.path());
    d.create_branch(checkout, "parked").await.unwrap();
    d.switch_branch(checkout, &on_it).await.unwrap();

    d.refresh_git_status();
    d.refresh_branches();

    assert_eq!(
        d.snapshot()[0].repositories[0].branches,
        vec!["parked".to_string()],
        "the branch nothing is sitting on is the one to offer"
    );
}

#[tokio::test]
async fn a_branch_a_checkout_is_sitting_on_is_not_offered_as_one_to_go_to() {
    let (_dir, d) = daemon_on_a_repo();
    d.create_worktree(only_checkout(&d), "in-a-worktree".to_string())
        .await
        .unwrap();

    d.refresh_git_status();
    d.refresh_branches();

    assert!(
        !d.snapshot()[0].repositories[0]
            .branches
            .contains(&"in-a-worktree".to_string()),
        "it already has a directory of its own"
    );
}

#[tokio::test]
async fn a_branch_that_already_exists_gets_a_worktree_rather_than_a_refusal() {
    // The tree offers a worktree for a branch row, and every branch row
    // is a branch that already exists.
    let (_dir, d) = daemon_on_a_repo();
    let checkout = only_checkout(&d);
    let on_it = head_of(&PathBuf::from(
        &d.snapshot()[0].repositories[0].checkouts[0].path,
    ));
    d.create_branch(checkout, "waiting").await.unwrap();
    d.switch_branch(checkout, &on_it).await.unwrap();

    d.create_worktree(checkout, "waiting".to_string())
        .await
        .unwrap();

    let snapshot = d.snapshot();
    let made = &snapshot[0].repositories[0].checkouts[1];
    assert_eq!(head_of(&PathBuf::from(&made.path)), "waiting");
}

#[tokio::test]
async fn a_dirty_primary_checkout_is_not_switched_out_from_under_its_work() {
    let (dir, d) = daemon_on_a_repo();
    let checkout = only_checkout(&d);
    let on_it = head_of(dir.path());
    d.create_branch(checkout, "elsewhere").await.unwrap();
    d.switch_branch(checkout, &on_it).await.unwrap();
    std::fs::write(dir.path().join("a.txt"), "uncommitted\n").unwrap();

    let err = d
        .switch_branch(checkout, "elsewhere")
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("worktree"), "say what to do instead: {err:?}");
    assert_eq!(head_of(dir.path()), on_it, "still where the work is");
}

#[tokio::test]
async fn a_dirty_worktree_still_switches_because_argus_made_it() {
    // The refusal is about the repo the user already had. A linked
    // worktree is Argus's own, and an agent moving between branches in
    // one is ordinary work.
    let (_dir, d, worktree) = daemon_with_a_worktree("scratch").await;
    let path = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
    d.create_branch(worktree, "second").await.unwrap();
    std::fs::write(path.join("a.txt"), "uncommitted\n").unwrap();

    d.switch_branch(worktree, "scratch").await.unwrap();

    assert_eq!(head_of(&path), "scratch");
}

#[tokio::test]
async fn a_clean_primary_checkout_still_switches() {
    let (dir, d) = daemon_on_a_repo();
    let checkout = only_checkout(&d);
    let on_it = head_of(dir.path());
    d.create_branch(checkout, "clean-move").await.unwrap();

    d.switch_branch(checkout, &on_it).await.unwrap();

    assert_eq!(head_of(dir.path()), on_it);
}

#[tokio::test]
async fn a_branch_operation_on_a_checkout_that_is_gone_errors() {
    let (_dir, d) = daemon_on_a_repo();
    assert!(d.create_branch(CheckoutId(9999), "x").await.is_err());
    assert!(d.switch_branch(CheckoutId(9999), "x").await.is_err());
}

#[tokio::test]
async fn a_gui_editor_never_gets_a_pane_even_when_a_pane_was_asked_for() {
    // A GUI editor in a pty is a blank grid and a child that never speaks.
    // Use a missing executable with a known GUI-editor name so this test
    // exercises that branch without opening a real window.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_primary(&dir.path().to_string_lossy());
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
    std::fs::write(dir.path().join("a.txt"), "x").unwrap();

    let made = d.spawn_editor(checkout, "a.txt", None, false, Some("missing/notepad.exe"));

    assert!(
        made.is_err(),
        "the deliberately missing editor must not launch"
    );
    assert!(
        d.snapshot()[0].repositories[0].checkouts[0]
            .panes
            .is_empty(),
        "a GUI editor must not become a pane"
    );
}

#[tokio::test]
async fn a_terminal_editor_pane_has_no_harness_session() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_primary(&dir.path().to_string_lossy());
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
    std::fs::write(dir.path().join("a.txt"), "x").unwrap();
    let editor = std::env::current_exe().unwrap();

    let pane = d
        .spawn_editor(
            checkout,
            "a.txt",
            None,
            false,
            Some(&editor.to_string_lossy()),
        )
        .unwrap();

    let stored = d
        .inner
        .lock()
        .unwrap()
        .projects
        .iter()
        .flat_map(|project| &project.repositories)
        .flat_map(|repository| &repository.checkouts)
        .flat_map(|checkout| &checkout.panes)
        .find(|candidate| candidate.id == pane)
        .map(|pane| (pane.kind, pane.harness_session_id.clone()));
    assert_eq!(stored, Some((PaneKind::Editor, None)));
    d.close_pane(pane).unwrap();
}
