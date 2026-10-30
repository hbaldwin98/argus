use super::*;
use crate::chrome::add_page::{FolderEntry, Held, ProjectRef};

fn page() -> AddProjectPage {
    AddProjectPage { open: true, path: Some("/home/me".into()), depth: Some(8), default_name: "me".into(), ..AddProjectPage::default() }
}

fn entry(name: &str) -> FolderEntry {
    FolderEntry { name: name.into(), is_repo: false, held: None }
}

/// SHELL-ADD-5.
#[test]
fn the_folder_list_is_bars_while_browsing_an_empty_one_then_an_error_then_the_rows() {
    let mut p = page();
    p.browsing = true;
    assert_eq!(folders(&p), Folders::Waiting);
    p.entries = vec![entry("a")];
    assert_eq!(folders(&p), Folders::Listed, "a folder already listed stays while the next is asked for");
    p.browsing = false;
    p.entries.clear();
    assert_eq!(folders(&p), Folders::Empty);
    p.listing_error = Some("cannot read".into());
    assert_eq!(folders(&p), Folders::Failed("cannot read".into()));
    p.browsing = true;
    assert_eq!(folders(&p), Folders::Waiting, "bars come before the error while a listing is awaited");
}

/// SHELL-ADD-6.
#[test]
fn a_new_repository_says_where_it_would_be_made() {
    let mut p = page();
    p.kind = AddKind::New;
    p.target = Some("/home/me/scratch".into());
    assert_eq!(outcome(&p), Outcome::New { target: Some("/home/me/scratch".into()) });
    p.previewing = true;
    assert!(matches!(outcome(&p), Outcome::New { .. }), "a new repository is not previewed");
}

/// SHELL-ADD-6.
#[test]
fn looking_is_while_previewing_or_a_path_with_no_depth_yet_and_no_error() {
    let mut p = page();
    p.previewing = true;
    assert_eq!(outcome(&p), Outcome::Looking);
    p.previewing = false;
    p.depth = None;
    assert_eq!(outcome(&p), Outcome::Looking);
    p.listing_error = Some("gone".into());
    assert!(matches!(outcome(&p), Outcome::Folder { .. }), "an error is not waiting");
    p.listing_error = None;
    p.path = None;
    assert!(matches!(outcome(&p), Outcome::Folder { .. }), "no path is not waiting either");
}

/// SHELL-ADD-6.
#[test]
fn one_repository_is_added_only_when_the_folder_is_exactly_that_repository() {
    let mut p = page();
    p.kind = AddKind::Repository;
    p.repositories = vec!["me".into()];
    assert_eq!(outcome(&p), Outcome::Repository { would_add: Some("me".into()) });
    p.repositories = vec!["me".into(), "me/nested".into()];
    assert_eq!(outcome(&p), Outcome::Repository { would_add: None });
    p.repositories = vec!["other".into()];
    assert_eq!(outcome(&p), Outcome::Repository { would_add: None });
    p.repositories.clear();
    assert_eq!(outcome(&p), Outcome::Repository { would_add: None });
}

/// SHELL-ADD-6.
#[test]
fn a_folder_of_repositories_counts_them_and_says_how_deep_it_searched_once_scanned() {
    let mut p = page();
    p.repositories = vec!["a".into(), "b".into()];
    let Outcome::Folder { heading, repositories, searched } = outcome(&p) else { panic!("a folder") };
    assert_eq!((heading.as_str(), repositories), ("Would add 2 repositories", vec!["a".to_owned(), "b".to_owned()]));
    assert!(searched.unwrap().starts_with("Searched up to 8 folders deep. A repository\u{2019}s own submodules"));
    p.repositories = vec!["a".into()];
    let Outcome::Folder { heading, .. } = outcome(&p) else { panic!("a folder") };
    assert_eq!(heading, "Would add 1 repository");
    p.repositories.clear();
    let Outcome::Folder { heading, .. } = outcome(&p) else { panic!("a folder") };
    assert_eq!(heading, "Would add 0 repositories");
}

/// SHELL-ADD-7.
#[test]
fn what_is_always_skipped_is_said_beside_the_skip_field_only_once_known() {
    let mut p = page();
    assert_eq!(always_skipped(&p), None);
    p.always_skipped = vec![".git".into(), "node_modules".into(), "target".into()];
    assert_eq!(always_skipped(&p).as_deref(), Some(".git, node_modules, target are always skipped"));
}

/// SHELL-ADD-11.
#[test]
fn the_footer_adds_the_new_repository_only_for_a_new_one() {
    assert_eq!(footer(AddKind::Folder), "Only Argus\u{2019}s config changes");
    assert_eq!(footer(AddKind::Repository), "Only Argus\u{2019}s config changes");
    assert_eq!(footer(AddKind::New), "Only Argus\u{2019}s config changes, and the new repository");
}

/// SHELL-ADD-9.
#[test]
fn add_is_named_by_the_typed_name_then_the_default_then_project() {
    let mut p = page();
    assert_eq!(add_label("  site  ", &p), "Add site");
    assert_eq!(add_label("   ", &p), "Add me");
    p.default_name.clear();
    assert_eq!(add_label("", &p), "Add project");
    p.adding = true;
    assert_eq!(add_label("site", &p), "Adding\u{2026}");
}

/// SHELL-ADD-9.
#[test]
fn add_cannot_be_pressed_while_blocked_or_adding() {
    let mut p = page();
    assert!(can_add(&p));
    p.blocked = Some("pick a folder".into());
    assert!(!can_add(&p));
    p.blocked = None;
    p.adding = true;
    assert!(!can_add(&p));
}

/// SHELL-ADD-7: the page's 250 ms pause.
#[test]
fn typing_is_sent_after_a_pause_of_a_quarter_second() {
    assert_eq!(FIELDS_PAUSE, Duration::from_millis(250));
}

#[test]
fn the_kinds_are_the_three_the_daemon_knows_in_the_pages_order() {
    assert_eq!(KINDS.map(|(kind, _)| kind), [AddKind::Folder, AddKind::Repository, AddKind::New]);
}

#[test]
fn with_a_project_to_add_to_the_dialog_and_its_kinds_read_as_a_repository_added_to_it() {
    let mut p = page();
    assert_eq!((title(&p).as_str(), kinds(&p)[0].1), ("Add a project", "A folder of repositories"));
    p.into = Some(ProjectRef { id: 1, name: "argus".into() });
    assert_eq!(title(&p), "Add a repository to argus");
    assert_eq!(kinds(&p).map(|(_, words)| words), ["A repository", "Every repository in a folder", "A new one (git init)"]);
    assert_eq!(kinds(&p).map(|(kind, _)| kind), [AddKind::Repository, AddKind::Folder, AddKind::New]);
    assert_eq!(add_label("typed", &p), "Add to argus", "a project's name is not the repository's");
    p.adding = true;
    assert_eq!(add_label("", &p), "Adding\u{2026}");
}

#[test]
fn a_folder_already_held_says_where_and_a_repository_that_is_a_project_says_it_stays() {
    let mut p = page();
    p.into = Some(ProjectRef { id: 1, name: "argus".into() });
    assert_eq!(held_words(Held::InTarget, &p), "already in argus");
    assert_eq!(held_words(Held::OwnProject, &p), "in its own project");
    assert_eq!(own_project_note(&p), None);
    p.own_project = true;
    assert_eq!(
        own_project_note(&p).as_deref(),
        Some("me is already its own project. Adding it here keeps that project too; to fold the whole project into argus, use Combine projects\u{2026}")
    );
}

#[test]
fn add_to_reads_the_target_against_the_project_the_rail_lists_and_offers_the_others() {
    let project = |id: u64, name: &str| ProjectRef { id, name: name.into() };
    let mut p = page();
    p.projects = vec![project(1, "argus"), project(2, "work"), project(3, "dots")];
    p.current = Some(project(1, "argus"));
    assert_eq!(add_to(&p), AddTo::New, "from the switcher it is a project of its own");
    p.into = Some(project(1, "argus"));
    assert_eq!(add_to(&p), AddTo::Current);
    p.into = Some(project(3, "dots"));
    assert_eq!(add_to(&p), AddTo::Another);
    assert_eq!(other_projects(&p).iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["work", "dots"], "the current one has its own segment");
    p.current = None;
    assert_eq!(add_to(&p), AddTo::Another);
    assert_eq!(other_projects(&p).len(), 3);
}
