//! What the strip across the top of the stage says, as data a window draws and does not decide:
//! the crumb over the title, the status chip, the resumed note, and whether the pane's menu is
//! there (the page's `drawHead`).
//!
//! The words of a stage that has a view of its own (an open feature, an extension's page, a popped
//! view, the Code stage's trail) are that view's: it hands them in as [`Says`] and the strip puts
//! them where the page does, so no central switch learns every stage's words.

use crate::connection::page::{Rail, Resumed, Stage};
use crate::system::clock::Clock;

/// The statuses a pane's chip says even when the pane is not an agent.
const PANE_SAYS: [&str; 5] = ["waiting", "needs-review", "failed", "exited", "done"];

/// One part of the crumb, between slashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Text(String),
    /// A branch or a number, in the mono face.
    Mono(String),
}

/// What a stage's own view says of itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Says {
    /// In place of the stage's name: an open feature's title, the page's, a popped view's.
    pub title: Option<String>,
    /// After the repository: Code's crumb, or `Features / #12` for an open feature.
    pub trail: Vec<Part>,
    /// Beside the title, muted and small: the Checkouts stage's counts, whose last part is the main
    /// branch in mono. Empty for a stage that says nothing there.
    pub meta: Vec<Part>,
    /// What hovering the meta says.
    pub meta_tip: Option<&'static str>,
}

/// The pane on screen, which the menu under the strip's `...` is of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub pane: u64,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripPage {
    /// Over the title; empty when the stage has no crumb.
    pub crumb: Vec<Part>,
    pub title: String,
    /// The status word of a pane's chip: an agent's always, any other pane's when it is one
    /// `PANE_SAYS`. The page draws it only where the rail's own row is not seen.
    pub status: Option<&'static str>,
    /// The muted note of a pane this window resumed.
    pub resumed: Option<String>,
    /// What the stage's view says beside the title (`Says::meta`) and its tooltip.
    pub meta: Vec<Part>,
    pub meta_tip: Option<&'static str>,
    /// The pane the `...` menu is of, with a pane on screen.
    pub pane: Option<Named>,
}

impl StripPage {
    /// A strip that is only a title, such as a stage that has no crumb.
    pub fn titled(title: impl Into<String>) -> Self {
        Self { title: title.into(), ..Self::default() }
    }

    /// The strip for the stage up (`console_fullscreen` makes it the Console whatever is under),
    /// at `clock`'s moment.
    pub fn of(rail: &Rail, console_fullscreen: bool, says: &Says, clock: &Clock) -> Self {
        let stage = if console_fullscreen { Stage::Console } else { rail.stage };
        if stage == Stage::Workspace {
            return Self::pane(rail, clock);
        }
        let name = stage_name(stage);
        // The browsers are every project's agents', so no repository names them; nor the console or
        // the extensions.
        let repository = (!matches!(stage, Stage::Console | Stage::Extension | Stage::Browsers))
            .then(|| chosen_repository(rail))
            .flatten();
        let crumb = match repository {
            Some(repository) => std::iter::once(Part::Text(repository)).chain(says.trail.iter().cloned()).collect(),
            // Every stage has a crumb, so every title sits on one baseline.
            None if matches!(stage, Stage::Extension | Stage::Browsers) => vec![Part::Text(name.to_owned())],
            None => Vec::new(),
        };
        let title = says.title.clone().unwrap_or_else(|| if stage == Stage::Browsers { "Every agent" } else { name }.to_owned());
        Self { crumb, title, meta: says.meta.clone(), meta_tip: says.meta_tip, ..Self::default() }
    }

    fn pane(rail: &Rail, clock: &Clock) -> Self {
        let Some(shown) = &rail.shown else { return Self::default() };
        let branch = if shown.branch.is_empty() { &shown.checkout } else { &shown.branch };
        Self {
            crumb: vec![Part::Text(shown.repository.clone()), Part::Mono(branch.clone())],
            title: shown.title.clone(),
            status: (shown.agent || PANE_SAYS.contains(&shown.status)).then_some(shown.status),
            resumed: shown.resumed.as_ref().map(|resumed| resumed_from(resumed, clock)),
            pane: Some(Named { pane: shown.pane, title: shown.title.clone() }),
            ..Self::default()
        }
    }
}

/// The stage's name as the strip and the nav say it. The Workspace has none: its strip is the pane's.
pub fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Workspace => "",
        Stage::Feature => "Features",
        Stage::Checkouts => "Checkouts",
        Stage::Code => "Code",
        Stage::Composer => "New",
        Stage::Console => "Console",
        Stage::Extension => "Extensions",
        Stage::View => "View",
        Stage::Browsers => "Browsers",
    }
}

/// The repository the Feature and Checkouts stages read; none when none is chosen.
fn chosen_repository(rail: &Rail) -> Option<String> {
    let project = rail.project.as_ref()?;
    project.repositories.iter().find(|repository| Some(repository.id) == rail.repository).map(|repository| repository.name.clone())
}

/// What a pane this window resumed a session in was resumed from (SHELL-STAGE-9).
pub fn resumed_from(from: &Resumed, clock: &Clock) -> String {
    let at = clock.clock(from.at.as_deref());
    let turns = from.turns.map(|turns| format!("{turns} turn{} before", if turns == 1 { "" } else { "s" }));
    let said = at.map_or_else(|| "resumed".to_owned(), |at| format!("resumed from {at}"));
    match turns {
        Some(turns) => format!("{said} \u{b7} {turns}"),
        None => said,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::page::{Project, Repository, Shown};
    use jiff::tz::TimeZone;

    fn clock() -> Clock {
        Clock::new("2026-10-21T15:30:00Z".parse().unwrap(), TimeZone::UTC)
    }

    fn shown(status: &'static str, agent: bool) -> Shown {
        Shown {
            pane: 7,
            repository: "argus".into(),
            checkout: "main".into(),
            branch: "feature-x".into(),
            agent,
            browser: false,
            title: "Trim context".into(),
            status,
            resumed: None,
        }
    }

    fn repository(id: u64, name: &str) -> Repository {
        Repository { id, name: name.into(), quiet: false, divider: false, open: true, panes: 0, status: None, used: None, checkouts: Vec::new() }
    }

    fn rail(stage: Stage) -> Rail {
        let project = Project { id: 1, name: "src".into(), needs_you: 0, repositories: vec![repository(10, "dotfiles"), repository(11, "argus")], running: Vec::new() };
        Rail { project: Some(project), repository: Some(11), stage, ..Rail::default() }
    }

    fn text(part: &str) -> Part {
        Part::Text(part.into())
    }

    #[test]
    fn a_pane_on_the_workspace_is_a_crumb_of_its_repository_and_branch_over_its_title() {
        let mut rail = rail(Stage::Workspace);
        rail.shown = Some(shown("working", true));
        let strip = StripPage::of(&rail, false, &Says::default(), &clock());
        assert_eq!(strip.crumb, [text("argus"), Part::Mono("feature-x".into())]);
        assert_eq!(strip.title, "Trim context");
        assert_eq!(strip.pane, Some(Named { pane: 7, title: "Trim context".into() }));
    }

    #[test]
    fn a_checkout_with_no_branch_is_named_by_its_own_name() {
        let mut rail = rail(Stage::Workspace);
        rail.shown = Some(Shown { branch: String::new(), ..shown("idle", false) });
        assert_eq!(StripPage::of(&rail, false, &Says::default(), &clock()).crumb[1], Part::Mono("main".into()));
    }

    #[test]
    fn the_workspace_with_no_pane_has_an_empty_strip() {
        assert_eq!(StripPage::of(&rail(Stage::Workspace), false, &Says::default(), &clock()), StripPage::default());
    }

    #[test]
    fn an_agent_always_says_its_status_and_another_pane_only_a_status_that_needs_a_look() {
        let said = |status, agent| {
            let mut rail = rail(Stage::Workspace);
            rail.shown = Some(shown(status, agent));
            StripPage::of(&rail, false, &Says::default(), &clock()).status
        };
        assert_eq!(said("working", true), Some("working"));
        assert_eq!(said("working", false), None);
        assert_eq!(said("idle", false), None);
        for status in ["waiting", "needs-review", "failed", "exited", "done"] {
            assert_eq!(said(status, false), Some(status));
        }
    }

    #[test]
    fn a_resumed_pane_says_when_and_how_many_turns_before() {
        let said = |at: Option<&str>, turns| resumed_from(&Resumed { at: at.map(str::to_owned), turns }, &clock());
        assert_eq!(said(Some("2026-10-21T09:05:00Z"), Some(3)), "resumed from 09:05 today \u{b7} 3 turns before");
        assert_eq!(said(Some("2026-10-20T23:00:00Z"), Some(1)), "resumed from 23:00 yesterday \u{b7} 1 turn before");
        assert_eq!(said(None, None), "resumed", "no clock and no turns says only that");
        assert_eq!(said(Some("garbage"), Some(2)), "resumed \u{b7} 2 turns before");
        assert_eq!(said(Some("2026-10-21T09:05:00Z"), None), "resumed from 09:05 today");
    }

    #[test]
    fn the_strip_carries_the_resumed_note_of_the_pane() {
        let mut rail = rail(Stage::Workspace);
        rail.shown = Some(Shown { resumed: Some(Resumed { at: None, turns: Some(2) }), ..shown("idle", true) });
        assert_eq!(StripPage::of(&rail, false, &Says::default(), &clock()).resumed.as_deref(), Some("resumed \u{b7} 2 turns before"));
    }

    #[test]
    fn a_stage_is_named_under_the_repository_chosen() {
        for (stage, name) in [(Stage::Feature, "Features"), (Stage::Checkouts, "Checkouts"), (Stage::Code, "Code"), (Stage::Composer, "New"), (Stage::View, "View")] {
            let strip = StripPage::of(&rail(stage), false, &Says::default(), &clock());
            assert_eq!((strip.crumb.as_slice(), strip.title.as_str()), ([text("argus")].as_slice(), name));
            assert_eq!(strip.pane, None);
        }
    }

    #[test]
    fn a_stage_with_no_repository_chosen_has_no_crumb() {
        let mut rail = rail(Stage::Checkouts);
        rail.repository = None;
        assert!(StripPage::of(&rail, false, &Says::default(), &clock()).crumb.is_empty());
    }

    #[test]
    fn the_console_extensions_and_browsers_are_no_repositorys() {
        let console = StripPage::of(&rail(Stage::Console), false, &Says::default(), &clock());
        assert_eq!((console.crumb.is_empty(), console.title.as_str()), (true, "Console"));
        let extensions = StripPage::of(&rail(Stage::Extension), false, &Says::default(), &clock());
        assert_eq!((extensions.crumb, extensions.title.as_str()), (vec![text("Extensions")], "Extensions"));
        let browsers = StripPage::of(&rail(Stage::Browsers), false, &Says::default(), &clock());
        assert_eq!((browsers.crumb, browsers.title.as_str()), (vec![text("Browsers")], "Every agent"));
    }

    #[test]
    fn a_fullscreen_console_is_the_strips_stage_whatever_is_under_it() {
        let mut rail = rail(Stage::Workspace);
        rail.shown = Some(shown("working", true));
        let strip = StripPage::of(&rail, true, &Says::default(), &clock());
        assert_eq!((strip.title.as_str(), strip.pane), ("Console", None));
    }

    #[test]
    fn what_the_stages_view_says_takes_the_names_place_and_follows_the_repository() {
        let says = Says { title: Some("Add retries".into()), trail: vec![text("Features"), Part::Mono("#12".into())], ..Says::default() };
        let strip = StripPage::of(&rail(Stage::Feature), false, &says, &clock());
        assert_eq!(strip.title, "Add retries");
        assert_eq!(strip.crumb, [text("argus"), text("Features"), Part::Mono("#12".into())]);
        // Without a repository the trail is not drawn, as in the page.
        let strip = StripPage::of(&rail(Stage::Console), false, &says, &clock());
        assert!(strip.crumb.is_empty());
    }

    #[test]
    fn a_stages_meta_and_its_tooltip_ride_beside_the_title() {
        let says = Says { meta: vec![text("4 checkouts"), Part::Mono("main".into())], meta_tip: Some("Enter open"), ..Says::default() };
        let strip = StripPage::of(&rail(Stage::Checkouts), false, &says, &clock());
        assert_eq!((strip.meta, strip.meta_tip), (vec![text("4 checkouts"), Part::Mono("main".into())], Some("Enter open")));
        assert!(StripPage::of(&rail(Stage::Checkouts), false, &Says::default(), &clock()).meta.is_empty());
    }
}
