//! The composer in a window: the message has the keys when it comes up, Ctrl Enter and the buttons
//! send what the page sends, a rail update for another pane touches nothing, and a start the daemon
//! made ends the message while the picks stay.

use argus_desktop_core::connection::page::{Checkout, Connection, Event, Project, Rail, Repository, Shown, Stage, Status};
use gpui_kit::TestAppContext;

use crate::keys::root::tests::{asked, click_label, drain, has_label, hear, open, press, type_text, Opened};

fn checkout(id: u64, name: &str, branch: &str, kind: &'static str) -> Checkout {
    Checkout { id, name: name.into(), branch: branch.into(), kind, git: None, clean: true, shared: None, open: true, count: 0, status: None, panes: Vec::new() }
}

fn repository(id: u64, name: &str, checkouts: Vec<Checkout>) -> Repository {
    Repository { id, name: name.into(), quiet: false, divider: false, open: !checkouts.is_empty(), panes: 0, status: None, used: None, checkouts }
}

fn shown(pane: u64) -> Shown {
    Shown { pane, repository: "argus".into(), checkout: "argus".into(), branch: "main".into(), agent: false, browser: false, title: "zsh".into(), status: "idle", resumed: None }
}

fn rail(stage: Stage) -> Rail {
    Rail {
        project: Some(Project {
            id: 1,
            name: "orion".into(),
            needs_you: 0,
            running: Vec::new(),
            repositories: vec![repository(10, "argus", vec![checkout(100, "argus", "main", "main"), checkout(101, "desktop", "desktop-design", "worktree")]), repository(11, "nebula", vec![checkout(110, "nebula", "main", "main")])],
        }),
        repository: Some(10),
        templates: vec!["claude".into(), "codex".into()],
        stage,
        loaded: true,
        ..Rail::default()
    }
}

fn hear_rail(cx: &mut TestAppContext, opened: &Opened, rail: Rail) {
    let status = Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None };
    hear(cx, opened, vec![Event::Status(status), Event::Rail(Box::new(rail))]);
}

fn composed(opened: &Opened) -> Vec<String> {
    asked(opened).into_iter().filter(|call| call.starts_with("Compose")).collect()
}

#[gpui_kit::test]
fn the_message_has_the_keys_as_the_stage_comes_up_and_ctrl_enter_starts_the_agent_with_it_untrimmed(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_rail(cx, &opened, rail(Stage::Composer));
    drain(&opened);
    type_text(cx, &opened, " hi ");
    press(cx, &opened, "ctrl-enter");
    assert_eq!(composed(&opened), ["Compose { checkout: 100, template: Some(\"claude\"), prompt: Some(\" hi \") }"]);
    assert!(asked(&opened).iter().all(|call| !call.starts_with("Key {")), "none of it reached a pane: {:?}", asked(&opened));
}

#[gpui_kit::test]
fn an_empty_message_starts_the_agent_idle_and_a_blank_one_too(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_rail(cx, &opened, rail(Stage::Composer));
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    type_text(cx, &opened, "  ");
    press(cx, &opened, "ctrl-enter");
    assert_eq!(composed(&opened), vec!["Compose { checkout: 100, template: Some(\"claude\"), prompt: None }"; 2]);
}

#[gpui_kit::test]
fn the_shell_button_starts_a_shell_that_ignores_the_message_and_start_agent_sends_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_rail(cx, &opened, rail(Stage::Composer));
    type_text(cx, &opened, "go");
    drain(&opened);
    click_label(cx, &opened, "Shell");
    assert_eq!(composed(&opened), ["Compose { checkout: 100, template: None, prompt: None }"]);
    click_label(cx, &opened, "Start agent");
    assert_eq!(composed(&opened)[1], "Compose { checkout: 100, template: Some(\"claude\"), prompt: Some(\"go\") }");
}

#[gpui_kit::test]
fn enter_types_a_newline_and_escape_does_nothing_in_the_composer(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_rail(cx, &opened, rail(Stage::Composer));
    drain(&opened);
    type_text(cx, &opened, "a");
    press(cx, &opened, "enter");
    press(cx, &opened, "escape");
    type_text(cx, &opened, "b");
    press(cx, &opened, "ctrl-enter");
    assert_eq!(composed(&opened), ["Compose { checkout: 100, template: Some(\"claude\"), prompt: Some(\"a\\nb\") }"]);
    assert_eq!(asked(&opened).len(), 1, "Escape closed nothing and sent nothing: {:?}", asked(&opened));
}

#[gpui_kit::test]
fn a_rail_update_for_another_pane_leaves_what_was_typed_and_where_the_keys_are(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_rail(cx, &opened, rail(Stage::Composer));
    type_text(cx, &opened, "ab");
    let mut other = rail(Stage::Composer);
    other.working = 3;
    other.shown = Some(shown(7));
    hear_rail(cx, &opened, other);
    type_text(cx, &opened, "c");
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    assert_eq!(composed(&opened), ["Compose { checkout: 100, template: Some(\"claude\"), prompt: Some(\"abc\") }"], "the message and the caret stayed");
}

#[gpui_kit::test]
fn a_start_the_daemon_made_ends_the_message_but_a_refused_one_keeps_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut up = rail(Stage::Composer);
    up.shown = Some(shown(1));
    hear_rail(cx, &opened, up.clone());
    type_text(cx, &opened, "keep");
    // Refused: the stage stays and nothing new is shown.
    hear_rail(cx, &opened, up.clone());
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    assert_eq!(composed(&opened), ["Compose { checkout: 100, template: Some(\"claude\"), prompt: Some(\"keep\") }"]);
    // Made: a new pane shown as the stage goes back to the Workspace.
    let mut made = up;
    made.stage = Stage::Workspace;
    made.shown = Some(shown(2));
    hear_rail(cx, &opened, made.clone());
    let mut again = made;
    again.stage = Stage::Composer;
    hear_rail(cx, &opened, again);
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    assert_eq!(composed(&opened), ["Compose { checkout: 100, template: Some(\"claude\"), prompt: None }"]);
}

#[gpui_kit::test]
fn the_question_and_the_place_are_the_repositorys(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_rail(cx, &opened, rail(Stage::Composer));
    assert!(has_label(cx, &opened, "What should start in argus?"));
    assert!(has_label(cx, &opened, "Starting in argus on main"));
    let mut elsewhere = rail(Stage::Composer);
    elsewhere.repository = Some(11);
    hear_rail(cx, &opened, elsewhere);
    assert!(has_label(cx, &opened, "What should start in nebula?"));
}

#[gpui_kit::test]
fn with_no_repository_the_start_buttons_are_off_and_say_why(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut empty = rail(Stage::Composer);
    empty.project.as_mut().unwrap().repositories.clear();
    hear_rail(cx, &opened, empty);
    assert!(has_label(cx, &opened, "No repository to start in"));
    assert!(has_label(cx, &opened, "Add a repository to the project to start here."));
    drain(&opened);
    click_label(cx, &opened, "Start agent");
    click_label(cx, &opened, "Shell");
    press(cx, &opened, "ctrl-enter");
    assert!(composed(&opened).is_empty(), "{:?}", asked(&opened));
}

#[gpui_kit::test]
fn picking_a_repository_other_than_the_rails_chooses_it_there_and_a_checkout_sends_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_rail(cx, &opened, rail(Stage::Composer));
    drain(&opened);
    let view = cx.update(|cx| crate::layout::bodies::Contents::peek(cx).stage(Stage::Composer).unwrap().downcast::<super::ComposeView>().unwrap());
    cx.update(|cx| view.update(cx, |this, cx| this.pick_checkout(101, cx)));
    assert!(asked(&opened).is_empty(), "a checkout is the draft's and redraws locally");
    cx.update(|cx| view.update(cx, |this, cx| this.pick_repository(11, cx)));
    assert_eq!(asked(&opened), ["ChooseRepository { repository: 11 }"]);
    cx.update(|cx| view.update(cx, |this, cx| this.pick_repository(10, cx)));
    assert_eq!(asked(&opened).len(), 1, "the rail's own repository is no call");
    assert_eq!(view.read_with(cx, |this, _| this.draft().checkout), None, "choosing a repository clears the checkout draft");
}
