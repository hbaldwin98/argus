//! The add-a-project dialog in a test window: it is up while the session holds it, says only what
//! the page says, keeps what is typed over what was sent, and sends its gestures as the page does.

use std::time::Duration;

use argus_desktop_core::chrome::add_project::{AddKind, AddProjectPage, FolderEntry};
use argus_desktop_core::connection::page::{Event, Rail, Shown};
use gpui_kit::prelude::*;
use gpui_kit::test::TestQueryExt as _;
use gpui_kit::TestAppContext;

use crate::keys::root::tests::{blur, click_label, drain, has_label, hear, open, overlays, press, type_text, Opened};
use crate::kit::overlay::kind::Overlay;

fn page() -> AddProjectPage {
    AddProjectPage {
        open: true,
        path: Some("/home/me".into()),
        parent: Some("/home".into()),
        entries: vec![FolderEntry { name: "dotfiles".into(), is_repo: true }, FolderEntry { name: "scratch".into(), is_repo: false }],
        repositories: vec!["dotfiles".into()],
        depth: Some(8),
        always_skipped: vec![".git".into(), "target".into()],
        default_name: "me".into(),
        ..AddProjectPage::default()
    }
}

fn say(page: AddProjectPage) -> Event {
    Event::AddProject(Box::new(page))
}

/// The dialog up for `page`, settled, and the calls so far gone.
fn dialog(cx: &mut TestAppContext, page: AddProjectPage) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![say(page)]);
    assert_eq!(overlays(cx), [Overlay::AddProject]);
    cx.executor().advance_clock(Duration::from_secs(1));
    hear(cx, &opened, Vec::new());
    drain(&opened);
    opened
}

fn selected(cx: &mut TestAppContext, opened: &Opened, label: &str) -> Option<bool> {
    cx.update_window(opened.handle, |_, window, _| window.find_by_label(label).selected()).unwrap()
}

/// SHELL-ADD-1, 10: it is up exactly while the session holds the dialog.
#[gpui_kit::test]
fn the_dialog_is_up_while_the_session_holds_it_and_goes_when_it_does_not(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    assert!(has_label(cx, &opened, "Add a project"));
    hear(cx, &opened, vec![say(AddProjectPage::default())]);
    assert!(overlays(cx).is_empty());
}

/// SHELL-ADD-3: a press on a kind asks for it and draws nothing: the page says which is chosen.
#[gpui_kit::test]
fn a_kind_is_asked_for_and_not_drawn_until_the_page_says(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    assert_eq!(selected(cx, &opened, "A folder of repositories"), Some(true));
    click_label(cx, &opened, "One repository");
    assert_eq!(drain(&opened), ["AddProject { gesture: Kind { kind: Repository } }"]);
    assert_eq!(selected(cx, &opened, "One repository"), Some(false), "the client does not predict the result");
    hear(cx, &opened, vec![say(AddProjectPage { kind: AddKind::Repository, ..page() })]);
    assert_eq!(selected(cx, &opened, "One repository"), Some(true));
}

/// SHELL-ADD-2: the path field has the keys when the dialog opens, and Enter in it browses to what is
/// typed (SHELL-ADD-4); no key reaches the stage under it.
#[gpui_kit::test]
fn the_path_field_has_the_keys_and_enter_browses_to_what_is_typed(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    type_text(cx, &opened, "x3");
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["AddProject { gesture: Browse { path: \"/home/mex3\" } }"], "typed into the path, and the 3 was no stage switch");
}

/// SHELL-ADD-4: a redraw never overwrites a path being typed.
#[gpui_kit::test]
fn a_page_that_lands_while_the_path_is_being_typed_does_not_overwrite_it(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    type_text(cx, &opened, "ab");
    hear(cx, &opened, vec![say(AddProjectPage { path: Some("/elsewhere".into()), ..page() })]);
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["AddProject { gesture: Browse { path: \"/home/meab\" } }"]);
}

/// SHELL-ADD-4: Up goes to the parent, and is off with none.
#[gpui_kit::test]
fn up_goes_to_the_parent_and_does_nothing_without_one(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    click_label(cx, &opened, "Up one folder");
    assert_eq!(drain(&opened), ["AddProject { gesture: Browse { path: \"/home\" } }"]);
    hear(cx, &opened, vec![say(AddProjectPage { parent: None, ..page() })]);
    click_label(cx, &opened, "Up one folder");
    assert!(drain(&opened).is_empty());
}

/// SHELL-ADD-5: a folder in the list goes into it.
#[gpui_kit::test]
fn a_folder_row_goes_into_the_folder(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    click_label(cx, &opened, "Open scratch");
    assert_eq!(drain(&opened), ["AddProject { gesture: Into { name: \"scratch\" } }"]);
}

/// SHELL-ADD-5: a listing error is shown in place of the rows, and an empty folder says so.
#[gpui_kit::test]
fn the_folder_list_says_why_it_is_empty(cx: &mut TestAppContext) {
    let opened = dialog(cx, AddProjectPage { entries: Vec::new(), ..page() });
    assert!(has_label(cx, &opened, "Folders in /home/me"));
    hear(cx, &opened, vec![say(AddProjectPage { entries: Vec::new(), listing_error: Some("cannot read that folder".into()), ..page() })]);
    assert!(has_label(cx, &opened, "cannot read that folder"), "the error is drawn in the list");
}

/// SHELL-ADD-7: typing is sent after a pause, once, with what the fields say.
#[gpui_kit::test]
fn typing_in_the_name_is_sent_once_after_a_quarter_second(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    click_label(cx, &opened, "Project name");
    cx.update_window(opened.handle, |_, _, _| {}).unwrap();
    type_text(cx, &opened, "abc");
    assert!(drain(&opened).is_empty(), "no call a letter");
    cx.executor().advance_clock(Duration::from_millis(249));
    cx.run_until_parked();
    assert!(drain(&opened).is_empty(), "not before the pause is over");
    cx.executor().advance_clock(Duration::from_millis(2));
    cx.run_until_parked();
    assert_eq!(drain(&opened), ["AddProject { gesture: Fields { name: \"abc\", skip: \"\", folder: \"\" } }"]);
}

/// SHELL-ADD-9: a press on Add sends what the fields say now and cancels the pause.
#[gpui_kit::test]
fn add_sends_what_is_typed_now_and_the_pause_does_not_send_it_again(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    click_label(cx, &opened, "Project name");
    type_text(cx, &opened, "site");
    click_label(cx, &opened, "Add site");
    assert_eq!(drain(&opened), ["AddProject { gesture: Add { name: \"site\", skip: \"\", folder: \"\" } }"]);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(drain(&opened).is_empty(), "the add cancelled the pending fields");
}

/// SHELL-ADD-9: Add is named for the default name when none is typed, and off while blocked, with
/// no press sending anything.
#[gpui_kit::test]
fn add_says_the_default_name_and_is_off_while_blocked(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    assert!(has_label(cx, &opened, "Add me"));
    hear(cx, &opened, vec![say(AddProjectPage { blocked: Some("pick a folder".into()), ..page() })]);
    click_label(cx, &opened, "Add me");
    assert!(drain(&opened).is_empty(), "a blocked Add sends nothing");
    hear(cx, &opened, vec![say(AddProjectPage { adding: true, ..page() })]);
    assert!(has_label(cx, &opened, "Adding\u{2026}"));
    click_label(cx, &opened, "Adding\u{2026}");
    assert!(drain(&opened).is_empty(), "a repeated add while one is in flight is ignored");
}

/// SHELL-ADD-10: a refusal is shown in the dialog, which stays up with what was typed.
#[gpui_kit::test]
fn a_refusal_is_shown_and_what_was_typed_stays(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    click_label(cx, &opened, "Project name");
    type_text(cx, &opened, "site");
    hear(cx, &opened, vec![say(AddProjectPage { refusal: Some("that name is taken".into()), ..page() })]);
    assert!(has_label(cx, &opened, "that name is taken"));
    assert_eq!(overlays(cx), [Overlay::AddProject]);
    assert!(has_label(cx, &opened, "Add site"), "the typed name is still the name");
}

/// SHELL-ADD-11: Escape, Cancel and the close button send the close and put the dialog away.
#[gpui_kit::test]
fn escape_sends_the_close_and_does_not_come_back_with_a_page_still_open(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    press(cx, &opened, "escape");
    assert_eq!(drain(&opened), ["AddProject { gesture: Close }"]);
    assert!(overlays(cx).is_empty());
    hear(cx, &opened, vec![say(page())]);
    assert!(overlays(cx).is_empty(), "a page heard before the session answered the close is not a new dialog");
    hear(cx, &opened, vec![say(AddProjectPage::default())]);
    hear(cx, &opened, vec![say(page())]);
    assert_eq!(overlays(cx), [Overlay::AddProject], "a dialog the session opens again is one");
}

#[gpui_kit::test]
fn cancel_sends_the_close(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    click_label(cx, &opened, "Cancel");
    assert_eq!(drain(&opened), ["AddProject { gesture: Close }"]);
    assert!(overlays(cx).is_empty());
}

/// SHELL-ADD-6: the outcome says what adding would do, by kind.
#[gpui_kit::test]
fn the_outcome_says_what_adding_would_do(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    assert!(has_label(cx, &opened, "Would add 1 repository"));
    hear(cx, &opened, vec![say(AddProjectPage { kind: AddKind::Repository, repositories: Vec::new(), ..page() })]);
    assert!(has_label(cx, &opened, "Not a repository"));
    hear(cx, &opened, vec![say(AddProjectPage { kind: AddKind::New, target: Some("/home/me/scratch".into()), ..page() })]);
    assert!(has_label(cx, &opened, "New repository\u{2019}s folder"));
    blur(cx, &opened);
}

/// SHELL-ADD-7: only a folder of repositories has an Also skip field.
#[gpui_kit::test]
fn only_a_folder_of_repositories_has_the_skip_field(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    assert!(has_label(cx, &opened, "Also skip"));
    hear(cx, &opened, vec![say(AddProjectPage { kind: AddKind::Repository, ..page() })]);
    assert!(!has_label(cx, &opened, "Also skip"));
}

/// SHELL-ADD-2: the dialog takes the keys from a pane and gives them back when it closes.
#[gpui_kit::test]
fn the_dialog_takes_the_keys_from_the_pane_and_gives_them_back(cx: &mut TestAppContext) {
    let opened = open(cx);
    let shown = Shown { pane: 1, repository: "argus".into(), checkout: "main".into(), branch: "main".into(), agent: false, browser: false, title: "pwsh".into(), status: "idle", resumed: None };
    hear(cx, &opened, vec![Event::Rail(Box::new(Rail { loaded: true, shown: Some(shown), ..Rail::default() }))]);
    hear(cx, &opened, vec![say(page())]);
    cx.executor().advance_clock(Duration::from_secs(1));
    type_text(cx, &opened, "q");
    assert!(drain(&opened).iter().all(|call| !call.starts_with("Key")), "the pane heard nothing");
    hear(cx, &opened, vec![say(AddProjectPage::default())]);
    cx.executor().advance_clock(Duration::from_secs(1));
    hear(cx, &opened, Vec::new());
    press(cx, &opened, "q");
    assert!(drain(&opened).iter().any(|call| call.starts_with("Key")), "the pane has the keys again");
}
