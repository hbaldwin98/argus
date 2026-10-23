//! What staging and committing from a client does to a real repository,
//! through the person's own `git`.

use argus_protocol::{IndexChange, LineKind, PickedLine, ReviewBase};

use super::*;

/// The daemon's repository with an identity of its own, so a commit does
/// not hang on whoever runs the tests, and without their signing.
fn committable() -> (tempfile::TempDir, Arc<Daemon>) {
    let (dir, d) = daemon_on_a_repo();
    let mut config = git2::Repository::open(dir.path()).unwrap().config().unwrap();
    config.set_str("user.name", "t").unwrap();
    config.set_str("user.email", "t@example.com").unwrap();
    config.set_bool("commit.gpgsign", false).unwrap();
    (dir, d)
}

/// What the index holds for `path`, or nothing.
fn indexed(root: &std::path::Path, path: &str) -> Option<String> {
    let repo = git2::Repository::open(root).unwrap();
    let entry = repo.index().unwrap().get_path(std::path::Path::new(path), 0)?;
    let text = String::from_utf8(repo.find_blob(entry.id).unwrap().content().to_vec()).unwrap();
    Some(text)
}

fn stage(paths: &[&str]) -> IndexChange {
    IndexChange::Stage { paths: paths.iter().map(|p| p.to_string()).collect() }
}

fn unstage(paths: &[&str]) -> IndexChange {
    IndexChange::Unstage { paths: paths.iter().map(|p| p.to_string()).collect() }
}

#[tokio::test]
async fn a_staged_file_and_a_new_one_reach_the_index_and_go_back_out() {
    let (dir, d) = committable();
    std::fs::write(dir.path().join("a.txt"), "two\n").unwrap();
    std::fs::write(dir.path().join("new file.txt"), "hi\n").unwrap();

    d.stage(only_checkout(&d), stage(&["a.txt", "new file.txt"])).await.unwrap();
    assert_eq!(indexed(dir.path(), "a.txt").as_deref(), Some("two\n"));
    assert_eq!(indexed(dir.path(), "new file.txt").as_deref(), Some("hi\n"));

    d.stage(only_checkout(&d), unstage(&["a.txt", "new file.txt"])).await.unwrap();
    assert_eq!(indexed(dir.path(), "a.txt").as_deref(), Some("one\n"));
    assert_eq!(indexed(dir.path(), "new file.txt"), None);
    assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "two\n", "the work stays on disk");
}

#[tokio::test]
async fn a_deleted_file_staged_leaves_the_index() {
    let (dir, d) = committable();
    std::fs::remove_file(dir.path().join("a.txt")).unwrap();
    d.stage(only_checkout(&d), stage(&["a.txt"])).await.unwrap();
    assert_eq!(indexed(dir.path(), "a.txt"), None);
}

#[tokio::test]
async fn a_name_git_would_read_as_a_pattern_stages_only_that_file() {
    let (dir, d) = committable();
    std::fs::write(dir.path().join("a.txt"), "two\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
    let error = d.stage(only_checkout(&d), stage(&["*.txt"])).await.unwrap_err();
    assert!(!error.to_string().is_empty());
    assert_eq!(indexed(dir.path(), "b.txt"), None, "no file is named *.txt");
}

#[tokio::test]
async fn a_path_out_of_the_checkout_is_refused_before_git_runs() {
    let (_dir, d) = committable();
    for path in ["../outside", "/etc/passwd", ""] {
        let error = d.stage(only_checkout(&d), stage(&[path])).await.unwrap_err();
        assert!(error.to_string().contains("not a file in this checkout"), "{path}: {error}");
    }
    let error = d.stage(only_checkout(&d), stage(&[])).await.unwrap_err();
    assert!(error.to_string().contains("no files"), "{error}");
}

#[tokio::test]
async fn picked_lines_are_written_to_the_index_and_given_back() {
    let (dir, d) = committable();
    std::fs::write(dir.path().join("a.txt"), "one\nx\ny\n").unwrap();
    let line = |lineno, text: &str| PickedLine { kind: LineKind::Added, lineno, text: text.into() };

    let take = IndexChange::Lines { from: ReviewBase::Unstaged, path: "a.txt".into(), lines: vec![line(3, "y")] };
    d.stage(only_checkout(&d), take).await.unwrap();
    assert_eq!(indexed(dir.path(), "a.txt").as_deref(), Some("one\ny\n"));

    let back = IndexChange::Lines { from: ReviewBase::Staged, path: "a.txt".into(), lines: vec![line(2, "y")] };
    d.stage(only_checkout(&d), back).await.unwrap();
    assert_eq!(indexed(dir.path(), "a.txt").as_deref(), Some("one\n"));
}

#[tokio::test]
async fn a_commit_is_made_of_the_index_alone_with_the_message_given() {
    let (dir, d) = committable();
    std::fs::write(dir.path().join("a.txt"), "two\n").unwrap();
    std::fs::write(dir.path().join("left.txt"), "not staged\n").unwrap();
    d.stage(only_checkout(&d), stage(&["a.txt"])).await.unwrap();

    d.commit(only_checkout(&d), "Say two\n\nWith a body; and \"quotes\".").await.unwrap();

    let repo = git2::Repository::open(dir.path()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().ok(), Some("Say two\n\nWith a body; and \"quotes\".\n"));
    assert_eq!(head.parent_count(), 1);
    assert!(head.tree().unwrap().get_path(std::path::Path::new("left.txt")).is_err());
    let last = d.snapshot()[0].repositories[0].checkouts[0].git.as_ref().unwrap().last_commit.clone();
    assert_eq!(last.map(|c| c.summary), Some("Say two".to_string()), "the tree is told");
}

#[tokio::test]
async fn a_commit_without_a_message_or_anything_staged_is_refused() {
    let (dir, d) = committable();
    let error = d.commit(only_checkout(&d), "  \n").await.unwrap_err();
    assert!(error.to_string().contains("needs a message"), "{error}");
    let error = d.commit(only_checkout(&d), "nothing").await.unwrap_err();
    assert!(!error.to_string().is_empty());
    let repo = git2::Repository::open(dir.path()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().ok(), Some("first"));
}
