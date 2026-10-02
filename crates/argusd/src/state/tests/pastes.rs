//! A file from a client's machine reaching a pane: kept where the pane's
//! process can open it, pasted as its path there, and gone with the pane.

use std::time::Duration;

use super::*;

/// A daemon that keeps pasted files under `root`, running `cat` as its
/// one agent so the pasted path is echoed onto the screen.
fn daemon_keeping_pastes(dir: &std::path::Path) -> Arc<Daemon> {
    let d = daemon_with_running_claude(dir);
    d.set_paste_root(dir.join("pastes"));
    d
}

#[cfg(unix)]
fn screen(d: &Daemon, pane: PaneId) -> String {
    let (_, _, cells, _, _, _, _) = d.subscribe_pane(pane).unwrap();
    cells
        .iter()
        .map(|row| row.iter().map(|c| c.ch.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(unix)]
#[tokio::test]
async fn a_pasted_file_is_kept_on_the_host_and_its_path_typed_into_the_pane() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_keeping_pastes(dir.path());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    let path = d.paste_file(pane, "shot.png", b"picture").unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"picture");
    assert!(path.starts_with(dir.path().join("pastes")), "{}", path.display());
    let typed = async {
        while !screen(&d, pane).contains(&path.to_string_lossy().to_string()) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    assert!(
        tokio::time::timeout(Duration::from_secs(5), typed).await.is_ok(),
        "{}",
        screen(&d, pane)
    );
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_closed_pane_takes_its_pasted_files_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_keeping_pastes(dir.path());
    let checkout = only_checkout(&d);
    let gone = d.spawn_agent(checkout, "claude").unwrap();
    let kept = d.spawn_agent(checkout, "claude").unwrap();
    let gone_file = d.paste_file(gone, "a.png", b"a").unwrap();
    let kept_file = d.paste_file(kept, "b.png", b"b").unwrap();

    d.close_pane(gone).unwrap();

    assert!(!gone_file.exists(), "the closed pane's file is gone");
    assert!(kept_file.exists(), "the other pane's stays");
    d.close_pane(kept).unwrap();
}

#[tokio::test]
async fn a_daemon_with_nowhere_to_keep_files_refuses_rather_than_typing_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_running_claude(dir.path());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    let refused = d.paste_file(pane, "shot.png", b"picture");

    assert!(refused.is_err());
    assert!(d.paste_file(PaneId(999), "shot.png", b"picture").is_err(), "no such pane");
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_earlier_daemons_pastes_are_cleared_when_this_one_starts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("pastes");
    std::fs::create_dir_all(root.join("3")).unwrap();
    std::fs::write(root.join("3").join("1-old.png"), b"old").unwrap();

    let d = daemon_with_running_claude(dir.path());
    d.set_paste_root(root.clone());

    assert!(!root.join("3").exists(), "an old pane's file names a pane that is gone");
}
