//! What fills the stage: one of the stages, or a page — an agent's view or
//! an extension's — opened over one; where closing, docking or Escape goes
//! back to; and whether the Workspace's panes are drawn under it.
//!
//! A page is never a stage of its own choosing. It opens over whatever stage
//! is up, a page up already giving way to it and leaving what was under that
//! one alone; closing it, docking it or its view going returns the person to
//! the stage under it, and going to any other stage closes it. The Code stage
//! goes back to the stage it was opened over. Whether a panel opens as a page
//! is the person's choice, kept by `docks`; the console's fullscreen is the
//! console's own state, kept by `panes::console`.

use argus_protocol::PaneId;

use crate::layout::docks::Panel;
use crate::connection::page::Stage;

/// A view open as a page, filling the stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    /// An agent's view, by the pane that showed it and its id.
    View { pane: PaneId, id: String },
    /// An extension's view, by its panel.
    Extension(Panel),
}

impl Page {
    fn stage(&self) -> Stage {
        match self {
            Page::View { .. } => Stage::View,
            Page::Extension(_) => Stage::Extension,
        }
    }

    /// What is said when the page's view goes from under it.
    fn gone(&self, title: &str) -> String {
        match self {
            Page::View { .. } => format!("{title} was closed"),
            Page::Extension(panel) => {
                let (extension, _) = panel.extension_view().unwrap_or_default();
                format!("{extension}: {title} is no longer shown here")
            }
        }
    }
}

/// The page up, and the title the person last saw it by, to say what went.
struct Paged {
    page: Page,
    title: String,
}

#[derive(Default)]
pub struct Stages {
    /// The stage up, or the one under the page while a page is: never a
    /// page's own stage, so closing a page always has somewhere to go.
    stage: Stage,
    page: Option<Paged>,
    /// The stage the Code stage was opened over, which Escape goes back to.
    before_code: Stage,
}

impl Stages {
    /// The stage the page draws: a page's own while one is up.
    pub fn now(&self) -> Stage {
        self.page.as_ref().map_or(self.stage, |p| p.page.stage())
    }

    /// Puts `stage` up, closing the page. A page's stage is opened with
    /// `open`, never gone to.
    pub fn go(&mut self, stage: Stage) {
        debug_assert!(!matches!(stage, Stage::Extension | Stage::View), "a page is opened, never gone to");
        self.page = None;
        self.stage = match stage {
            Stage::Extension | Stage::View => Stage::Workspace,
            stage => stage,
        };
    }

    /// The Code stage over whichever stage is up — under a page, the one
    /// the page opened over — unless it is up already.
    pub fn enter_code(&mut self) {
        if self.stage != Stage::Code {
            self.before_code = self.stage;
        }
        self.go(Stage::Code);
    }

    /// Leaves the Code stage for the one it was opened over.
    pub fn leave_code(&mut self) {
        self.go(self.before_code);
    }

    /// Opens `page` over the stage up, in place of any page up already,
    /// known by `title` until its view says otherwise.
    pub fn open(&mut self, page: Page, title: String) {
        self.page = Some(Paged { page, title });
    }

    /// The extension's view up as the page, if one is.
    pub fn extension_page(&self) -> Option<&Panel> {
        match self.page.as_ref().map(|p| &p.page) {
            Some(Page::Extension(panel)) => Some(panel),
            _ => None,
        }
    }

    /// The agent's view up as the page, if one is.
    pub fn view_page(&self) -> Option<(PaneId, &str)> {
        match self.page.as_ref().map(|p| &p.page) {
            Some(Page::View { pane, id }) => Some((*pane, id)),
            _ => None,
        }
    }

    /// Closes the page, docked or put back by the person: back to the
    /// stage under it, nothing said. Nothing to close is no change.
    pub fn close(&mut self) {
        self.page = None;
    }

    /// Keeps the page level with what the window holds. `held` gives the
    /// title of the page's view while the window holds it where the person
    /// is; a page whose view went closes, and the sentence saying so comes
    /// back.
    pub fn settle(&mut self, held: impl FnOnce(&Page) -> Option<String>) -> Option<String> {
        let paged = self.page.as_mut()?;
        match held(&paged.page) {
            Some(title) => {
                paged.title = title;
                None
            }
            None => self.page.take().map(|Paged { page, title }| page.gone(&title)),
        }
    }

    /// Whether the Workspace's panes are drawn: only while its stage is up
    /// and no fullscreen console covers it. Every other stage covers them,
    /// the composer's too.
    pub fn shows_workspace(&self, console_fullscreen: bool) -> bool {
        self.now() == Stage::Workspace && !console_fullscreen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ci() -> Page {
        Page::View { pane: PaneId(1), id: "ci".into() }
    }

    fn containers() -> Page {
        Page::Extension(Panel::extension("docker", "containers"))
    }

    #[test]
    fn a_page_goes_back_to_the_stage_it_opened_over() {
        let mut stages = Stages::default();
        stages.go(Stage::Feature);
        stages.open(ci(), "CI".into());
        assert_eq!(stages.now(), Stage::View);
        stages.close();
        assert_eq!(stages.now(), Stage::Feature);
        stages.close();
        assert_eq!(stages.now(), Stage::Feature, "nothing to close is no change");
    }

    #[test]
    fn a_page_opened_over_a_page_takes_its_place_and_leaves_what_is_under_it_alone() {
        for (first, second) in [(containers(), ci()), (ci(), containers())] {
            let mut stages = Stages::default();
            stages.go(Stage::Checkouts);
            stages.open(first, "first".into());
            stages.open(second.clone(), "second".into());
            assert_eq!(stages.now(), second.stage());
            stages.close();
            assert_eq!(stages.now(), Stage::Checkouts, "{second:?} over the other");
        }
    }

    #[test]
    fn going_to_another_stage_closes_the_page() {
        let mut stages = Stages::default();
        stages.open(ci(), "CI".into());
        stages.go(Stage::Feature);
        assert_eq!((stages.view_page(), stages.now()), (None, Stage::Feature));
        stages.close();
        assert_eq!(stages.now(), Stage::Feature);
    }

    #[test]
    fn a_page_whose_view_goes_says_so_by_the_title_last_seen_and_goes_back() {
        let mut stages = Stages::default();
        stages.go(Stage::Console);
        stages.open(containers(), "Containers".into());
        assert_eq!(stages.settle(|_| Some("Running".into())), None);
        assert_eq!(stages.extension_page(), Some(&Panel::extension("docker", "containers")));
        assert_eq!(stages.settle(|_| None).as_deref(), Some("docker: Running is no longer shown here"));
        assert_eq!(stages.now(), Stage::Console);

        stages.open(ci(), "CI".into());
        assert_eq!(stages.settle(|_| None).as_deref(), Some("CI was closed"));
        assert_eq!(stages.settle(|_| panic!("no page to ask about")), None);
    }

    #[test]
    fn the_code_stage_goes_back_to_the_stage_it_was_opened_over() {
        let mut stages = Stages::default();
        stages.go(Stage::Checkouts);
        stages.enter_code();
        stages.enter_code();
        assert_eq!(stages.now(), Stage::Code);
        stages.leave_code();
        assert_eq!(stages.now(), Stage::Checkouts, "re-entering does not forget what it was over");
    }

    #[test]
    fn the_code_stage_entered_from_a_page_goes_back_to_what_the_page_opened_over() {
        let mut stages = Stages::default();
        stages.go(Stage::Checkouts);
        stages.open(ci(), "CI".into());
        stages.enter_code();
        assert_eq!(stages.view_page(), None);
        stages.leave_code();
        assert_eq!(stages.now(), Stage::Checkouts);
    }

    #[test]
    fn code_entered_from_a_page_over_code_keeps_what_code_was_opened_over() {
        let mut stages = Stages::default();
        stages.go(Stage::Checkouts);
        stages.enter_code();
        stages.open(containers(), "Containers".into());
        stages.enter_code();
        assert_eq!(stages.now(), Stage::Code);
        stages.leave_code();
        assert_eq!(stages.now(), Stage::Checkouts);
    }

    #[test]
    fn the_workspace_is_drawn_only_while_its_stage_is_up_and_no_fullscreen_console_covers_it() {
        let mut stages = Stages::default();
        assert!(stages.shows_workspace(false));
        assert!(!stages.shows_workspace(true), "a fullscreen console covers it");
        for stage in [Stage::Feature, Stage::Checkouts, Stage::Composer, Stage::Console, Stage::Code] {
            stages.go(stage);
            assert!(!stages.shows_workspace(false), "{stage:?} covers it");
        }
        stages.go(Stage::Workspace);
        for page in [ci(), containers()] {
            stages.open(page.clone(), "page".into());
            assert!(!stages.shows_workspace(false), "{page:?} covers it");
        }
    }
}
