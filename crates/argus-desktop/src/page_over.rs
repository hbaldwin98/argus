//! Which stage a page opened over, so that closing or docking it goes back
//! there: an agent's view popped out, or an extension's view moved to a page.
//!
//! Neither page is a stage of its own choosing. Each is opened over whatever
//! stage was up, and the person is returned to that stage, the Workspace if
//! it is not known. The session does the going; this only says where.

use crate::page::Stage;

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

    /// An extension's page opened while `now` is up.
    pub fn open_extension(&mut self, now: Stage) -> Option<Stage> {
        if now == Stage::Extension {
            return None;
        }
        self.before = Some(now);
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
}
