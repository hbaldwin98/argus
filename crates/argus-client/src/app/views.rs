//! The client's top-level views, and the fetches opening one makes.
//!
//! The Workspace stage — the selected pane's terminal — was for a long time
//! the only thing the content area could hold. A feature is read at project
//! scope, all at once, and says nothing useful in a strip beside a pane, so
//! it wants the stage rather than a corner of it (TARGET.md, "Product
//! boundary").
//!
//! What a view replaces is the screen, never the running work: every pane
//! keeps running while another view is up, and the Workspace stage is one
//! keystroke back. Which view is open is this client's business and is not sent to
//! the daemon — two people attached to one daemon are not necessarily
//! reading the same thing.

use super::*;

/// Which top-level surface the content area is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Workspace,
    /// The selected repository branch's features, and whichever one is selected read whole:
    /// its brief, what is left to do under it, and why it has the shape it
    /// does.
    Feature,
    /// Every live pane in the workspace, arranged for scanning rather than
    /// terminal input.
    Panes,
    /// The selected repository's checkouts and worktrees as an operational
    /// table.
    Checkouts,
}

impl View {
    /// Every top-level surface, in the order shown by the design document.
    pub const ALL: [View; 4] = [View::Workspace, View::Feature, View::Panes, View::Checkouts];

    pub fn label(self) -> &'static str {
        match self {
            View::Workspace => "workspace",
            View::Feature => "feature",
            View::Panes => "panes",
            View::Checkouts => "checkouts",
        }
    }

    /// The digit that opens this view, which is also its place in the
    /// strip. Numbered rather than cycled because a cycle key makes the
    /// second view cheap to reach and the fourth expensive, and because a
    /// tab strip that shows numbers teaches its own bindings.
    pub fn digit(self) -> char {
        let index = View::ALL.iter().position(|v| *v == self).unwrap_or(0);
        char::from_digit(index as u32 + 1, 10).unwrap_or('1')
    }

    pub fn from_digit(c: char) -> Option<View> {
        View::ALL.into_iter().find(|view| view.digit() == c)
    }
}

impl App {
    /// Opens a view, remembering where focus was in the workspace so coming
    /// back does not cost you your place.
    ///
    /// Focus has to move: leaving it in a pane would send every key to the
    /// child of a pane that is no longer on screen.
    pub fn open_view(&mut self, view: View) {
        self.open_view_with_origin(view, false);
    }

    /// Opens a view from the tab strip. A tab is navigation chrome: it
    /// must never leave keyboard focus in a live pane, including when the
    /// workspace tab is pressed while the workspace is already showing.
    pub(crate) fn open_view_from_tab(&mut self, view: View) {
        self.open_view_with_origin(view, true);
    }

    fn open_view_with_origin(&mut self, view: View, from_tab: bool) {
        if self.view == view && !from_tab {
            return;
        }
        if self.view == View::Workspace && view != View::Workspace {
            self.workspace_focus = self.focus;
        }
        let changed = self.view != view;
        if changed {
            self.view = view;
        }
        self.focus = match view {
            View::Workspace => {
                let mut focus = if from_tab
                    && self.view == View::Workspace
                    && matches!(self.focus, Focus::PaneContent)
                {
                    Focus::Panes
                } else {
                    self.workspace_focus
                };
                if from_tab && matches!(focus, Focus::PaneContent) {
                    focus = Focus::Panes;
                }
                if from_tab {
                    self.workspace_focus = focus;
                }
                focus
            }
            _ => Focus::View,
        };
        self.leader_pending = false;
        // A client that attached after the last write has never been
        // pushed a board, so opening the view is a fetch.
        if changed && view == View::Feature {
            self.ask_for_decisions();
            self.ask_for_tasks();
        }
        if changed || from_tab {
            self.report(view.label());
        }
    }

    /// Asks for the board of the project the rail is on. Sent on opening
    /// the view and on `r`, because a client that attached after the last
    /// write has never been pushed one.
    pub(super) fn ask_for_decisions(&mut self) {
        let (Some(project), Some(checkout)) = (
            self.current_project().map(|project| project.id),
            self.current_checkout().map(|checkout| checkout.id),
        ) else {
            self.board = None;
            self.board_checkout = None;
            return;
        };
        self.board_checkout = Some(checkout);
        let _ = self.out.send(ClientMsg::GetDecisions { project, checkout });
    }

    /// Asks again when the board on screen is not the one the view should
    /// be showing.
    ///
    /// Opening the view is a fetch, but the view can be open before there
    /// is anything to fetch for: the client draws before the first board
    /// arrives, and a workspace switch re-scopes the tree under whatever
    /// is open. Called on every tree, and cheap when nothing has moved —
    /// an adopted board carries the project's name, so the comparison
    /// fails exactly once per change.
    pub(super) fn refresh_board_if_stale(&mut self) {
        if self.view != View::Feature {
            return;
        }
        let Some(name) = self.current_project().map(|p| p.name.clone()) else {
            self.board = None;
            return;
        };
        let checkout = self.current_checkout().map(|c| c.id);
        if self.board.as_ref().map(|b| b.name.as_str()) != Some(name.as_str())
            || checkout != self.board_checkout
        {
            self.ask_for_decisions();
        }
    }
}

impl App {
    pub(super) fn ask_for_tasks(&mut self) {
        let (Some(project), Some(checkout), Some(feature)) = (
            self.board.as_ref().and_then(|b| b.project),
            self.current_checkout().map(|checkout| checkout.id),
            self.feature_slug(),
        ) else {
            self.tasks = None;
            return;
        };
        let _ = self.out.send(ClientMsg::Task {
            project,
            checkout,
            feature,
            action: argus_protocol::TaskAction::List,
        });
    }

    pub(super) fn ask_for_diagrams(&mut self) {
        let (Some(project), Some(checkout), Some(feature)) = (
            self.board.as_ref().and_then(|b| b.project),
            self.current_checkout().map(|checkout| checkout.id),
            self.feature_slug(),
        ) else {
            self.diagrams = None;
            return;
        };
        let _ = self.out.send(ClientMsg::SequenceDiagram {
            project,
            checkout,
            feature,
            action: argus_protocol::DiagramAction::List,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_views_digit_is_its_place_in_the_strip() {
        for view in View::ALL {
            assert_eq!(View::from_digit(view.digit()), Some(view));
        }
        assert_eq!(View::from_digit('1'), Some(View::Workspace));
    }

    #[test]
    fn a_digit_no_view_sits_on_opens_nothing() {
        assert_eq!(View::from_digit('0'), None);
        assert_eq!(View::from_digit('5'), None);
        assert_eq!(View::from_digit('x'), None);
    }
}
