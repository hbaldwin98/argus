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

use crate::app::*;

/// Which top-level surface the content area is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Workspace,
    /// The selected repository branch's features, and whichever one is selected read whole:
    /// its brief, what is left to do under it, and why it has the shape it
    /// does.
    Feature,
    /// The selected repository's checkouts and worktrees as an operational
    /// table.
    Checkouts,
    /// What agents showed the person, drawn as text. The stage sits beside
    /// the rail like the others; the richer blocks are the desktop's.
    AgentViews,
}

impl View {
    /// Every top-level surface, in the order shown by the design document.
    pub const ALL: [View; 4] = [
        View::Workspace,
        View::Feature,
        View::Checkouts,
        View::AgentViews,
    ];

    pub fn label(self) -> &'static str {
        match self {
            View::Workspace => "workspace",
            View::Feature => "feature",
            View::Checkouts => "checkouts",
            View::AgentViews => "views",
        }
    }

    /// The digit that opens this view, which is also its place in the
    /// strip. Numbered rather than cycled because a cycle key makes the
    /// second view cheap to reach and the last expensive, and because a
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
        // pushed a board, so opening the view follows one.
        if changed && view == View::Feature {
            self.follow_board();
        }
        if changed || from_tab {
            self.report(view.label());
        }
    }

    /// Follows the board of the repository the rail is on while the
    /// Feature view is open.
    ///
    /// Called on every tree, key and click, because any of them can move
    /// the rail, and cheap when nothing has moved: the follower asks once
    /// per repository per connection, and pushes keep it current between.
    pub(in crate::app) fn follow_board(&mut self) {
        if self.view != View::Feature {
            return;
        }
        let scope = self.board_scope();
        let mut out = Vec::new();
        if self.board_follower.follow(scope, &mut out) {
            self.rescope_feature();
        }
        self.send_board_requests(out);
    }

    /// The repository the rail is on, named by the checkout under the
    /// cursor, or by its primary when the cursor is on a branch: the board
    /// is the repository's whichever checkout asks.
    fn board_scope(&self) -> Option<argus_protocol::BoardScope> {
        let project = self.current_project()?;
        let repository = self.current_repository()?;
        let checkout = self.current_checkout().or_else(|| {
            let primary = repository.checkouts.iter().find(|checkout| checkout.primary);
            primary.or_else(|| repository.checkouts.first())
        })?;
        Some(argus_protocol::BoardScope {
            project: project.id,
            project_name: project.name.clone(),
            repository: repository.id,
            checkout: checkout.id,
        })
    }

    pub(in crate::app) fn send_board_requests(&self, out: Vec<ClientMsg>) {
        for msg in out {
            let _ = self.out.send(msg);
        }
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
        assert_eq!(View::from_digit('3'), Some(View::Checkouts));
        assert_eq!(View::from_digit('4'), Some(View::AgentViews));
    }

    #[test]
    fn a_digit_no_view_sits_on_opens_nothing() {
        assert_eq!(View::from_digit('0'), None);
        assert_eq!(View::from_digit('5'), None);
        assert_eq!(View::from_digit('x'), None);
    }
}
