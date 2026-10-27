//! Extensions' console views: the panes the daemon runs for them, kept and
//! restarted as the views are published, and ended as the views go.

use std::time::Duration;

use argus_protocol::{ConsoleEnd, ExtEvent, ExtView};

use super::*;

/// A view of one console in `scope`, as JSON an extension would put.
fn console_view(name: &str, scope: &str, console: &str) -> ExtView {
    let json = format!(r#"{{"name": "{name}", "title": "{name} console", "scope": {scope}, "console": {console}}}"#);
    serde_json::from_str(&json).unwrap()
}

/// A scripted daemon over one repository in a temp directory, with the
/// extension `dev` admitted and listening.
fn dev_on_a_checkout() -> (tempfile::TempDir, ScriptedDaemon, String, tokio::sync::mpsc::UnboundedReceiver<ExtEvent>) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("web")).unwrap();
    let d = daemon_with_fake_claude(dir.path());
    d.admit_extension("dev");
    let (_, events) = d.open_extension_events("dev").unwrap();
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id.0;
    (dir, d, format!(r#"{{"checkout": {checkout}}}"#), events)
}

fn pane_of(d: &Daemon, view: &str) -> Option<PaneId> {
    d.extension_views().iter().find(|v| v.name == view)?.console.as_ref()?.pane
}

#[tokio::test]
async fn publishing_a_console_view_starts_its_command_in_a_pane() {
    let (dir, d, scope, _events) = dev_on_a_checkout();
    let view = console_view("dev", &scope, r#"{"command": ["npm", "run", "dev"], "cwd": "web", "env": {"PORT": "3000"}}"#);

    d.publish_views("dev", vec![view]).unwrap();

    let pane = pane_of(&d, "dev").expect("the view names its pane");
    let started = d.processes.started(pane).unwrap();
    assert_eq!(started.program.as_deref(), Some("npm"));
    assert_eq!(started.args, ["run", "dev"]);
    assert_eq!(started.env, [("PORT".to_string(), "3000".to_string())]);
    assert_eq!(started.cwd, dir.path().join("web"));
    assert!(d.subscribe_pane(pane).is_ok(), "a client can watch it like any pane");
    assert!(d.pane_input(pane).is_some());
}

#[tokio::test]
async fn what_the_extension_puts_in_the_daemons_fields_is_not_believed() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    let view = console_view("dev", &scope, r#"{"command": ["sh"], "pane": 999, "ended": {"code": 3}}"#);

    d.publish_views("dev", vec![view]).unwrap();

    let console = d.extension_views().remove(0).console.unwrap();
    assert_ne!(console.pane, Some(PaneId(999)));
    assert_eq!(console.ended, None);
}

#[tokio::test]
async fn publishing_a_running_console_again_unchanged_keeps_its_pane() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    let view = console_view("dev", &scope, r#"{"command": ["sh"]}"#);
    d.publish_views("dev", vec![view.clone()]).unwrap();
    let first = pane_of(&d, "dev").unwrap();

    d.publish_views("dev", vec![view]).unwrap();

    assert_eq!(pane_of(&d, "dev"), Some(first));
    assert!(!d.processes.killed(first));
    assert_eq!(d.console_panes("dev"), [first]);
}

#[tokio::test]
async fn a_changed_command_starts_the_console_anew_and_ends_the_old_one() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    d.publish_views("dev", vec![console_view("dev", &scope, r#"{"command": ["sh"]}"#)]).unwrap();
    let first = pane_of(&d, "dev").unwrap();

    d.publish_views("dev", vec![console_view("dev", &scope, r#"{"command": ["bash"]}"#)]).unwrap();

    let second = pane_of(&d, "dev").unwrap();
    assert_ne!(first, second);
    assert!(d.processes.killed(first));
    assert_eq!(d.console_panes("dev"), [second]);
    assert!(d.subscribe_pane(first).is_err(), "the old pane is gone");
}

#[tokio::test]
async fn a_view_the_extension_stops_publishing_ends_its_console() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    d.publish_views("dev", vec![console_view("dev", &scope, r#"{"command": ["sh"]}"#)]).unwrap();
    let pane = pane_of(&d, "dev").unwrap();

    d.publish_views("dev", Vec::new()).unwrap();

    assert!(d.processes.killed(pane));
    assert!(d.console_panes("dev").is_empty());
    assert!(d.pane_input(pane).is_none());
}

#[tokio::test]
async fn an_extension_that_stops_takes_every_console_it_ran() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    let views = vec![
        console_view("a", &scope, r#"{"command": ["sh"]}"#),
        console_view("b", &scope, r#"{"command": ["sh"]}"#),
    ];
    d.publish_views("dev", views).unwrap();
    let panes = d.console_panes("dev");
    assert_eq!(panes.len(), 2);

    d.retire_extension("dev");

    assert!(panes.iter().all(|pane| d.processes.killed(*pane)));
    assert!(d.console_panes("dev").is_empty());
    assert!(d.subscribe_pane(panes[0]).is_err());
}

/// The extension `dev` exits, and the one started in its place is admitted
/// and listening, as `running` does it.
fn restart(d: &ScriptedDaemon, wait: Duration) -> tokio::sync::mpsc::UnboundedReceiver<ExtEvent> {
    d.extension_exited("dev", wait);
    d.admit_extension("dev");
    d.open_extension_events("dev").unwrap().1
}

#[tokio::test]
async fn an_extension_restarted_takes_over_the_consoles_it_publishes_the_same() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    let stack = console_view("stack", &scope, r#"{"command": ["sh"], "cwd": "web"}"#);
    d.publish_views("dev", vec![stack.clone()]).unwrap();
    let pane = pane_of(&d, "stack").unwrap();

    let _events = restart(&d, Duration::from_secs(60));
    assert!(!d.processes.killed(pane), "an extension exiting is no request to stop the person's stack");
    assert!(d.extension_views().is_empty(), "while it is down it shows nothing");
    d.publish_views("dev", vec![stack]).unwrap();

    assert_eq!(pane_of(&d, "stack"), Some(pane), "the same process, not a new one");
    assert!(!d.processes.killed(pane));
    assert_eq!(d.console_panes("dev"), [pane]);
}

#[tokio::test]
async fn a_console_published_differently_after_a_restart_is_started_anew_and_the_old_one_waits_out_its_handover() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    d.publish_views("dev", vec![console_view("stack", &scope, r#"{"command": ["sh"]}"#)]).unwrap();
    let old = pane_of(&d, "stack").unwrap();

    let _events = restart(&d, Duration::from_millis(50));
    d.publish_views("dev", vec![console_view("stack", &scope, r#"{"command": ["bash"]}"#)]).unwrap();

    let new = pane_of(&d, "stack").unwrap();
    assert_ne!(new, old, "another run is another process");
    assert!(!d.processes.killed(old), "unclaimed, it is still in its handover");
    for _ in 0..100 {
        if d.processes.killed(old) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(d.processes.killed(old), "ended once the handover is over");
    assert!(!d.processes.killed(new));
    assert_eq!(d.console_panes("dev"), [new]);
}

#[tokio::test]
async fn a_console_claimed_is_not_ended_by_the_handover_it_left() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    let stack = console_view("stack", &scope, r#"{"command": ["sh"]}"#);
    d.publish_views("dev", vec![stack.clone()]).unwrap();
    let pane = pane_of(&d, "stack").unwrap();

    let _events = restart(&d, Duration::from_millis(20));
    d.publish_views("dev", vec![stack]).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(!d.processes.killed(pane));
    assert_eq!(pane_of(&d, "stack"), Some(pane));
}

#[tokio::test]
async fn an_exited_console_records_how_it_ended_keeps_its_pane_and_tells_the_extension() {
    let (_dir, d, scope, mut events) = dev_on_a_checkout();
    d.publish_views("dev", vec![console_view("dev", &scope, r#"{"command": ["sh"]}"#)]).unwrap();
    let pane = pane_of(&d, "dev").unwrap();
    let mut news = d.subscribe_extensions();

    d.exit(pane, Some(2));

    let console = d.extension_views().remove(0).console.unwrap();
    assert_eq!((console.pane, console.ended), (Some(pane), Some(ConsoleEnd { code: Some(2) })));
    assert!(matches!(news.try_recv(), Ok(ExtensionNews::Views)));
    assert_eq!(events.try_recv().unwrap(), ExtEvent::ConsoleEnded { view: "dev".into(), code: Some(2) });
    assert!(d.subscribe_pane(pane).is_ok(), "its last screen can still be read");
    assert!(d.pane_input(pane).is_none(), "but nothing can be typed into it");
}

#[tokio::test]
async fn publishing_an_ended_console_again_starts_it_again() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    let view = console_view("dev", &scope, r#"{"command": ["sh"]}"#);
    d.publish_views("dev", vec![view.clone()]).unwrap();
    let first = pane_of(&d, "dev").unwrap();
    d.exit(first, None);

    d.publish_views("dev", vec![view]).unwrap();

    let console = d.extension_views().remove(0).console.unwrap();
    assert_ne!(console.pane, Some(first));
    assert_eq!(console.ended, None);
    assert_eq!(d.console_panes("dev").len(), 1, "the ended one is let go");
}

#[tokio::test]
async fn a_client_cannot_close_a_console_an_extension_runs() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    d.publish_views("dev", vec![console_view("dev", &scope, r#"{"command": ["sh"]}"#)]).unwrap();
    let pane = pane_of(&d, "dev").unwrap();

    let refused = d.close_pane(pane).unwrap_err().to_string();

    assert!(refused.contains("dev"), "{refused}");
    assert!(!d.processes.killed(pane));
    assert_eq!(d.console_panes("dev"), [pane]);
}

#[tokio::test]
async fn an_extensions_console_is_no_console_tab_and_is_not_remembered() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    d.publish_views("dev", vec![console_view("dev", &scope, r#"{"command": ["sh"]}"#)]).unwrap();
    d.broadcast_tree();

    assert!(d.consoles().is_empty());
    assert_eq!(d.store.consoles().unwrap(), 0);
    assert!(d.store.panes().unwrap().is_empty());
    assert!(d.snapshot()[0].repositories[0].checkouts[0].panes.is_empty());
}

#[tokio::test]
async fn a_directory_that_is_not_there_refuses_the_publish_and_changes_nothing() {
    let (_dir, d, scope, _events) = dev_on_a_checkout();
    let kept = console_view("dev", &scope, r#"{"command": ["sh"]}"#);
    d.publish_views("dev", vec![kept]).unwrap();
    let pane = pane_of(&d, "dev").unwrap();

    let views = vec![
        console_view("fine", &scope, r#"{"command": ["sh"], "cwd": "web"}"#),
        console_view("lost", &scope, r#"{"command": ["sh"], "cwd": "nowhere"}"#),
    ];
    let refused = d.publish_views("dev", views).unwrap_err().to_string();

    assert!(refused.starts_with("views[1].console.cwd:"), "{refused}");
    assert_eq!(d.console_panes("dev"), [pane], "the one started for the first view was taken back");
    assert!(!d.processes.killed(pane));
    assert_eq!(pane_of(&d, "dev"), Some(pane));
    assert_eq!(d.extension_views().len(), 1);
}

#[tokio::test]
async fn a_checkout_the_daemon_does_not_know_refuses_the_publish() {
    let (_dir, d, _scope, _events) = dev_on_a_checkout();

    let refused = d
        .publish_views("dev", vec![console_view("dev", r#"{"checkout": 987654}"#, r#"{"command": ["sh"]}"#)])
        .unwrap_err()
        .to_string();

    assert!(refused.starts_with("views[0].console.cwd:"), "{refused}");
    assert!(d.extension_views().is_empty());
}
