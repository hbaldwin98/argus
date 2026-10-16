//! Which stage is up, and the stages a page opened over, so that closing or
//! docking it goes back there: an agent's view popped out, an extension's
//! view moved to a page, or the Code stage opened from another.
//!
//! The session does the going and the rest of what a stage means; this
//! only says where the person is and where they return to. A page is never
//! a stage of its own choosing: it is opened over whatever stage was up,
//! and the person is returned to that stage, the Workspace if it is not
//! known. The console's fullscreen is the console's own state and stays in
//! `console`.

use crate::page::Stage;

/// Where the window is, and where each way out of a page leads back to.
pub struct Stages {
    now: Stage,
    /// The stage the Code stage was opened over, which Escape goes back to.
    before_code: Stage,
    pub over: PageOver,
}

impl Default for Stages {
    fn default() -> Self {
        Stages { now: Stage::Workspace, before_code: Stage::Workspace, over: PageOver::default() }
    }
}

impl Stages {
    pub fn now(&self) -> Stage {
        self.now
    }

    /// Puts `stage` up, and whichever was up away.
    pub fn go(&mut self, stage: Stage) {
        self.now = stage;
    }

    /// The Code stage opens over whichever stage is up, unless it already is.
    pub fn enter_code(&mut self) {
        if self.now != Stage::Code {
            self.before_code = self.now;
        }
        self.go(Stage::Code);
    }

    /// Leaves the Code stage for the one it was opened over.
    pub fn leave_code(&mut self) {
        self.go(self.before_code);
    }

    /// Whether the Workspace's panes are drawn: only while its stage is up
    /// and no fullscreen console covers it. Every other stage covers them,
    /// the composer's too.
    pub fn shows_workspace(&self, console_fullscreen: bool) -> bool {
        self.now == Stage::Workspace && !console_fullscreen
    }
}

#[derive(Default)]
pub struct PageOver {
    before: Option<Stage>,
}

/// What a popped-out view and the stage together call for.
#[derive(Debug, PartialEq, Eq)]
pub enum Settled {
    /// The view docked, closed or was left for another stage: it is no
    /// page, so the window's hold on it ends.
    EndPopOut,
    /// The view stage is up with no view on it: back to this stage.
    Back(Stage),
}

impl PageOver {
    /// An agent's view opened while `now` is up: the stage to go to, if not
    /// there already. An extension's page, which it replaces, stays what
    /// the stage under it is.
    pub fn open_view(&mut self, now: Stage) -> Option<Stage> {
        if now == Stage::View {
            return None;
        }
        if now != Stage::Extension {
            self.before = Some(now);
        }
        Some(Stage::View)
    }

    /// An extension's page opened while `now` is up. An agent's view's
    /// page, which it replaces, stays what the stage under it is.
    pub fn open_extension(&mut self, now: Stage) -> Option<Stage> {
        if now == Stage::Extension {
            return None;
        }
        if now != Stage::View {
            self.before = Some(now);
        }
        Some(Stage::Extension)
    }

    /// An extension's page ended while `now` is up: where it goes back to.
    pub fn leave_extension(&mut self, now: Stage) -> Option<Stage> {
        (now == Stage::Extension).then(|| self.back())
    }

    /// Keeps the stage level with the view: one docked, closed, or left for
    /// another stage is no longer a page, and the view stage is never up
    /// with no view on it.
    pub fn settle_view(&mut self, popped: bool, now: Stage) -> Option<Settled> {
        match (popped, now == Stage::View) {
            (true, false) => Some(Settled::EndPopOut),
            (false, true) => Some(Settled::Back(self.back())),
            _ => None,
        }
    }

    fn back(&mut self) -> Stage {
        self.before.take().unwrap_or(Stage::Workspace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_goes_back_to_the_stage_it_opened_over_or_the_workspace() {
        let mut over = PageOver::default();
        assert_eq!(over.open_view(Stage::Feature), Some(Stage::View));
        assert_eq!(over.open_view(Stage::View), None, "already up");
        assert_eq!(over.settle_view(false, Stage::View), Some(Settled::Back(Stage::Feature)));
        assert_eq!(over.settle_view(false, Stage::View), Some(Settled::Back(Stage::Workspace)), "once only");
    }

    #[test]
    fn a_view_opened_over_an_extensions_page_leaves_what_is_under_that_page_alone() {
        let mut over = PageOver::default();
        assert_eq!(over.open_extension(Stage::Checkouts), Some(Stage::Extension));
        assert_eq!(over.open_view(Stage::Extension), Some(Stage::View));
        assert_eq!(over.settle_view(false, Stage::View), Some(Settled::Back(Stage::Checkouts)));
    }

    #[test]
    fn an_extensions_page_opened_over_a_views_page_leaves_what_is_under_that_page_alone() {
        let mut over = PageOver::default();
        assert_eq!(over.open_view(Stage::Checkouts), Some(Stage::View));
        assert_eq!(over.open_extension(Stage::View), Some(Stage::Extension));
        assert_eq!(over.leave_extension(Stage::Extension), Some(Stage::Checkouts));
    }

    #[test]
    fn a_view_popped_out_that_the_person_left_for_another_stage_is_no_page() {
        let mut over = PageOver::default();
        assert_eq!(over.settle_view(true, Stage::Feature), Some(Settled::EndPopOut));
        assert_eq!(over.settle_view(true, Stage::View), None);
        assert_eq!(over.settle_view(false, Stage::Feature), None);
    }

    #[test]
    fn an_extension_page_ending_goes_back_only_while_it_is_the_stage() {
        let mut over = PageOver::default();
        over.open_extension(Stage::Console);
        assert_eq!(over.leave_extension(Stage::Workspace), None);
        assert_eq!(over.leave_extension(Stage::Extension), Some(Stage::Console));
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
    fn the_workspace_is_drawn_only_while_its_stage_is_up_and_no_fullscreen_console_covers_it() {
        let mut stages = Stages::default();
        assert!(stages.shows_workspace(false));
        assert!(!stages.shows_workspace(true), "a fullscreen console covers it");
        for stage in [Stage::Feature, Stage::Checkouts, Stage::Composer, Stage::Console, Stage::Code, Stage::View, Stage::Extension] {
            stages.go(stage);
            assert!(!stages.shows_workspace(false), "{stage:?} covers it");
        }
    }
}
