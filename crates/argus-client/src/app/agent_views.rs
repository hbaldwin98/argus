//! The views agents showed, as this client holds them, which one is open,
//! and how far it is scrolled.
//!
//! The daemon owns the views. Closing one or asking for a refresh is a
//! request, and nothing here changes until the daemon says it did: a view
//! an agent never answers stays as it was read. Only which view is open and
//! where it is scrolled are this client's own.

use argus_protocol::{ClientMsg, PaneId, ShownView};

use super::*;

#[derive(Default)]
pub struct AgentViews {
    /// Newest first, as the daemon sends them.
    views: Vec<ShownView>,
    /// The open view by the pane that showed it and its id, so a newer view
    /// arriving in front of it does not change which one is open. The
    /// newest while none is chosen.
    open: Option<(PaneId, String)>,
    /// Lines scrolled off the top. Past the end is clamped by the frame
    /// that knows how tall the view is, which writes the answer back.
    pub scroll: usize,
    /// How many lines the view had room for in the last frame, for paging.
    pub page: usize,
}

impl AgentViews {
    /// A new connection replays every view the daemon holds, so what was
    /// held before it is dropped rather than doubled.
    pub fn connected(&mut self) {
        *self = AgentViews::default();
    }

    pub fn all(&self) -> &[ShownView] {
        &self.views
    }

    /// The open view's place in [`AgentViews::all`].
    pub fn open_index(&self) -> Option<usize> {
        let chosen = self.open.as_ref().and_then(|(pane, id)| {
            self.views
                .iter()
                .position(|v| v.pane == *pane && v.view.id == *id)
        });
        chosen.or(if self.views.is_empty() { None } else { Some(0) })
    }

    pub fn opened(&self) -> Option<&ShownView> {
        self.open_index().map(|index| &self.views[index])
    }

    /// A view came; one with the same pane and id is replaced by it. The
    /// scroll stays when it is the open view that was shown again, which is
    /// what a refresh is.
    pub fn shown(&mut self, view: ShownView) {
        let key = (view.pane, view.view.id.clone());
        self.views
            .retain(|v| (v.pane, v.view.id.as_str()) != (key.0, key.1.as_str()));
        self.views.insert(0, view);
    }

    pub fn closed(&mut self, pane: PaneId, id: &str) {
        let at = self.open_index();
        let was_open = self.opened().is_some_and(|v| v.pane == pane && v.view.id == id);
        self.views
            .retain(|v| (v.pane, v.view.id.as_str()) != (pane, id));
        if was_open {
            // The neighbour takes its place on screen.
            self.open = at
                .and_then(|at| self.views.get(at.min(self.views.len().saturating_sub(1))))
                .map(|v| (v.pane, v.view.id.clone()));
            self.scroll = 0;
        }
    }

    /// Opens the view `delta` places along the list, stopping at its ends.
    pub fn step(&mut self, delta: i32) {
        let Some(at) = self.open_index() else { return };
        let next = (at as i32 + delta).clamp(0, self.views.len() as i32 - 1) as usize;
        self.choose(next);
    }

    pub fn choose(&mut self, index: usize) {
        let Some(view) = self.views.get(index) else { return };
        let key = (view.pane, view.view.id.clone());
        if self.open_index() != Some(index) {
            self.scroll = 0;
        }
        self.open = Some(key);
    }

    pub fn scroll_by(&mut self, lines: i32) {
        self.scroll = (self.scroll as i64 + i64::from(lines)).max(0) as usize;
    }

    pub fn page_by(&mut self, pages: i32) {
        self.scroll_by(pages * self.page.saturating_sub(1).max(1) as i32);
    }

    pub fn scroll_to_end(&mut self) {
        self.scroll = usize::MAX / 2;
    }
}

impl App {
    /// Asks the agent that showed the open view to read it again. The view
    /// stays as it is until the agent shows it anew.
    pub(crate) fn refresh_open_view(&mut self) {
        let Some(shown) = self.agent_views.opened() else {
            return;
        };
        let _ = self.out.send(ClientMsg::RefreshView {
            pane: shown.pane,
            id: shown.view.id.clone(),
        });
        self.report("asked the agent to read it again");
    }

    /// Asks the daemon to close the open view; it goes when the daemon says
    /// it did.
    pub(crate) fn close_open_view(&mut self) {
        let Some(shown) = self.agent_views.opened() else {
            return;
        };
        let _ = self.out.send(ClientMsg::CloseView {
            pane: shown.pane,
            id: shown.view.id.clone(),
        });
    }

    /// The title of the pane that showed a view, if the tree still has it.
    pub(crate) fn pane_title(&self, pane: PaneId) -> Option<&str> {
        self.tree
            .iter()
            .flat_map(|p| &p.repositories)
            .flat_map(|r| &r.checkouts)
            .flat_map(|c| &c.panes)
            .find(|p| p.id == pane)
            .map(|p| p.title.as_str())
    }
}
