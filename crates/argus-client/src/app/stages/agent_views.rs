//! The views agents showed, as this client holds them, which one is open,
//! and how far it is scrolled.
//!
//! The daemon owns the views. Closing one or asking for a refresh is a
//! request, and nothing here changes until the daemon says it did: a view
//! an agent never answers stays as it was read. Only which view is open and
//! where it is scrolled are this client's own.

use argus_protocol::{ClientMsg, PaneId, ShownView};

use crate::app::*;

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
    /// scroll stays only when the view being read is still the same (pane,
    /// id) — a refresh. A newer view taking the default-open place is a
    /// different page, and reading it from the middle would be wrong.
    pub fn shown(&mut self, view: ShownView) {
        let key = (view.pane, view.view.id.clone());
        let before = self.opened().map(|v| (v.pane, v.view.id.clone()));
        self.views
            .retain(|v| (v.pane, v.view.id.as_str()) != (key.0, key.1.as_str()));
        self.views.insert(0, view);
        if self.opened().map(|v| (v.pane, v.view.id.clone())) != before {
            self.scroll = 0;
        }
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

    pub fn choose(&mut self, index: usize) {
        let Some(view) = self.views.get(index) else { return };
        let key = (view.pane, view.view.id.clone());
        if self.open_index() != Some(index) {
            self.scroll = 0;
        }
        self.open = Some(key);
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
        if self.saved_views.is_open() {
            // A saved view is a record, not something on screen to dismiss.
            self.report("x closes a shown view; a saved view stays");
            return;
        }
        let Some(shown) = self.agent_views.opened() else {
            return;
        };
        let _ = self.out.send(ClientMsg::CloseView {
            pane: shown.pane,
            id: shown.view.id.clone(),
        });
    }

    /// The stage's items are the live views, then the saved ones, so the
    /// list, its clicks and its keys share one count.
    pub(crate) fn stage_len(&self) -> usize {
        self.agent_views.all().len() + self.saved_views.all().len()
    }

    /// The item the stage shows, in [`App::stage_len`]'s count.
    pub(crate) fn stage_open(&self) -> Option<usize> {
        if self.saved_views.is_open() {
            return self
                .saved_views
                .open_index()
                .map(|at| self.agent_views.all().len() + at);
        }
        self.agent_views.open_index()
    }

    pub(crate) fn choose_stage(&mut self, index: usize) {
        let live = self.agent_views.all().len();
        if index < live {
            if self.saved_views.is_open() {
                // The live view's scroll was left where it was when the saved view took the stage.
                self.agent_views.scroll = 0;
            }
            self.saved_views.close();
            self.agent_views.choose(index);
        } else {
            self.saved_views.choose(index - live);
        }
    }

    /// Moves to the item `delta` places along the stage's list. A saved
    /// view that is open but gone is let go, so the walk does not stop at it.
    pub(crate) fn step_stage(&mut self, delta: i32) {
        if self.saved_views.is_open() && self.saved_views.open_index().is_none() {
            self.saved_views.close();
        }
        let Some(at) = self.stage_open() else { return };
        let last = self.stage_len().saturating_sub(1) as i32;
        self.choose_stage((at as i32 + delta).clamp(0, last) as usize);
    }

    /// The scroll and page of whichever the stage shows: a saved view's own
    /// when one is open, a live view's otherwise.
    fn stage_scroll_mut(&mut self) -> &mut usize {
        if self.saved_views.is_open() {
            &mut self.saved_views.scroll
        } else {
            &mut self.agent_views.scroll
        }
    }

    pub(crate) fn stage_scroll(&self) -> usize {
        if self.saved_views.is_open() {
            self.saved_views.scroll
        } else {
            self.agent_views.scroll
        }
    }

    fn stage_page(&self) -> usize {
        if self.saved_views.is_open() {
            self.saved_views.page
        } else {
            self.agent_views.page
        }
    }

    /// Records what the last frame showed of the open view, which clamps
    /// the scroll and sets how far a page goes.
    pub(crate) fn set_stage_window(&mut self, top: usize, visible: usize) {
        *self.stage_scroll_mut() = top;
        if self.saved_views.is_open() {
            self.saved_views.page = visible;
        } else {
            self.agent_views.page = visible;
        }
    }

    pub(crate) fn scroll_stage_by(&mut self, lines: i32) {
        let at = self.stage_scroll_mut();
        *at = (*at as i64 + i64::from(lines)).max(0) as usize;
    }

    pub(crate) fn page_stage_by(&mut self, pages: i32) {
        let page = self.stage_page().saturating_sub(1).max(1) as i32;
        self.scroll_stage_by(pages * page);
    }

    pub(crate) fn top_stage(&mut self) {
        *self.stage_scroll_mut() = 0;
    }

    pub(crate) fn bottom_stage(&mut self) {
        *self.stage_scroll_mut() = usize::MAX / 2;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn view(pane: u64, id: &str) -> ShownView {
        ShownView {
            pane: PaneId(pane),
            at: 0,
            subject: None,
            view: serde_json::from_value(serde_json::json!({ "id": id, "title": id, "blocks": [] })).unwrap(),
        }
    }

    #[test]
    fn a_refresh_of_the_open_view_keeps_its_scroll() {
        let mut views = AgentViews::default();
        views.shown(view(1, "run"));
        views.scroll = 12;
        views.shown(view(1, "run"));
        assert_eq!(views.scroll, 12);
    }

    #[test]
    fn a_newer_view_taking_the_default_place_starts_at_the_top() {
        let mut views = AgentViews::default();
        views.shown(view(1, "run"));
        views.scroll = 12;
        views.shown(view(1, "ticket"));
        assert_eq!(views.opened().unwrap().view.id, "ticket");
        assert_eq!(views.scroll, 0);
    }

    #[test]
    fn a_newer_view_does_not_move_a_view_chosen_to_read() {
        let mut views = AgentViews::default();
        views.shown(view(1, "run"));
        views.shown(view(1, "ticket"));
        views.choose(1);
        views.scroll = 12;
        views.shown(view(2, "other"));
        assert_eq!(views.opened().unwrap().view.id, "run");
        assert_eq!(views.scroll, 12);
    }
}
