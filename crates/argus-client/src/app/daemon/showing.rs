//! What this client tells the daemon it is showing, which is how the
//! daemon can say where the person is working (`argus focus`).
//!
//! Sent by comparing with what was last sent rather than from each place
//! the selection moves: it moves from nav, mouse, input and every tree that
//! arrives, and a report remembered at each of them is one somebody
//! forgets.

use argus_protocol::View;

use crate::app::*;

impl App {
    /// The view the selection names now. Off screen it names nothing: the
    /// person is looking at another host, and this one's daemon should not
    /// say they are here.
    pub(crate) fn view(&self) -> View {
        if !self.on_screen {
            return View::default();
        }
        View {
            project: self.current_project().map(|p| p.id),
            checkout: self.current_checkout().map(|c| c.id),
            pane: self.current_pane().map(|p| p.id),
            active: self.terminal_focused,
        }
    }

    /// Tells the daemon what this client shows, when that is not what it
    /// was last told and it can take it.
    pub fn report_view(&mut self) {
        if !self.reports_view {
            return;
        }
        let view = self.view();
        if self.view_sent.as_ref() == Some(&view) {
            return;
        }
        let _ = self.out.send(ClientMsg::View(view.clone()));
        self.view_sent = Some(view);
    }

    /// Whether the terminal holds the person's focus. Coming back is said
    /// again even with the selection unchanged, so this client takes the
    /// focus back from whichever one the person was at meanwhile.
    pub fn set_terminal_focus(&mut self, focused: bool) {
        self.terminal_focused = focused;
        if focused {
            self.view_sent = None;
        }
        self.report_view();
    }
}
