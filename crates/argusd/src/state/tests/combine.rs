//! Folding several projects into one: what it refuses, what moves with the
//! repositories, and what a restart rebuilds from the store afterwards.
//!
//! The rules are the user's request to combine projects: repositories move
//! live, a directory already held is not listed twice, the emptied projects
//! leave the panel for good, and `into` keeps what is its own.

use super::*;

/// A project added at runtime, rooted at a temp directory holding one
/// repository per name.
fn add_rooted(d: &Daemon, names: &[&str]) -> (tempfile::TempDir, ProjectId) {
    let dir = tempfile::tempdir().unwrap();
    for name in names {
        let child = dir.path().join(name);
        std::fs::create_dir(&child).unwrap();
        let _repo = real_repo(&child);
    }
    let id = d.add_project(&dir.path().to_string_lossy(), None, &[]).unwrap();
    (dir, id)
}

fn project_repositories(d: &Daemon, id: ProjectId) -> Vec<String> {
    let project = d.snapshot().into_iter().find(|p| p.id == id).unwrap();
    let mut names: Vec<String> = project.repositories.into_iter().map(|r| r.name).collect();
    names.sort();
    names
}

fn refusal(d: &Daemon, into: ProjectId, from: &[ProjectId]) -> String {
    d.combine_projects(into, from).unwrap_err().to_string()
}

#[test]
fn combining_moves_every_repository_into_one_project_and_drops_the_rest() {
    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        let (_a, into) = add_rooted(&d, &["orion"]);
        let (_b, one) = add_rooted(&d, &["notes"]);
        let (_c, two) = add_rooted(&d, &["web", "api"]);
        let before: Vec<_> = d.snapshot().into_iter().flat_map(|p| p.repositories).map(|r| r.id).collect();

        assert_eq!(d.combine_projects(into, &[one, two]).unwrap(), into);

        let snapshot = d.snapshot();
        assert_eq!(snapshot.iter().map(|p| p.id).collect::<Vec<_>>(), vec![into]);
        let mut after: Vec<_> = snapshot[0].repositories.iter().map(|r| r.id).collect();
        let mut before = before;
        after.sort_by_key(|id| id.0);
        before.sort_by_key(|id| id.0);
        assert_eq!(after, before, "the same repositories, under the ids clients hold");
    });
}

#[test]
fn a_combine_is_refused_for_what_it_cannot_mean() {
    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        let (_a, into) = add_rooted(&d, &["orion"]);
        let (_b, other) = add_rooted(&d, &["notes"]);
        let stranger = ProjectId(9999);

        assert!(refusal(&d, into, &[]).contains("choose the projects"));
        assert!(refusal(&d, into, &[into]).contains("into itself"));
        assert!(refusal(&d, into, &[other, other]).contains("named twice"));
        assert!(refusal(&d, stranger, &[other]).contains("no such project"));
        assert!(refusal(&d, into, &[other, stranger]).contains("no such project"));

        assert_eq!(d.snapshot().len(), 2, "a refusal changes nothing");
        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(restarted.snapshot().len(), 2, "nor anything the store holds");
    });
}

#[tokio::test]
async fn running_panes_move_with_their_repository() {
    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        let (_a, into) = add_rooted(&d, &["orion"]);
        let (_b, from) = add_rooted(&d, &["notes"]);
        let checkout = d
            .snapshot()
            .into_iter()
            .find(|p| p.id == from)
            .unwrap()
            .repositories[0]
            .checkouts[0]
            .id;
        let pane = d.spawn_shell(checkout).unwrap();

        d.combine_projects(into, &[from]).unwrap();

        let snapshot = d.snapshot();
        let moved = snapshot[0]
            .repositories
            .iter()
            .flat_map(|r| &r.checkouts)
            .find(|c| c.id == checkout)
            .expect("the checkout kept its id");
        assert_eq!(moved.panes.iter().map(|p| p.id).collect::<Vec<_>>(), vec![pane], "and its shell");
        let _ = d.close_pane(pane);
    });
}

#[tokio::test]
async fn a_directory_both_projects_list_is_one_row_holding_both_sets_of_panes() {
    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        let (_a, into) = add_rooted(&d, &["orion"]);
        let (_b, from) = add_rooted(&d, &["notes"]);
        let shared = d.snapshot()[0].repositories[0].checkouts[0].path.clone();
        d.add_repository(from, &shared).unwrap();
        let theirs = d
            .snapshot()
            .into_iter()
            .find(|p| p.id == from)
            .unwrap()
            .repositories
            .into_iter()
            .find(|r| r.name == "orion")
            .unwrap()
            .checkouts[0]
            .id;
        let pane = d.spawn_shell(theirs).unwrap();

        d.combine_projects(into, &[from]).unwrap();

        assert_eq!(project_repositories(&d, into), vec!["notes", "orion"]);
        let snapshot = d.snapshot();
        let orion = snapshot[0].repositories.iter().find(|r| r.name == "orion").unwrap();
        assert_eq!(orion.checkouts.len(), 1);
        assert_eq!(orion.checkouts[0].panes.iter().map(|p| p.id).collect::<Vec<_>>(), vec![pane]);
        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(repository_names(&restarted).iter().filter(|n| *n == "orion").count(), 1);
        let _ = d.close_pane(pane);
    });
}

#[test]
fn a_restart_finds_each_moved_repository_once_in_the_combined_project() {
    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        let (_a, into) = add_rooted(&d, &["orion"]);
        let (_b, one) = add_rooted(&d, &["notes"]);
        let (_c, two) = add_rooted(&d, &["web"]);
        let into_name = d.snapshot()[0].name.clone();

        d.combine_projects(into, &[one, two]).unwrap();

        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(names_of(&restarted), vec![into_name]);
        let id = restarted.snapshot()[0].id;
        assert_eq!(project_repositories(&restarted, id), vec!["notes", "orion", "web"]);
    });
}

#[test]
fn the_combined_projects_own_repositories_still_answer_to_its_root() {
    // `into` keeps what is its own: a repository its root found is not
    // made a named one by a combine that also listed it, so it still
    // leaves when it leaves the disk.
    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        let (root, into) = add_rooted(&d, &["orion", "kept"]);
        let (_b, from) = add_rooted(&d, &["notes"]);
        let shared = root.path().join("orion");
        d.add_repository(from, &shared.to_string_lossy()).unwrap();

        d.combine_projects(into, &[from]).unwrap();
        std::fs::remove_dir_all(&shared).unwrap();

        let restarted = persistent(crate::config::load().unwrap());
        let id = restarted.snapshot()[0].id;
        assert_eq!(project_repositories(&restarted, id), vec!["kept", "notes"]);
    });
}

#[test]
fn a_rescan_of_the_combined_projects_root_keeps_what_it_cannot_reach() {
    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        let (_a, into) = add_rooted(&d, &["orion"]);
        let (_b, from) = add_rooted(&d, &["notes"]);

        d.combine_projects(into, &[from]).unwrap();
        d.reconcile_repositories();

        assert_eq!(project_repositories(&d, into), vec!["notes", "orion"]);
    });
}

#[test]
fn a_declared_project_combined_away_stays_away_and_its_file_is_untouched() {
    with_temp_config(|cfg| {
        let declared = tempfile::tempdir().unwrap();
        let repo = declared.path().join("legacy");
        std::fs::create_dir(&repo).unwrap();
        let _repo = real_repo(&repo);
        let file = format!(
            "[[project]]\nname = \"declared\"\nroot = \"{}\"\n",
            declared.path().to_string_lossy().replace('\\', "/")
        );
        std::fs::write(cfg.join("projects.toml"), &file).unwrap();
        let d = persistent(crate::config::load().unwrap());
        let from = d.snapshot()[0].id;
        let (_a, into) = add_rooted(&d, &["orion"]);
        let into_name = d.snapshot().into_iter().find(|p| p.id == into).unwrap().name;

        d.combine_projects(into, &[from]).unwrap();

        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(names_of(&restarted), vec![into_name]);
        let id = restarted.snapshot()[0].id;
        assert_eq!(project_repositories(&restarted, id), vec!["legacy", "orion"]);
        assert_eq!(std::fs::read_to_string(cfg.join("projects.toml")).unwrap(), file);
    });
}

#[test]
fn a_declared_project_combined_into_gains_the_repositories_after_a_restart() {
    with_temp_config(|cfg| {
        let declared = tempfile::tempdir().unwrap();
        let repo = declared.path().join("legacy");
        std::fs::create_dir(&repo).unwrap();
        let _repo = real_repo(&repo);
        std::fs::write(
            cfg.join("projects.toml"),
            format!(
                "[[project]]\nname = \"declared\"\nroot = \"{}\"\n",
                declared.path().to_string_lossy().replace('\\', "/")
            ),
        )
        .unwrap();
        let d = persistent(crate::config::load().unwrap());
        let into = d.snapshot()[0].id;
        let (_a, from) = add_rooted(&d, &["notes", "web"]);

        d.combine_projects(into, &[from]).unwrap();

        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(names_of(&restarted), vec!["declared"]);
        let id = restarted.snapshot()[0].id;
        assert_eq!(project_repositories(&restarted, id), vec!["legacy", "notes", "web"]);
    });
}

#[test]
fn the_combined_project_keeps_its_own_workspace() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        let (home_ws, work) = (workspace_named(&d, "default"), workspace_named(&d, "work"));
        let home = d.snapshot()[0].id;
        d.open_workspace(work).unwrap();
        let away = d.snapshot()[0].id;
        d.open_workspace(home_ws).unwrap();

        d.combine_projects(home, &[away]).unwrap();

        assert_eq!(names_of(&d), vec!["home-thing"]);
        assert_eq!(project_repositories(&d, home), vec!["day-job", "home-thing"]);
        d.open_workspace(work).unwrap();
        assert!(d.snapshot().is_empty(), "the other workspace lost its project");
    });
}

#[test]
fn a_feature_on_a_moved_repositorys_board_is_read_through_the_combined_project() {
    // Boards are filed under a repository's Git directory, so the board
    // goes where the repository goes with nothing renumbered.
    use argus_protocol::{FeatureAction, FeatureWrite};
    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        let (_a, into) = add_rooted(&d, &["orion"]);
        let (_b, from) = add_rooted(&d, &["notes"]);
        let checkout = d.snapshot()[1].repositories[0].checkouts[0].id;
        let open = FeatureAction::Open(FeatureWrite { title: "Trim".into(), body: None });
        d.feature_action_for_client(from, checkout, open).unwrap();
        let before = d.decision_board(from, checkout).unwrap();

        d.combine_projects(into, &[from]).unwrap();

        let after = d.decision_board(into, checkout).unwrap();
        let numbered = |b: &argus_protocol::DecisionBoard| {
            b.features.iter().map(|f| (f.number, f.slug.clone())).collect::<Vec<_>>()
        };
        assert_eq!(numbered(&after), numbered(&before));
        assert_eq!(numbered(&after), vec![(1, "trim".to_string())]);
    });
}
