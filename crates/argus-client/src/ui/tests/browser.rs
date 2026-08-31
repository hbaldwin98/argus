//! The directory browser.

use super::*;

#[test]
fn the_browser_shows_where_you_are_and_what_is_under_it() {
    let mut app = app_browsing();
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("add project"), "{rendered}");
    assert!(rendered.contains("github.com"), "the breadcrumb");
    assert!(rendered.contains("add this directory"), "{rendered}");
    assert!(rendered.contains("orion"), "{rendered}");
    assert!(rendered.contains("enter open"), "the keys are on screen");
    assert!(rendered.contains("← up"), "the parent key is on screen");
}

#[test]
fn browsing_for_somewhere_to_put_a_repository_says_so() {
    // The same rows, asked a different question: confirming here puts
    // the new repository in this directory rather than adding the
    // directory itself.
    let mut app = app_browsing();
    app.dir_picker.as_mut().unwrap().target =
        crate::dirpicker::DirTarget::NewRepository(ProjectId(1));
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("new repository"), "{rendered}");
    assert!(rendered.contains("make it in this directory"), "{rendered}");
    assert!(!rendered.contains("add this directory"), "{rendered}");
    assert!(rendered.contains("enter choose on ·"), "{rendered}");
}

#[test]
fn naming_a_new_repository_shows_where_it_will_land() {
    let mut app = app_with_tree();
    app.prompt = Some(Prompt::NewRepository {
        project: ProjectId(1),
        parent: "/home/u/Source".to_string(),
        input: "thing".to_string(),
    });
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("new repository"), "{rendered}");
    assert!(rendered.contains("/home/u/Source/thing"), "{rendered}");
    assert!(
        rendered.contains("empty to use the directory"),
        "{rendered}"
    );
}

#[test]
fn a_new_repository_with_no_name_yet_points_at_the_directory_itself() {
    let mut app = app_with_tree();
    app.prompt = Some(Prompt::NewRepository {
        project: ProjectId(1),
        parent: "/home/u/Source".to_string(),
        input: String::new(),
    });
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("/home/u/Source"), "{rendered}");
}

#[test]
fn a_repository_among_the_directories_is_marked() {
    // Which children are already repos is the question the browser
    // exists to answer, and it is invisible from the name.
    let mut app = app_browsing();
    let rendered = lines(&draw(&mut app));
    // Rightmost match: the repositories column behind the modal also
    // has an "orion" on it.
    let row = rendered.iter().rev().find(|r| r.contains("orion")).unwrap();
    assert!(row.contains("git"), "{row}");
    let plain = rendered.iter().find(|r| r.contains("notes")).unwrap();
    assert!(!plain.contains("git"), "{plain}");
}

#[test]
fn typing_narrows_the_browser_to_what_matches() {
    let mut app = app_browsing();
    app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("notes"), "{rendered}");
    assert!(!rendered.contains("argus\n"), "{rendered}");
    assert!(
        !rendered.contains("add this directory"),
        "the row that answers no query steps aside"
    );
}

#[test]
fn an_unreadable_directory_says_so_instead_of_looking_empty() {
    let mut app = app_with_tree();
    let mut picker = crate::dirpicker::DirPicker::new(crate::dirpicker::DirTarget::Project, 1);
    picker.show(argus_protocol::DirListing {
        request_id: 1,
        path: "/root".to_string(),
        parent: Some("/".to_string()),
        entries: Vec::new(),
        error: Some("permission denied".to_string()),
    });
    app.dir_picker = Some(picker);
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("permission denied"), "{rendered}");
}

#[test]
fn a_breadcrumb_too_long_for_the_box_keeps_its_end() {
    // The segments nearest the cursor are the ones that say where you
    // are; the drive letter is not.
    let long = "/very/deep".repeat(20);
    assert_eq!(elide_head(&long, 12).chars().next(), Some('\u{2026}'));
    assert!(elide_head(&long, 12).ends_with("very/deep"));
    assert_eq!(elide_head("/short", 12), "/short");
}

#[test]
#[ignore]
fn dump_dir_picker() {
    let mut app = app_browsing();
    app.theme = Theme::default();
    for line in lines(&draw_at(&mut app, 100, 20)) {
        println!("|{line}");
    }
}

#[test]
#[ignore]
fn dump_picker() {
    let mut app = app_with_tree();
    app.theme = Theme::default();
    let mut p = crate::app::Picker::new(
        PickerKind::Branch {
            checkout: CheckoutId(10),
        },
        "switch branch",
        ["feature/login", "feature/logout", "hotfix", "release/2.1"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        0,
    );
    p.type_query("log");
    app.picker = Some(p);

    for line in lines(&draw_at(&mut app, 100, 20)) {
        println!("|{line}");
    }
}

#[test]
#[ignore]
fn dump_settings() {
    let mut app = app_with_tree();
    app.open_settings();
    for line in lines(&draw_at(&mut app, 100, 20)) {
        println!("|{line}");
    }
}

#[test]
fn an_empty_tree_renders_the_add_project_hint() {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    std::mem::forget(rx);
    let mut app = App::new(tx);
    // The hint wraps across the narrow column, so assert on its words
    // rather than on a contiguous phrase.
    let text = lines(&draw(&mut app)).join("\n");
    assert!(
        text.contains("no projects"),
        "a first run should say the tree is empty"
    );
    assert!(text.contains("add"), "and how to start:\n{text}");
}
