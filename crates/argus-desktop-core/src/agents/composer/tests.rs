use super::*;
use crate::connection::page::{Project, Shown, Stage};

fn checkout(id: u64, name: &str, branch: &str, kind: &'static str) -> Checkout {
    Checkout { id, name: name.into(), branch: branch.into(), kind, git: None, clean: true, shared: None, open: true, count: 0, status: None, panes: Vec::new() }
}

fn repository(id: u64, name: &str, checkouts: Vec<Checkout>) -> Repository {
    Repository { id, name: name.into(), quiet: false, divider: false, open: !checkouts.is_empty(), panes: 0, status: None, used: None, checkouts }
}

fn shown(pane: u64, repository: &str, checkout: &str) -> Shown {
    Shown { pane, repository: repository.into(), checkout: checkout.into(), branch: String::new(), agent: false, browser: false, title: "zsh".into(), status: "idle", resumed: None }
}

fn rail() -> Rail {
    Rail {
        project: Some(Project {
            id: 1,
            name: "orion".into(),
            needs_you: 0,
            running: Vec::new(),
            repositories: vec![
                repository(10, "argus", vec![checkout(100, "argus", "main", "main"), checkout(101, "desktop", "desktop-design", "worktree"), checkout(102, "fix", "fix", "worktree")]),
                repository(11, "nebula", vec![checkout(110, "nebula", "main", "main")]),
                repository(12, "folded", Vec::new()),
            ],
        }),
        repository: Some(10),
        templates: vec!["claude".into(), "codex".into()],
        stage: Stage::Composer,
        ..Rail::default()
    }
}

#[test]
fn the_rails_repository_is_shown_else_the_first() {
    let mut rail = rail();
    assert_eq!(ComposerPage::of(&rail, &Draft::default()).title, "What should start in argus?");
    rail.repository = Some(11);
    assert_eq!(ComposerPage::of(&rail, &Draft::default()).title, "What should start in nebula?");
    rail.repository = None;
    assert_eq!(ComposerPage::of(&rail, &Draft::default()).repository, Some(10), "none chosen: the first");
}

#[test]
fn a_project_with_no_repository_says_so_and_starts_nothing() {
    let mut rail = rail();
    rail.project.as_mut().unwrap().repositories.clear();
    let page = ComposerPage::of(&rail, &Draft::default());
    assert_eq!(page.title, "No repository to start in");
    assert_eq!(page.place, Where::NoRepository);
    assert!(!page.can_shell && !page.can_start);
}

#[test]
fn a_folded_repository_offers_no_checkout_and_says_to_open_it() {
    let mut rail = rail();
    rail.repository = Some(12);
    let page = ComposerPage::of(&rail, &Draft::default());
    assert_eq!(page.place, Where::Folded { repository: "folded".into() });
    assert!(page.checkouts.is_empty() && !page.can_shell && !page.can_start);
}

#[test]
fn the_checkout_is_the_drafts_while_listed_then_the_one_on_screen_then_the_first() {
    let mut rail = rail();
    let mut draft = Draft::default();
    assert_eq!(ComposerPage::of(&rail, &draft).checkout, Some(100));
    rail.shown = Some(shown(1, "argus", "desktop"));
    assert_eq!(ComposerPage::of(&rail, &draft).checkout, Some(101), "the one on screen in that repository");
    rail.shown = Some(shown(1, "nebula", "desktop"));
    assert_eq!(ComposerPage::of(&rail, &draft).checkout, Some(100), "a checkout of another repository is not on screen here");
    draft.pick_checkout(102);
    assert_eq!(ComposerPage::of(&rail, &draft).checkout, Some(102));
    draft.pick_checkout(110);
    assert_eq!(ComposerPage::of(&rail, &draft).checkout, Some(100), "a draft for a checkout this repository does not list is ignored");
}

#[test]
fn the_checkout_options_name_the_branch_only_when_it_differs() {
    let page = ComposerPage::of(&rail(), &Draft::default());
    let labels: Vec<_> = page.checkouts.iter().map(|pick| pick.label.as_str()).collect();
    assert_eq!(labels, ["argus \u{b7} main", "desktop \u{b7} desktop-design", "fix"]);
}

#[test]
fn the_place_names_the_repository_for_the_main_checkout_and_the_worktree_by_itself() {
    let mut draft = Draft::default();
    assert_eq!(ComposerPage::of(&rail(), &draft).place, Where::Starting { worktree: false, name: "argus".into(), branch: Some("main".into()) });
    draft.pick_checkout(101);
    assert_eq!(ComposerPage::of(&rail(), &draft).place, Where::Starting { worktree: true, name: "desktop".into(), branch: Some("desktop-design".into()) });
}

#[test]
fn the_agent_is_the_drafts_while_offered_else_the_first() {
    let mut rail = rail();
    let mut draft = Draft::default();
    assert_eq!(ComposerPage::of(&rail, &draft).template.as_deref(), Some("claude"));
    draft.pick_template("codex");
    assert_eq!(ComposerPage::of(&rail, &draft).template.as_deref(), Some("codex"));
    rail.templates = vec!["claude".into()];
    assert_eq!(ComposerPage::of(&rail, &draft).template.as_deref(), Some("claude"), "no longer offered");
    rail.templates.clear();
    let page = ComposerPage::of(&rail, &draft);
    assert!(page.can_shell && !page.can_start, "a shell needs no agent");
}

#[test]
fn choosing_a_repository_clears_the_checkout_and_asks_the_rail_to_follow_only_when_it_differs() {
    let rail = rail();
    let mut draft = Draft { checkout: Some(101), ..Draft::default() };
    assert!(draft.pick_repository(&rail, 10).is_none());
    assert_eq!(draft.checkout, None);
    draft.checkout = Some(101);
    assert!(matches!(draft.pick_repository(&rail, 11), Some(Call::ChooseRepository { repository: 11 })));
    assert_eq!(draft.checkout, None);
}

#[test]
fn an_agent_starts_with_the_message_untrimmed_or_none_when_it_is_blank() {
    let rail = rail();
    let mut draft = Draft { text: "  fix it\n".into(), ..Draft::default() };
    match draft.start(&rail, true) {
        Some(Call::Compose { checkout: 100, template: Some(template), prompt: Some(prompt) }) => {
            assert_eq!((template.as_str(), prompt.as_str()), ("claude", "  fix it\n"));
        }
        other => panic!("{other:?}"),
    }
    draft.text = " \n ".into();
    assert!(matches!(draft.start(&rail, true), Some(Call::Compose { prompt: None, template: Some(_), .. })), "idle");
}

#[test]
fn a_shell_ignores_the_message_and_the_agent() {
    let rail = rail();
    let draft = Draft { text: "ignored".into(), ..Draft::default() };
    assert!(matches!(draft.start(&rail, false), Some(Call::Compose { checkout: 100, template: None, prompt: None })));
}

#[test]
fn nothing_starts_without_a_checkout_and_an_agent_not_without_a_template() {
    let mut rail = rail();
    rail.templates.clear();
    assert!(Draft::default().start(&rail, true).is_none());
    assert!(Draft::default().start(&rail, false).is_some());
    rail.repository = Some(12);
    assert!(Draft::default().start(&rail, false).is_none());
}

#[test]
fn the_message_is_cleared_when_a_start_made_a_pane_and_the_picks_stay() {
    let before = rail();
    let mut draft = Draft { checkout: Some(101), template: Some("codex".into()), text: "hello".into() };
    let mut other = before.clone();
    other.shown = Some(shown(1, "argus", "argus"));
    draft.settle(&before, &other);
    assert_eq!(draft.text, "hello", "the stage is still the composer");
    let mut made = other.clone();
    made.stage = Stage::Workspace;
    draft.settle(&before, &made);
    assert_eq!(draft.text, "", "a pane newly shown as the stage went to the workspace");
    assert_eq!((draft.checkout, draft.template.as_deref()), (Some(101), Some("codex")));
}

#[test]
fn going_back_to_the_workspace_without_a_new_pane_keeps_the_message() {
    let mut before = rail();
    before.shown = Some(shown(1, "argus", "argus"));
    let mut draft = Draft { text: "keep".into(), ..Draft::default() };
    let mut after = before.clone();
    after.stage = Stage::Workspace;
    draft.settle(&before, &after);
    assert_eq!(draft.text, "keep", "the pane on screen is the one that was");
}
