//! Extensions: what one publishes through the pane API, the keys it is
//! refused, and what the person does reaching it back on its event stream.

use std::time::Duration;

use argus_protocol::{
    extension_path, CommandScope, ExtAnswer, ExtCommand, ExtDialog, ExtDialogKind, ExtEndpoint, ExtEvent, ExtKey,
    ExtView,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use super::*;

fn served() -> Arc<Daemon> {
    let d = daemon_with_primary("/nonexistent/argus-extensions");
    d.start_hook_server().unwrap();
    d
}

async fn connect(d: &Daemon) -> tokio::net::TcpStream {
    let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
    tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap()
}

/// One request to `name`'s `endpoint` with `token`: the status code and the
/// body of the answer.
async fn request(d: &Daemon, method: &str, name: &str, endpoint: ExtEndpoint, token: &str, body: &str) -> (u16, String) {
    let mut stream = connect(d).await;
    let request = format!(
        "{method} {} HTTP/1.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
        extension_path(name, endpoint),
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    let code = answer.split(' ').nth(1).unwrap().parse().unwrap();
    let body = answer.split_once("\r\n\r\n").map(|(_, body)| body.to_string()).unwrap_or_default();
    (code, body)
}

/// `name`'s event stream, past the answer's head.
async fn events(d: &Daemon, name: &str, token: &str) -> BufReader<tokio::net::TcpStream> {
    let mut stream = connect(d).await;
    let request = format!(
        "GET {} HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n",
        extension_path(name, ExtEndpoint::Events)
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    reader.read_line(&mut status).await.unwrap();
    assert!(status.contains("200"), "{status}");
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap() == 0 || line == "\r\n" {
            break;
        }
    }
    reader
}

async fn next_event(reader: &mut BufReader<tokio::net::TcpStream>) -> ExtEvent {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            if let Some(json) = line.strip_prefix("data: ") {
                return serde_json::from_str(json.trim()).unwrap();
            }
        }
    })
    .await
    .expect("an event")
}

/// Waits for the daemon to hold the stream a test just opened.
async fn listening(d: &Daemon, name: &str) {
    for _ in 0..100 {
        if d.extension_listening(name) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the stream never opened");
}

const CONTAINERS: &str = r#"{
    "name": "containers", "title": "Containers",
    "columns": [{"title": "Name"}, {"title": "State"}],
    "rows": [{"key": "web", "cells": [{"kind": "text", "text": "web"}, {"kind": "text", "text": "up"}],
              "status": {"label": "running", "tone": "good"}}],
    "actions": [
        {"id": "restart", "label": "Restart", "key": "r", "confirm": "Restart it?"},
        {"id": "refresh", "label": "Refresh", "view_wide": true}
    ]
}"#;

fn view(json: &str) -> ExtView {
    serde_json::from_str(json).unwrap()
}

fn command(name: &str, key: Option<ExtKey>, scope: CommandScope) -> ExtCommand {
    ExtCommand { extension: String::new(), name: name.into(), title: name.into(), key, scope, help: None }
}

#[tokio::test]
async fn a_view_put_by_an_admitted_extension_is_published_under_its_name() {
    let d = served();
    let token = d.admit_extension("docker");
    let mut news = d.subscribe_extensions();

    let (code, body) = request(&d, "PUT", "docker", ExtEndpoint::View, &token, CONTAINERS).await;

    assert_eq!(code, 200, "{body}");
    let views = d.extension_views();
    assert_eq!(views.len(), 1);
    assert_eq!((views[0].extension.as_str(), views[0].name.as_str()), ("docker", "containers"));
    assert!(matches!(news.try_recv(), Ok(ExtensionNews::Views)));
}

#[tokio::test]
async fn a_put_without_the_extensions_own_token_is_refused() {
    let d = served();
    let docker = d.admit_extension("docker");
    let _other = d.admit_extension("other");

    let (code, _) = request(&d, "PUT", "other", ExtEndpoint::View, &docker, CONTAINERS).await;
    assert_eq!(code, 401, "one extension's token is not another's");
    let hook_token = d.hook_token.clone();
    let (code, _) = request(&d, "PUT", "docker", ExtEndpoint::View, &hook_token, CONTAINERS).await;
    assert_eq!(code, 401, "nor is a pane's");
    let (code, _) = request(&d, "PUT", "stranger", ExtEndpoint::View, &docker, CONTAINERS).await;
    assert_eq!(code, 401, "nor may one the daemon never started publish");
    assert!(d.extension_views().is_empty());
}

#[tokio::test]
async fn an_endpoint_takes_only_its_own_method() {
    let d = served();
    let token = d.admit_extension("docker");
    let (code, body) = request(&d, "POST", "docker", ExtEndpoint::View, &token, CONTAINERS).await;
    assert_eq!(code, 405);
    assert!(body.contains("PUT"), "{body}");
}

#[tokio::test]
async fn an_action_reaches_the_extensions_event_stream_and_the_view_stays_as_published() {
    let d = served();
    let token = d.admit_extension("docker");
    request(&d, "PUT", "docker", ExtEndpoint::View, &token, CONTAINERS).await;
    let mut stream = events(&d, "docker", &token).await;
    listening(&d, "docker").await;

    d.extension_action("docker", "containers", Some("web".into()), "restart").unwrap();
    assert_eq!(
        next_event(&mut stream).await,
        ExtEvent::Action { view: "containers".into(), action: "restart".into(), row: Some("web".into()) }
    );
    d.extension_action("docker", "containers", Some("web".into()), "refresh").unwrap();
    assert_eq!(
        next_event(&mut stream).await,
        ExtEvent::Action { view: "containers".into(), action: "refresh".into(), row: None },
        "a view-wide action names no row"
    );
    let up = argus_protocol::Value::Text { text: "up".into() };
    assert_eq!(d.extension_views()[0].rows[0].cells[1], up, "nothing predicted");

    assert!(d.extension_action("docker", "containers", Some("db".into()), "restart").is_err(), "no such row");
    assert!(d.extension_action("docker", "containers", None, "restart").is_err(), "a row action needs a row");
    assert!(d.extension_action("docker", "containers", Some("web".into()), "delete").is_err());
}

#[tokio::test]
async fn an_action_for_an_extension_not_listening_is_refused_in_words() {
    let d = served();
    d.admit_extension("docker");
    d.publish_views("docker", vec![view(CONTAINERS)]).unwrap();
    let refused = d.extension_action("docker", "containers", Some("web".into()), "restart").unwrap_err();
    assert!(refused.to_string().contains("not listening"), "{refused}");
}

/// The window refuses a stale action in these same words, from the same
/// rule, before it sends; the daemon's are for a window a publish behind.
#[tokio::test]
async fn an_action_is_refused_in_the_words_the_window_uses() {
    let d = served();
    d.admit_extension("docker");
    d.publish_views("docker", vec![view(CONTAINERS)]).unwrap();
    let refused = |row: Option<&str>, action: &str| {
        d.extension_action("docker", "containers", row.map(str::to_string), action).unwrap_err().to_string()
    };
    assert_eq!(refused(Some("web"), "delete"), "Containers offers no delete");
    assert_eq!(refused(Some("gone"), "restart"), "Containers has no row gone");
    assert_eq!(refused(None, "restart"), "Restart needs a row");
}

#[tokio::test]
async fn a_view_that_breaks_a_limit_is_refused_whole_by_its_path() {
    let d = served();
    let token = d.admit_extension("docker");
    let bad = CONTAINERS.replace(r#"{"kind": "text", "text": "up"}"#, r#"{"kind": "link", "label": "x", "url": "file:///x"}"#);
    let (code, body) = request(&d, "PUT", "docker", ExtEndpoint::View, &token, &bad).await;
    assert_eq!(code, 409);
    assert!(body.starts_with("views[0].rows[0][1]"), "{body}");
    assert!(d.extension_views().is_empty());
}

#[tokio::test]
async fn a_view_that_is_not_one_is_refused_in_the_views_own_words() {
    // One view or a list of them, told apart by the first byte past any
    // blanks: read as an untagged pair, either refusal said only that
    // neither matched.
    let d = served();
    let token = d.admit_extension("docker");
    for (body, column) in [(r#"{"title":"Web"}"#, 15), (r#"  [{"title":"Web"}]"#, 18)] {
        let (code, answer) = request(&d, "PUT", "docker", ExtEndpoint::View, &token, body).await;
        let words = format!("not a view: missing field `name` at line 1 column {column}");
        assert_eq!((code, answer), (400, words), "{body}");
    }
}

#[tokio::test]
async fn a_views_place_and_icon_reach_the_clients_and_a_bad_icon_is_refused_by_its_path() {
    let d = served();
    let token = d.admit_extension("docker");
    let staged = CONTAINERS.replacen(r#""title": "Containers","#, r#""title": "Containers", "place": "stage", "icon": "box","#, 1);
    let (code, body) = request(&d, "PUT", "docker", ExtEndpoint::View, &token, &staged).await;
    assert_eq!(code, 200, "{body}");
    let published = &d.extension_views()[0];
    assert_eq!((published.place, published.icon.as_deref()), (argus_protocol::ExtPlace::Stage, Some("box")));
    let bad = CONTAINERS.replacen(r#""title": "Containers","#, r#""title": "Containers", "icon": "<svg/>","#, 1);
    let (code, body) = request(&d, "PUT", "docker", ExtEndpoint::View, &token, &bad).await;
    assert_eq!(code, 409);
    assert!(body.starts_with("views[0].icon"), "{body}");
}

#[tokio::test]
async fn an_action_key_the_view_keeps_is_refused_and_the_action_kept() {
    let d = served();
    d.admit_extension("docker");
    let mut keyed = view(CONTAINERS);
    keyed.actions[1].key = Some("j".into());
    keyed.actions.push(serde_json::from_str(r#"{"id": "again", "label": "Again", "key": "r"}"#).unwrap());

    let refusals = d.publish_views("docker", vec![keyed]).unwrap();

    let names: Vec<&str> = refusals.refused.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["containers/refresh", "containers/again"]);
    let published = &d.extension_views()[0];
    assert_eq!(published.actions.len(), 3);
    assert_eq!(published.actions[0].key.as_deref(), Some("r"), "the first to ask keeps it");
    assert_eq!(published.actions[1].key, None);
}

#[tokio::test]
async fn a_command_key_argus_or_another_extension_has_is_refused_with_why() {
    let d = served();
    d.admit_extension("docker");
    d.admit_extension("k8s");
    let ctrl_d = ExtKey { ctrl: true, ..ExtKey::plain("d") };
    let refusals = d
        .publish_commands("docker", vec![
            command("deploy", Some(ctrl_d.clone()), CommandScope::Anywhere),
            command("checkouts", Some(ExtKey::plain("3")), CommandScope::Anywhere),
            command("down", Some(ExtKey::plain("j")), CommandScope::Checkout),
        ])
        .unwrap();
    assert_eq!(refusals.refused.len(), 2, "{refusals:?}");
    assert!(refusals.refused[0].reason.contains("stage"));
    assert!(refusals.refused[1].reason.contains("Argus's own"));

    let refusals = d.publish_commands("k8s", vec![command("apply", Some(ctrl_d), CommandScope::Anywhere)]).unwrap();
    assert!(refusals.refused[0].reason.contains("docker"), "{refusals:?}");

    let commands = d.extension_commands();
    let keyed: Vec<&str> = commands.iter().filter(|c| c.key.is_some()).map(|c| c.name.as_str()).collect();
    assert_eq!(keyed, ["deploy"], "every command kept, only the free key bound");
    assert_eq!(commands.len(), 4);
}

#[tokio::test]
async fn a_put_of_commands_answers_with_the_keys_refused() {
    let d = served();
    let token = d.admit_extension("docker");
    let body = r#"[{"name": "up", "title": "Compose up", "key": {"key": "k", "ctrl": true}}]"#;
    let (code, answer) = request(&d, "PUT", "docker", ExtEndpoint::Commands, &token, body).await;
    assert_eq!(code, 200);
    let refusals: argus_protocol::Refusals = serde_json::from_str(&answer).unwrap();
    assert_eq!(refusals.refused[0].name, "up");
}

#[tokio::test]
async fn a_command_carries_where_the_person_was_and_its_id() {
    let d = served();
    let token = d.admit_extension("docker");
    d.publish_commands("docker", vec![command("up", None, CommandScope::Anywhere)]).unwrap();
    let mut stream = events(&d, "docker", &token).await;
    listening(&d, "docker").await;
    let checkout = only_checkout(&d);
    let viewer = d.new_viewer();

    d.extension_command(viewer, "docker", "up", &View { checkout: Some(checkout), ..View::default() }).unwrap();

    let ExtEvent::Command { command, context, .. } = next_event(&mut stream).await else {
        panic!("not a command");
    };
    assert_eq!(command, "up");
    let context = context.unwrap();
    assert_eq!(context.checkout.map(|c| c.id), Some(checkout));
    assert!(context.repository.is_some(), "read against the tree");
    assert!(d.extension_command(viewer, "docker", "down", &View::default()).is_err());
}

fn confirm(answers: Option<u64>) -> ExtDialog {
    ExtDialog { title: "Restart".into(), kind: ExtDialogKind::Confirm { message: "sure?".into(), yes: None, danger: false }, answers }
}

/// The client a dialog was put to, and whether it was prompted.
fn put_to(news: &mut broadcast::Receiver<ExtensionNews>) -> (ViewerId, bool) {
    loop {
        match news.try_recv().expect("a dialog") {
            ExtensionNews::Dialog { viewer, prompted, .. } => return (viewer, prompted),
            _ => continue,
        }
    }
}

#[tokio::test]
async fn a_dialog_goes_to_the_window_the_person_is_at_that_can_draw_one() {
    let d = served();
    d.admit_extension("docker");
    let (terminal, window, other) = (d.new_viewer(), d.new_viewer(), d.new_viewer());
    d.extension_asker(window, true);
    d.extension_asker(other, true);
    d.report_view(window, View { active: true, ..View::default() });
    d.report_view(other, View { active: true, ..View::default() });
    d.report_view(terminal, View { active: true, ..View::default() });
    let mut news = d.subscribe_extensions();

    d.raise_dialog("docker", confirm(None)).unwrap();

    assert_eq!(put_to(&mut news), (other, false), "the latest that can ask, not the terminal");
}

#[tokio::test]
async fn a_dialog_answering_a_command_goes_to_where_it_was_run_and_takes_the_keys() {
    let d = served();
    let token = d.admit_extension("docker");
    d.publish_commands("docker", vec![command("up", None, CommandScope::Anywhere)]).unwrap();
    let mut stream = events(&d, "docker", &token).await;
    listening(&d, "docker").await;
    let (ran_in, latest) = (d.new_viewer(), d.new_viewer());
    for viewer in [ran_in, latest] {
        d.extension_asker(viewer, true);
        d.report_view(viewer, View { active: true, ..View::default() });
    }
    d.extension_command(ran_in, "docker", "up", &View::default()).unwrap();
    let ExtEvent::Command { id, .. } = next_event(&mut stream).await else { panic!("not a command") };
    let mut news = d.subscribe_extensions();

    d.raise_dialog("docker", confirm(Some(id))).unwrap();

    assert_eq!(put_to(&mut news), (ran_in, true));
}

#[tokio::test]
async fn a_viewer_is_put_the_dialogs_still_asked_of_it() {
    let d = served();
    let token = d.admit_extension("docker");
    d.publish_commands("docker", vec![command("up", None, CommandScope::Anywhere)]).unwrap();
    let mut stream = events(&d, "docker", &token).await;
    listening(&d, "docker").await;
    let (window, other) = (d.new_viewer(), d.new_viewer());
    d.extension_asker(window, true);
    d.extension_command(window, "docker", "up", &View::default()).unwrap();
    let ExtEvent::Command { id: ran, .. } = next_event(&mut stream).await else { panic!("not a command") };

    let answered = d.raise_dialog("docker", confirm(None)).unwrap();
    let asked = d.raise_dialog("docker", confirm(Some(ran))).unwrap();
    d.answer_dialog(window, answered, Some(ExtAnswer::Yes)).unwrap();

    let put = d.dialogs_put_to(window);
    assert_eq!(put.len(), 1, "{put:?}");
    let ExtensionNews::Dialog { viewer, extension, id, prompted, .. } = &put[0] else { panic!("a dialog") };
    assert_eq!((*viewer, extension.as_str(), *id, *prompted), (window, "docker", asked, true));
    assert!(d.dialogs_put_to(other).is_empty(), "nothing was put to another client");
}

#[tokio::test]
async fn an_answer_or_its_window_going_reaches_the_extension_once() {
    let d = served();
    let token = d.admit_extension("docker");
    let mut stream = events(&d, "docker", &token).await;
    listening(&d, "docker").await;
    let (window, elsewhere) = (d.new_viewer(), d.new_viewer());
    d.extension_asker(window, true);

    let first = d.raise_dialog("docker", confirm(None)).unwrap();
    assert!(d.answer_dialog(elsewhere, first, Some(ExtAnswer::Yes)).is_err(), "only the window asked answers");
    assert!(d.answer_dialog(window, first, Some(ExtAnswer::Text("x".into()))).is_err(), "in its own kind");
    d.answer_dialog(window, first, Some(ExtAnswer::Yes)).unwrap();
    assert_eq!(next_event(&mut stream).await, ExtEvent::Answer { dialog: first, answer: ExtAnswer::Yes });
    assert!(d.answer_dialog(window, first, Some(ExtAnswer::Yes)).is_err(), "answered once");

    let second = d.raise_dialog("docker", confirm(None)).unwrap();
    d.forget_extension_viewer(window);
    assert_eq!(next_event(&mut stream).await, ExtEvent::Cancel { dialog: second });
    assert!(d.raise_dialog("docker", confirm(None)).is_err(), "no window is left to ask");
}

#[tokio::test]
async fn an_extension_that_goes_takes_its_tabs_keys_and_dialogs_with_it() {
    let d = served();
    d.admit_extension("docker");
    let window = d.new_viewer();
    d.extension_asker(window, true);
    d.publish_views("docker", vec![view(CONTAINERS)]).unwrap();
    d.publish_commands("docker", vec![command("up", None, CommandScope::Anywhere)]).unwrap();
    let dialog = d.raise_dialog("docker", confirm(None)).unwrap();
    let mut news = d.subscribe_extensions();

    d.retire_extension("docker");

    assert!(d.extension_views().is_empty());
    assert!(d.extension_commands().is_empty());
    let told: Vec<ExtensionNews> = std::iter::from_fn(|| news.try_recv().ok()).collect();
    assert!(told.iter().any(|n| matches!(n, ExtensionNews::Withdrawn { viewer, id } if *viewer == window && *id == dialog)));
    assert!(told.iter().any(|n| matches!(n, ExtensionNews::Views)));
    assert!(told.iter().any(|n| matches!(n, ExtensionNews::Commands)));
}

#[tokio::test]
async fn a_command_whose_help_breaks_its_limit_is_refused_by_its_path() {
    let d = served();
    let token = d.admit_extension("docker");
    let body = r#"[{"name": "up", "title": "Up", "help": "one\ntwo"}]"#;
    let (code, answer) = request(&d, "PUT", "docker", ExtEndpoint::Commands, &token, body).await;
    assert_eq!((code, answer.as_str()), (409, "commands[0].help: one line of at most 200 bytes"));
    let twice = r#"[{"name": "up", "title": "Up"}, {"name": "up", "title": "Again"}]"#;
    let (code, answer) = request(&d, "PUT", "docker", ExtEndpoint::Commands, &token, twice).await;
    assert_eq!((code, answer.as_str()), (409, r#"commands[1].name: "up" is another command's"#));
    assert!(d.extension_commands().is_empty());
}

#[tokio::test]
async fn what_an_extension_says_it_is_is_held_until_it_goes() {
    let d = served();
    let token = d.admit_extension("docker");
    let about = r#"{"summary": "Docker containers", "guide": "Press r to restart."}"#;
    let (code, body) = request(&d, "PUT", "docker", ExtEndpoint::About, &token, about).await;
    assert_eq!(code, 200, "{body}");
    let described = d.describe_extensions();
    assert_eq!(described[0].about.as_ref().map(|a| a.summary.as_str()), Some("Docker containers"));

    let (code, body) = request(&d, "PUT", "docker", ExtEndpoint::About, &token, r#"{"summary": ""}"#).await;
    assert_eq!((code, body.as_str()), (409, "summary: one line of at most 200 bytes"));
    let (code, _) = request(&d, "PUT", "docker", ExtEndpoint::About, &token, "[]").await;
    assert_eq!(code, 400);

    d.retire_extension("docker");
    d.admit_extension("docker");
    assert_eq!(d.describe_extensions()[0].about, None, "a restart starts from nothing");
}

#[tokio::test]
async fn a_dialog_and_a_pane_are_answered_with_their_ids() {
    let d = served();
    let token = d.admit_extension("docker");
    d.extension_asker(d.new_viewer(), true);
    let dialog = r#"{"title": "Restart", "kind": {"confirm": {"message": "sure?"}}}"#;
    let (code, body) = request(&d, "POST", "docker", ExtEndpoint::Dialog, &token, dialog).await;
    assert_eq!(code, 200, "{body}");
    let raised: argus_protocol::ExtDialogRaised = serde_json::from_str(&body).unwrap();
    assert!(raised.dialog > 0);
}

#[tokio::test]
async fn a_description_holds_each_running_extension_without_its_rows() {
    let d = served();
    d.admit_extension("docker");
    d.admit_extension("idle");
    d.publish_views("docker", vec![view(CONTAINERS)]).unwrap();
    d.publish_commands("docker", vec![command("up", None, CommandScope::Anywhere)]).unwrap();

    let described = d.describe_extensions();

    let names: Vec<&str> = described.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["docker", "idle"], "one that published nothing is still running");
    assert_eq!(described[0].commands[0].name, "up");
    assert_eq!(described[0].views[0].name, "containers");
    assert!(described[0].views[0].rows.is_empty(), "what it shows is not what it is");
    assert_eq!(described[0].views[0].actions.len(), 2);
}

#[tokio::test]
async fn a_notice_is_one_line_for_the_status_bar() {
    let d = served();
    let token = d.admit_extension("docker");
    let mut news = d.subscribe_extensions();
    let (code, _) = request(&d, "POST", "docker", ExtEndpoint::Notice, &token, "web restarted\nmore").await;
    assert_eq!(code, 200);
    assert!(matches!(
        news.try_recv(),
        Ok(ExtensionNews::Notice { extension, text }) if extension == "docker" && text == "web restarted"
    ));
}
