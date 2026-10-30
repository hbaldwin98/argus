use super::*;
use crate::kit::menu::item::{lines, Hint};
use argus_desktop_core::connection::page::{HostChoice, Project, ProjectChoice, WorkspaceChoice};

fn rail(projects: &[(u64, &str, usize, usize)], current: Option<u64>) -> Rail {
    let projects: Vec<ProjectChoice> = projects.iter().map(|(id, name, needs_you, working)| ProjectChoice { id: *id, name: (*name).into(), needs_you: *needs_you, working: *working }).collect();
    let project = current.and_then(|id| projects.iter().find(|p| p.id == id)).map(|p| Project { id: p.id, name: p.name.clone(), needs_you: 0, repositories: Vec::new(), running: Vec::new() });
    Rail { loaded: true, projects, project, ..Rail::default() }
}

fn calls(item: &MenuItem<Does>) -> Vec<String> {
    match &item.as_choice().expect("a choice").does {
        Does::Send(calls) | Does::Go(calls) => calls.iter().map(|call| format!("{call:?}")).collect(),
        Does::Rename { .. } => vec!["rename".into()],
        Does::NewWorkspace => vec!["new workspace".into()],
    }
}

/// SHELL-SWITCH-2.
#[test]
fn the_menu_has_the_projects_add_hosts_and_remove_even_with_one_project() {
    let items = items(&rail(&[(1, "orion", 3, 1), (2, "work", 0, 1), (3, "idle", 0, 0)], Some(1)));
    assert_eq!(
        lines(&items),
        ["# Projects", "orion", "work", "idle", "---", "Add a project\u{2026}", "Connect to a host\u{2026} >", "---", "!Remove orion\u{2026}"]
    );
    let single = super::items(&rail(&[(1, "orion", 0, 0)], Some(1)));
    assert!(lines(&single).iter().any(|line| line.starts_with("Add a project")), "one project is no reason to hide the menu");
}

/// SHELL-SWITCH-1, 2: with no project it is how the first is added, and there is nothing to remove.
#[test]
fn with_no_project_the_menu_is_the_way_to_add_the_first() {
    let items = items(&rail(&[], None));
    assert_eq!(lines(&items), ["Add a project\u{2026}", "Connect to a host\u{2026} >"]);
}

/// SHELL-SWITCH-2: the open one is ticked, the others keep the room, and each says its tally.
#[test]
fn the_open_project_is_ticked_and_each_says_who_needs_you_before_who_works_before_quiet() {
    let items = items(&rail(&[(1, "orion", 3, 1), (2, "work", 0, 1), (3, "idle", 0, 0)], Some(1)));
    let choices: Vec<_> = items.iter().filter_map(MenuItem::as_choice).take(3).collect();
    assert_eq!(choices.iter().map(|c| c.checked).collect::<Vec<_>>(), [Some(true), Some(false), Some(false)]);
    let said: Vec<_> = choices
        .iter()
        .map(|c| match &c.hint {
            Hint::Toned(words, tone) => (words.as_str(), *tone),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(said, [("3 need you", Voice::Loud), ("1 working", Voice::Active), ("quiet", Voice::Muted)]);
}

/// SHELL-SWITCH-3: a project selects it, Add opens the dialog, Remove asks the question.
#[test]
fn a_project_adds_and_removes_by_the_pages_calls() {
    let items = items(&rail(&[(1, "orion", 0, 0), (2, "work", 0, 0)], Some(1)));
    assert_eq!(calls(&items[2]), ["SelectProject { project: 2 }"]);
    assert!(matches!(items[2].as_choice().unwrap().does, Does::Go(_)), "it goes to another project, so it leaves Git");
    assert_eq!(calls(&items[4]), ["OpenAddProject"]);
    assert_eq!(calls(items.last().unwrap()), ["RemoveProject { project: 1 }"]);
}

fn fold_of(items: &[MenuItem<Does>], label: &str) -> (Option<String>, Vec<MenuItem<Does>>) {
    items
        .iter()
        .find_map(|item| match item {
            MenuItem::Fold(fold) if fold.label.starts_with(label) => Some((fold.note.clone(), fold.items.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {label}"))
}

/// SHELL-SWITCH-4: the open workspace is ticked and asks nothing; another asks for itself.
#[test]
fn a_workspace_opens_only_when_it_is_not_open_and_says_how_many_projects_it_holds() {
    let mut with = rail(&[(1, "orion", 0, 0)], Some(1));
    assert!(!lines(&items(&with)).iter().any(|line| line.starts_with("Switch workspace")), "only when the daemon lists workspaces");
    with.workspaces = vec![
        WorkspaceChoice { id: 1, name: "default".into(), open: true, projects: 1, panes: 0 },
        WorkspaceChoice { id: 2, name: "work".into(), open: false, projects: 3, panes: 2 },
    ];
    let (note, rows) = fold_of(&items(&with), "Switch workspace");
    assert_eq!(note.as_deref(), Some("default"), "the open workspace is named beside the choice");
    assert_eq!(rows.iter().filter_map(|r| r.as_choice()).map(|r| r.checked).take(2).collect::<Vec<_>>(), [Some(true), Some(false)]);
    assert!(calls(&rows[0]).is_empty(), "the open one asks nothing");
    assert_eq!(calls(&rows[1]), ["OpenWorkspace { workspace: 2 }"]);
    assert_eq!(rows[1].as_choice().unwrap().hint, Hint::Text("3 projects".into()));
    assert_eq!(lines(&rows)[2..], ["---", "New workspace\u{2026}"], "the way to make another ends the list");
    assert_eq!(calls(&rows[3]), ["new workspace"]);
    assert_eq!(rows[3].as_choice().unwrap().lead, Some("+"));
}

/// SHELL-SWITCH-5.
#[test]
fn this_machine_comes_first_and_a_host_asks_for_itself_unless_it_is_the_one_reached() {
    let mut with = rail(&[], None);
    with.hosts = vec![HostChoice { name: "devbox".into(), current: true }, HostChoice { name: "build".into(), current: false }];
    let (note, rows) = fold_of(&items(&with), "Connect to a host");
    assert_eq!(note.as_deref(), Some("devbox"));
    assert_eq!(lines(&rows), ["This machine", "devbox", "build"]);
    assert_eq!(calls(&rows[0]), ["SetHost { host: None }"]);
    assert!(calls(&rows[1]).is_empty(), "the host being reached asks nothing");
    assert_eq!(calls(&rows[2]), ["SetHost { host: Some(\"build\") }"]);
}

/// SHELL-SWITCH-5: with no host listed the list says where they come from.
#[test]
fn with_no_hosts_the_list_says_where_they_come_from() {
    let (_, rows) = fold_of(&items(&rail(&[], None)), "Connect to a host");
    assert_eq!(lines(&rows), ["This machine", "# No hosts in ~/.ssh/config, or set ARGUS_DESKTOP_HOST"]);
    assert!(calls(&rows[0]).is_empty(), "this machine is the one reached, so it asks nothing");
}
