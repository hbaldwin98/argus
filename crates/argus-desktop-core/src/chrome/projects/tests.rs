use super::*;
use crate::connection::page::Project;

fn choice(id: u64, name: &str, needs_you: usize, working: usize) -> ProjectChoice {
    ProjectChoice { id, name: name.into(), needs_you, working }
}

fn project(id: u64, name: &str) -> Project {
    Project { id, name: name.into(), needs_you: 0, repositories: Vec::new(), running: Vec::new() }
}

fn workspace(id: u64, name: &str, open: bool, projects: usize) -> WorkspaceChoice {
    WorkspaceChoice { id, name: name.into(), open, projects, panes: 0 }
}

fn rail(projects: Vec<ProjectChoice>, current: Option<u64>) -> Rail {
    let project = current.and_then(|id| projects.iter().find(|choice| choice.id == id)).map(|choice| project(choice.id, &choice.name));
    Rail { loaded: true, projects, project, ..Rail::default() }
}

fn shape(entries: &[Entry]) -> Vec<&'static str> {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Label(_) => "label",
            Entry::Project { .. } => "project",
            Entry::Rule => "rule",
            Entry::AddProject => "add",
            Entry::CombineProjects => "combine",
            Entry::Workspaces { .. } => "workspaces",
            Entry::Hosts { .. } => "hosts",
            Entry::Remove { .. } => "remove",
        })
        .collect()
}

/// SHELL-SWITCH-2: who needs you, else who is working, else quiet.
#[test]
fn a_tally_says_who_needs_you_before_who_is_working_before_quiet() {
    assert_eq!(tally(&choice(1, "a", 3, 5)), Tally { words: "3 need you".into(), tone: Tone::Loud });
    assert_eq!(tally(&choice(1, "a", 0, 2)), Tally { words: "2 working".into(), tone: Tone::Active });
    assert_eq!(tally(&choice(1, "a", 0, 0)), Tally { words: "quiet".into(), tone: Tone::Muted });
}

/// SHELL-SWITCH-2.
#[test]
fn the_menu_lists_the_projects_then_add_then_hosts_then_remove_with_rules_between() {
    let entries = menu(&rail(vec![choice(1, "orion", 3, 0), choice(2, "work", 0, 1)], Some(1)));
    assert_eq!(shape(&entries), ["label", "project", "project", "rule", "add", "hosts", "rule", "remove"]);
    assert!(matches!(&entries[1], Entry::Project { current: true, .. }));
    assert!(matches!(&entries[2], Entry::Project { current: false, .. }));
    assert!(matches!(entries.last(), Some(Entry::Remove { project: 1, name }) if name == "orion"));
}

/// SHELL-SWITCH-1, 2: with no project the menu still offers the way to add one, and no remove.
#[test]
fn with_no_project_the_menu_is_still_the_way_to_add_the_first() {
    let entries = menu(&rail(Vec::new(), None));
    assert_eq!(shape(&entries), ["add", "hosts"]);
}

/// SHELL-SWITCH-2: a single project still has the whole menu.
#[test]
fn a_single_project_has_the_menu_too() {
    let entries = menu(&rail(vec![choice(1, "orion", 0, 0)], Some(1)));
    assert_eq!(shape(&entries), ["label", "project", "rule", "add", "hosts", "rule", "remove"]);
}

/// Combining is offered under Add a project only when the daemon can fold projects and the open
/// workspace holds two to fold, since one project has nothing to fold into it.
#[test]
fn combine_is_offered_only_with_the_daemon_s_word_and_two_projects() {
    let mut two = rail(vec![choice(1, "orion", 0, 0), choice(2, "work", 0, 0)], Some(1));
    assert!(!shape(&menu(&two)).contains(&"combine"), "an older daemon cannot");
    two.can_combine = true;
    assert_eq!(shape(&menu(&two)), ["label", "project", "project", "rule", "add", "combine", "hosts", "rule", "remove"]);
    let mut one = rail(vec![choice(1, "orion", 0, 0)], Some(1));
    one.can_combine = true;
    assert!(!shape(&menu(&one)).contains(&"combine"));
}

/// SHELL-SWITCH-2: Switch workspace only when the daemon lists workspaces, naming the open one.
#[test]
fn workspaces_are_offered_only_when_the_daemon_lists_some_and_name_the_open_one() {
    let mut with = rail(vec![choice(1, "orion", 0, 0)], Some(1));
    assert!(!shape(&menu(&with)).contains(&"workspaces"));
    with.workspaces = vec![workspace(1, "default", true, 1), workspace(2, "work", false, 3)];
    let entries = menu(&with);
    let Some(Entry::Workspaces { open, rows }) = entries.iter().find(|entry| matches!(entry, Entry::Workspaces { .. })) else { panic!("no workspaces") };
    assert_eq!(open.as_deref(), Some("default"));
    assert_eq!(rows.iter().map(|row| row.count.as_str()).collect::<Vec<_>>(), ["1 project", "3 projects"]);
}

/// SHELL-SWITCH-4.
#[test]
fn choosing_the_open_workspace_asks_nothing_and_another_asks_for_it() {
    let open = WorkspaceRow { id: 1, name: "default".into(), open: true, count: "1 project".into() };
    let other = WorkspaceRow { id: 2, name: "work".into(), open: false, count: "3 projects".into() };
    assert!(open.choose().is_none());
    assert!(matches!(other.choose(), Some(Call::OpenWorkspace { workspace: 2 })));
}

/// SHELL-SWITCH-5.
#[test]
fn this_machine_is_first_and_checked_until_a_remote_is_current() {
    let mut with = rail(Vec::new(), None);
    let host_rows = |rail: &Rail| match menu(rail).into_iter().find(|entry| matches!(entry, Entry::Hosts { .. })) {
        Some(Entry::Hosts { rows, connected }) => (rows, connected),
        _ => panic!("no hosts"),
    };
    let (rows, connected) = host_rows(&with);
    assert_eq!((rows.len(), rows[0].name.as_str(), rows[0].current, connected), (1, "This machine", true, None));
    with.hosts = vec![HostChoice { name: "devbox".into(), current: true }, HostChoice { name: "build".into(), current: false }];
    let (rows, connected) = host_rows(&with);
    assert_eq!(rows.iter().map(|row| (row.name.as_str(), row.current)).collect::<Vec<_>>(), [("This machine", false), ("devbox", true), ("build", false)]);
    assert_eq!(connected.as_deref(), Some("devbox"));
}

/// SHELL-SWITCH-5: the current host asks nothing, this machine asks for `None`.
#[test]
fn choosing_the_current_host_asks_nothing_and_this_machine_asks_for_none() {
    let here = HostRow { name: "This machine".into(), host: None, current: false };
    let there = HostRow { name: "devbox".into(), host: Some("devbox".into()), current: false };
    let now = HostRow { current: true, ..there.clone() };
    assert!(matches!(here.choose(), Some(Call::SetHost { host: None })));
    assert!(matches!(there.choose(), Some(Call::SetHost { host: Some(name) }) if name == "devbox"));
    assert!(now.choose().is_none());
}

/// SHELL-SWITCH-6.
#[test]
fn the_question_names_the_project_and_what_leaves_the_sidebar() {
    let one = question(&Removing { project: 1, name: "orion".into(), repositories: 1, panes: 0 });
    assert_eq!(one.title, "Remove orion?");
    assert_eq!(one.body, "Its 1 repository leave the sidebar. The folders stay where they are, and adding it again brings them back.");
    assert!(one.warning.is_none());
    let many = question(&Removing { project: 1, name: "orion".into(), repositories: 4, panes: 0 });
    assert!(many.body.starts_with("Its 4 repositories leave"));
}

/// SHELL-SWITCH-6.
#[test]
fn the_warning_counts_the_running_panes_in_the_singular_or_the_plural() {
    let warning = |panes| question(&Removing { project: 1, name: "x".into(), repositories: 1, panes }).warning;
    assert_eq!(warning(1).as_deref(), Some("1 pane is running in it; argusd removes it only once they are closed."));
    assert_eq!(warning(3).as_deref(), Some("3 panes are running in it; argusd removes it only once they are closed."));
    assert_eq!(warning(0), None);
}

#[test]
fn a_count_is_in_the_singular_only_for_one() {
    assert_eq!(counted(1, "project", "projects"), "1 project");
    assert_eq!(counted(0, "project", "projects"), "0 projects");
    assert_eq!(counted(2, "project", "projects"), "2 projects");
}
