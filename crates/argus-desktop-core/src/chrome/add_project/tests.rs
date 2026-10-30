use super::*;
use argus_protocol::DirEntry;

fn listing(request_id: u64, path: &str) -> DirListing {
    DirListing {
        request_id,
        path: path.into(),
        parent: Some("/home".into()),
        entries: vec![DirEntry { name: "web".into(), is_repo: true }],
        error: None,
    }
}

fn preview(request_id: u64, repositories: &[&str]) -> ProjectPreview {
    ProjectPreview {
        request_id,
        path: "/home/work".into(),
        repositories: repositories.iter().map(|r| r.to_string()).collect(),
        depth: 8,
        always_skipped: vec![".git".into()],
        error: None,
    }
}

/// A dialog open on `/home/work`, its preview answered with `found`.
fn browsed(found: &[&str]) -> AddProject {
    let mut out = Vec::new();
    let mut dialog = AddProject::new(1, &mut out);
    assert!(dialog.listed(listing(1, "/home/work"), || 2, &mut out));
    assert!(dialog.previewed(preview(2, found)));
    dialog
}

/// The one message an add into nothing sends.
fn one(dialog: &mut AddProject, request_id: u64) -> ClientMsg {
    let mut msgs = dialog.add(|_| request_id).expect("the add is allowed");
    assert_eq!(msgs.len(), 1);
    msgs.remove(0)
}

#[test]
fn opening_lists_where_the_daemon_starts_and_a_listing_asks_for_its_preview() {
    let mut out = Vec::new();
    let mut dialog = AddProject::new(1, &mut out);
    assert!(matches!(&out[..], [ClientMsg::ListDirectories { request_id: 1, path }] if path.is_empty()));
    assert!(dialog.page(&[], None).browsing);

    out.clear();
    dialog.listed(listing(1, "/home/work"), || 2, &mut out);
    assert!(matches!(&out[..], [ClientMsg::PreviewProject { request_id: 2, path, .. }] if path == "/home/work"));
    let page = dialog.page(&[], None);
    assert_eq!((page.path.as_deref(), page.browsing, page.previewing), (Some("/home/work"), false, true));
    assert_eq!(page.blocked.as_deref(), Some("looking for repositories"));
}

#[test]
fn an_answer_for_a_folder_since_left_is_dropped() {
    let mut out = Vec::new();
    let mut dialog = AddProject::new(1, &mut out);
    dialog.browse("/elsewhere".into(), 3, &mut out);
    assert!(!dialog.listed(listing(1, "/home"), || 9, &mut out), "the first listing is stale");
    assert!(dialog.listed(listing(3, "/elsewhere"), || 4, &mut out));
    assert!(!dialog.previewed(preview(9, &["old"])));
    assert!(dialog.previewed(preview(4, &["web"])));
    assert_eq!(dialog.page(&[], None).repositories, ["web"]);
}

#[test]
fn a_folder_adds_what_the_preview_found_skipping_what_was_typed() {
    let mut dialog = browsed(&["billing-api", "infra/k8s"]);
    let mut out = Vec::new();
    dialog.set_fields("work".into(), "vendor, archive".into(), String::new(), || 5, &mut out);
    assert!(
        matches!(&out[..], [ClientMsg::PreviewProject { request_id: 5, skip, .. }] if skip == &["vendor", "archive"]),
        "new skips scan again"
    );
    assert!(dialog.add(|_| 6).is_err(), "not until the new preview is in");
    dialog.previewed(preview(5, &["billing-api"]));

    let ClientMsg::AddProject { path, request_id, name, skip, init } = one(&mut dialog, 6) else {
        panic!("not an add");
    };
    assert_eq!(
        (path.as_str(), request_id, name.as_deref(), skip, init),
        ("/home/work", 6, Some("work"), vec!["vendor".to_string(), "archive".to_string()], false)
    );
    assert!(dialog.page(&[], None).adding);
    assert!(dialog.add(|_| 7).is_err(), "one add at a time");
}

#[test]
fn one_repository_is_only_a_folder_that_is_one() {
    let mut dialog = browsed(&["web", "api"]);
    dialog.set_kind(AddKind::Repository);
    assert_eq!(dialog.page(&[], None).blocked.as_deref(), Some("this folder is not a repository"));
    assert!(dialog.add(|_| 2).is_err());
    assert_eq!(dialog.page(&[], None).refusal.as_deref(), Some("this folder is not a repository"), "said in the dialog");

    let mut dialog = browsed(&[""]);
    dialog.set_kind(AddKind::Repository);
    assert_eq!(dialog.page(&[], None).repositories, ["work"], "the folder itself, by its name");
    let ClientMsg::AddProject { name: None, init: false, skip, .. } = one(&mut dialog, 3) else {
        panic!("a repository is added as itself");
    };
    assert!(skip.is_empty());
}

#[test]
fn a_new_repository_is_made_in_a_folder_named_inside_the_one_browsed() {
    let mut dialog = browsed(&[]);
    dialog.set_kind(AddKind::New);
    assert_eq!(dialog.page(&[], None).blocked.as_deref(), Some("name the new repository's folder"));
    dialog.set_fields(String::new(), String::new(), "scratch".into(), || 9, &mut Vec::new());
    let page = dialog.page(&[], None);
    assert_eq!((page.target.as_deref(), page.default_name.as_str()), (Some("/home/work/scratch"), "scratch"));
    let ClientMsg::AddProject { path, init: true, .. } = one(&mut dialog, 4) else {
        panic!("a new repository is initialised");
    };
    assert_eq!(path, "/home/work/scratch");
}

#[test]
fn a_refused_add_keeps_what_was_typed_and_says_why() {
    let mut dialog = browsed(&["web"]);
    dialog.set_fields("work".into(), String::new(), String::new(), || 9, &mut Vec::new());
    dialog.add(|_| 6).unwrap();
    let refusal = |request_id| RequestRefusal {
        request_id,
        reason: "there is already a project called work".into(),
        read: false,
    };
    assert!(!dialog.refused(&refusal(5)), "another request's refusal is not the add's");
    assert!(dialog.page(&[], None).adding);
    assert!(dialog.refused(&refusal(6)));
    let page = dialog.page(&[], None);
    assert_eq!((page.adding, page.name.as_str()), (false, "work"));
    assert_eq!(page.refusal.as_deref(), Some("there is already a project called work"));
    assert!(dialog.add(|_| 7).is_ok(), "and it can be tried again");
}

#[test]
fn only_a_folder_the_listing_holds_is_gone_into() {
    let mut dialog = browsed(&["web"]);
    let mut out = Vec::new();
    dialog.open("elsewhere", 5, &mut out);
    assert!(out.is_empty());
    dialog.open("web", 5, &mut out);
    assert!(matches!(&out[..], [ClientMsg::ListDirectories { request_id: 5, path }] if path == "/home/work/web"));
}

#[test]
fn a_path_is_joined_in_the_daemons_own_spelling() {
    assert_eq!(join(r"C:\Users\me", "x"), r"C:\Users\me\x");
    assert_eq!(join(r"C:\", "x"), r"C:\x");
    assert_eq!(join("/", "x"), "/x");
    assert_eq!(join("/home/me", "x"), "/home/me/x");
    assert_eq!(last_segment(r"C:\Users\me\"), "me");
}

/// Two projects holding `/src/argus` and `/src/web`, the second a project of its own.
fn held_tree() -> Vec<ProjectInfo> {
    use crate::fixtures::{checkout, project, repository};
    let at = |path: &str| {
        let mut c = checkout(1, "main", true, vec![]);
        c.path = path.into();
        c
    };
    vec![
        project(1, "src", vec![repository(10, "argus", None, vec![at("/home/work/argus/")])]),
        project(2, "web", vec![repository(20, "web", None, vec![at(r"/home/work/web")])]),
    ]
}

#[test]
fn a_folder_row_says_whether_the_project_added_to_holds_it_or_it_is_a_project_of_its_own() {
    let mut out = Vec::new();
    let mut dialog = AddProject::new(1, &mut out).for_project(ProjectId(1), AddKind::Folder);
    let mut found = listing(1, "/home/work");
    found.entries = ["argus", "web", "fresh"].map(|name| DirEntry { name: name.into(), is_repo: true }).to_vec();
    dialog.listed(found, || 2, &mut out);
    let held: Vec<_> = dialog.page(&held_tree(), Some(ProjectId(1))).entries.into_iter().map(|e| e.held).collect();
    assert_eq!(held, [Some(Held::InTarget), Some(Held::OwnProject), None], "a trailing slash is the same folder");
    let mut out = Vec::new();
    let mut elsewhere = AddProject::new(1, &mut out);
    elsewhere.listed(listing(1, "/home/work"), || 2, &mut out);
    assert!(elsewhere.page(&held_tree(), None).entries.iter().all(|e| e.held.is_none()), "only a project to add to has anything to compare with");
}

#[test]
fn a_repository_that_is_its_own_project_is_said_when_added_into_another() {
    let mut out = Vec::new();
    let mut dialog = AddProject::new(1, &mut out).for_project(ProjectId(1), AddKind::Repository);
    dialog.listed(listing(1, "/home/work/web"), || 2, &mut out);
    dialog.previewed(preview(2, &[""]));
    let page = dialog.page(&held_tree(), Some(ProjectId(1)));
    assert!(page.own_project);
    assert_eq!(page.into.map(|p| p.name), Some("src".into()));
    assert_eq!(page.projects.len(), 2);
    dialog.add_to(Some(ProjectId(2)));
    assert!(!dialog.page(&held_tree(), None).own_project, "into its own project it is just the repository");
    dialog.add_to(None);
    assert!(!dialog.page(&held_tree(), None).own_project);
}

#[test]
fn a_folder_into_a_project_needs_a_repository_in_it() {
    let mut dialog = browsed(&[]);
    dialog.add_to(Some(ProjectId(1)));
    assert_eq!(dialog.page(&[], None).blocked.as_deref(), Some("no repositories in this folder"));
    dialog.add_to(None);
    assert_eq!(dialog.page(&[], None).blocked, None, "a project of its own may be empty, the daemon says");
}

#[test]
fn each_repository_of_a_folder_is_asked_under_a_name_of_its_own_and_the_dialog_closes_after_the_last() {
    let mut dialog = browsed(&["web", ""]).for_project(ProjectId(2), AddKind::Folder);
    let mut ids = 10..;
    let msgs = dialog.add(|into| {
        assert_eq!(into, Some(ProjectId(2)));
        ids.next().unwrap()
    }).unwrap();
    let asked: Vec<_> = msgs.iter().map(|m| match m {
        ClientMsg::AddRepository { project: ProjectId(2), path, request_id } => (path.as_str(), *request_id),
        other => panic!("{other:?}"),
    }).collect();
    assert_eq!(asked, [("/home/work/web", 10), ("/home/work", 11)]);
    assert!(dialog.page(&[], None).adding);
    dialog.made(10);
    dialog.made(11);
    assert!(!dialog.settled(), "one is not in the tree yet");
    assert!(dialog.settled());
    assert!(!dialog.page(&[], None).adding);
}
