//! The saved views this client holds: the list the daemon sends and each
//! change to it, which one the stage shows, the links offered from it, and
//! the trail of links followed to reach it.
//!
//! As with shown views, the daemon owns the list, and nothing here changes
//! until it says so. Which view is open, where it is scrolled and the trail
//! are this client's own.

use argus_protocol::{linked_views, Block, ClientMsg, SavedView, MAX_EMBED_DEPTH};

use super::*;

#[derive(Default)]
pub struct SavedViews {
    /// In the order the daemon sent them, a new one at the end.
    views: Vec<SavedView>,
    /// The view the stage shows, by id. It stays when that view is gone, so
    /// the stage says it is not saved rather than falling back to a live view.
    open: Option<String>,
    /// The views left behind by following links, the last one nearest.
    /// Choosing a view from the list starts it over.
    trail: Vec<String>,
    /// While the list of links is up, the ids it offers and the one the
    /// cursor is on.
    pub links: Option<Links>,
    /// Lines scrolled off the top of the open view, and how many it had
    /// room for in the last frame.
    pub scroll: usize,
    pub page: usize,
}

pub struct Links {
    pub ids: Vec<String>,
    pub at: usize,
}

impl SavedViews {
    /// A new connection replays every saved view, so what was held is dropped.
    pub fn connected(&mut self) {
        *self = SavedViews::default();
    }

    pub fn all(&self) -> &[SavedView] {
        &self.views
    }

    pub fn get(&self, id: &str) -> Option<&SavedView> {
        self.views.iter().find(|v| v.id == id)
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn open_id(&self) -> Option<&str> {
        self.open.as_deref()
    }

    /// The open view's place in [`SavedViews::all`], or `None` when it is open
    /// but not saved.
    pub fn open_index(&self) -> Option<usize> {
        let id = self.open.as_deref()?;
        self.views.iter().position(|v| v.id == id)
    }

    pub fn opened(&self) -> Option<&SavedView> {
        self.open_index().map(|at| &self.views[at])
    }

    pub fn replaced(&mut self, all: Vec<SavedView>) {
        self.views = all;
    }

    /// A view saved or refreshed: replaces the one with its id in place.
    pub fn changed(&mut self, view: SavedView) {
        match self.views.iter_mut().find(|v| v.id == view.id) {
            Some(slot) => *slot = view,
            None => self.views.push(view),
        }
    }

    pub fn removed(&mut self, id: &str) {
        self.views.retain(|v| v.id != id);
    }

    /// Opens a view chosen from the list: a fresh start, so the trail goes.
    pub fn choose(&mut self, index: usize) {
        let Some(id) = self.views.get(index).map(|v| v.id.clone()) else {
            return;
        };
        self.trail.clear();
        self.show(id);
    }

    /// Opens a view a link named, leaving the one open behind on the trail.
    pub fn follow(&mut self, id: String) {
        if let Some(from) = self.open.clone() {
            self.trail.push(from);
        }
        self.show(id);
    }

    /// Reopens the view before the last link followed. `false` when there is
    /// none to reopen.
    pub fn back(&mut self) -> bool {
        match self.trail.pop() {
            Some(id) => {
                self.show(id);
                true
            }
            None => false,
        }
    }

    pub fn close(&mut self) {
        self.open = None;
        self.trail.clear();
        self.links = None;
        self.scroll = 0;
    }

    fn show(&mut self, id: String) {
        if self.open.as_deref() != Some(id.as_str()) {
            self.scroll = 0;
        }
        self.open = Some(id);
        self.links = None;
    }

    /// The blocks of a view with each embed drawn in place. The embedded
    /// view's own blocks are copied in here, not kept in the store, so a
    /// refresh of the embedded view shows at once.
    pub fn embedded(&self, id: &str, blocks: &[Block]) -> Vec<Block> {
        let mut blocks = blocks.to_vec();
        self.expand(&mut blocks, &mut vec![id.to_string()]);
        blocks
    }

    /// `chain` is the views being drawn around this list, the outermost first.
    /// A view already in it would draw itself forever; past the depth the
    /// contract follows, it is not drawn at all.
    fn expand(&self, blocks: &mut [Block], chain: &mut Vec<String>) {
        for block in blocks.iter_mut() {
            if let Block::Embed { view } = block {
                let id = view.clone();
                *block = self.embed(&id, chain);
                continue;
            }
            for (_, inner) in block.children_mut() {
                self.expand(inner, chain);
            }
        }
    }

    fn embed(&self, id: &str, chain: &mut Vec<String>) -> Block {
        if chain.iter().any(|c| c == id) {
            return note(format!("view {id} is already drawn above; not drawn again"));
        }
        if chain.len() > MAX_EMBED_DEPTH {
            return note(format!("view {id} is nested too deep to draw"));
        }
        let Some(view) = self.get(id) else {
            return note(format!("view {id} not found"));
        };
        let mut blocks = view.view.blocks.clone();
        chain.push(id.to_string());
        self.expand(&mut blocks, chain);
        chain.pop();
        Block::Section {
            heading: format!("↳ {}", view.view.title),
            blocks,
            opens: None,
        }
    }
}

fn note(text: String) -> Block {
    Block::Callout {
        tone: argus_protocol::Tone::Neutral,
        text,
    }
}

impl App {
    /// Lists the views the open saved view links to. A view that links to
    /// none says so rather than opening an empty list.
    pub(crate) fn list_links(&mut self) {
        let Some(view) = self.saved_views.opened() else {
            self.report("links are listed from a saved view");
            return;
        };
        let ids = linked_views(&view.view.blocks);
        if ids.is_empty() {
            self.report("this view links to no other view");
            return;
        }
        self.saved_views.links = Some(Links { ids, at: 0 });
    }

    /// Opens the link under the cursor and closes the list.
    pub(crate) fn follow_link(&mut self) {
        let Some(links) = self.saved_views.links.take() else {
            return;
        };
        if let Some(id) = links.ids.get(links.at).cloned() {
            self.saved_views.follow(id);
        }
    }

    /// Moves the link cursor while the list is up, and scrolls the view
    /// otherwise.
    pub(crate) fn stage_down(&mut self, delta: i32) {
        match &mut self.saved_views.links {
            Some(links) => {
                let last = links.ids.len().saturating_sub(1) as i32;
                links.at = (links.at as i32 + delta).clamp(0, last) as usize;
            }
            None => self.scroll_stage_by(delta),
        }
    }

    /// Backspace and `h`: closes the list if it is up, else goes back along
    /// the trail.
    pub(crate) fn back_in_stage(&mut self) {
        if self.saved_views.links.take().is_some() {
            return;
        }
        if !self.saved_views.back() {
            self.report("nothing to go back to");
        }
    }

    /// Esc: closes the list if it is up, else leaves for the workspace.
    pub(crate) fn leave_stage(&mut self) {
        if self.saved_views.links.take().is_some() {
            return;
        }
        self.open_view(View::Workspace);
    }

    /// `r`: the open saved view is refreshed by the agent that saved it; a
    /// live view is asked again as before.
    pub(crate) fn refresh_stage(&mut self) {
        if let Some(view) = self.saved_views.opened() {
            let _ = self.out.send(ClientMsg::RefreshSavedView { id: view.id.clone() });
            self.report("asked the agent to refresh it");
        } else if self.saved_views.is_open() {
            self.report("this view is not saved");
        } else {
            self.refresh_open_view();
        }
    }

    /// `s`: marks the open saved view out of date. The mark clears when the
    /// view is saved again.
    pub(crate) fn mark_stage_stale(&mut self) {
        match self.saved_views.opened() {
            Some(view) => {
                let _ = self.out.send(ClientMsg::MarkSavedViewStale { id: view.id.clone() });
            }
            None => self.report("only a saved view can be marked stale"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{AgentView, ViewScope};

    fn saved(id: &str, blocks: serde_json::Value) -> SavedView {
        SavedView {
            id: id.into(),
            scope: ViewScope::Global,
            features: Vec::new(),
            recipe: None,
            sources: Vec::new(),
            built_at: 0,
            built_commit: None,
            stale: false,
            view: AgentView {
                id: id.into(),
                title: format!("{id} title"),
                blocks: serde_json::from_value(blocks).unwrap(),
                ..AgentView::default()
            },
        }
    }

    fn embed(target: &str) -> serde_json::Value {
        serde_json::json!([{ "kind": "embed", "view": target }])
    }

    fn with(views: Vec<SavedView>) -> SavedViews {
        let mut store = SavedViews::default();
        store.replaced(views);
        store
    }

    #[test]
    fn a_refresh_replaces_the_view_with_its_id_and_a_new_one_is_appended() {
        let mut store = with(vec![saved("a", serde_json::json!([]))]);
        store.changed(saved("a", serde_json::json!([{ "kind": "divider" }])));
        store.changed(saved("b", serde_json::json!([])));
        let ids: Vec<&str> = store.all().iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
        assert_eq!(store.get("a").unwrap().view.blocks.len(), 1);
    }

    #[test]
    fn a_view_removed_while_open_is_drawn_as_not_saved_not_as_another() {
        let mut store = with(vec![saved("a", serde_json::json!([])), saved("b", serde_json::json!([]))]);
        store.choose(0);
        store.removed("a");
        assert!(store.is_open());
        assert!(store.opened().is_none());
    }

    #[test]
    fn following_links_builds_a_trail_that_goes_back_in_order() {
        let mut store = with(vec![saved("a", serde_json::json!([])), saved("b", serde_json::json!([])), saved("c", serde_json::json!([]))]);
        store.choose(0);
        store.follow("b".into());
        store.follow("c".into());
        assert_eq!(store.open_id(), Some("c"));
        assert!(store.back());
        assert_eq!(store.open_id(), Some("b"));
        assert!(store.back());
        assert_eq!(store.open_id(), Some("a"));
        assert!(!store.back());
    }

    #[test]
    fn choosing_from_the_list_starts_the_trail_over() {
        let mut store = with(vec![saved("a", serde_json::json!([])), saved("b", serde_json::json!([]))]);
        store.choose(0);
        store.follow("b".into());
        store.choose(1);
        assert!(!store.back());
    }

    #[test]
    fn an_embed_draws_the_embedded_blocks_under_its_title() {
        let store = with(vec![
            saved("outer", embed("inner")),
            saved("inner", serde_json::json!([{ "kind": "divider" }])),
        ]);
        let drawn = store.embedded("outer", &serde_json::from_value::<Vec<Block>>(embed("inner")).unwrap());
        match &drawn[0] {
            Block::Section { heading, blocks, .. } => {
                assert_eq!(heading, "↳ inner title");
                assert!(matches!(blocks[0], Block::Divider));
            }
            other => panic!("expected the embed drawn as a section, got {other:?}"),
        }
    }

    #[test]
    fn an_embed_of_a_missing_view_says_so() {
        let store = with(vec![]);
        let drawn = store.embedded("outer", &serde_json::from_value::<Vec<Block>>(embed("gone")).unwrap());
        assert!(matches!(&drawn[0], Block::Callout { text, .. } if text == "view gone not found"));
    }

    #[test]
    fn a_cycle_stops_at_the_view_that_repeats() {
        let store = with(vec![saved("a", embed("b")), saved("b", embed("a"))]);
        let drawn = store.embedded("a", &serde_json::from_value::<Vec<Block>>(embed("b")).unwrap());
        let Block::Section { blocks, .. } = &drawn[0] else { panic!("b is drawn") };
        assert!(matches!(&blocks[0], Block::Callout { text, .. } if text.contains("already drawn above")));
    }

    #[test]
    fn an_embed_nested_past_the_depth_is_not_drawn() {
        let store = with(vec![
            saved("a", embed("b")),
            saved("b", embed("c")),
            saved("c", embed("d")),
            saved("d", embed("e")),
            saved("e", serde_json::json!([{ "kind": "divider" }])),
        ]);
        let drawn = store.embedded("a", &serde_json::from_value::<Vec<Block>>(embed("b")).unwrap());
        let mut at = &drawn[0];
        let mut depth = 0;
        while let Block::Section { blocks, .. } = at {
            depth += 1;
            at = &blocks[0];
        }
        assert_eq!(depth, MAX_EMBED_DEPTH);
        assert!(matches!(at, Block::Callout { text, .. } if text.contains("too deep")));
    }
}
