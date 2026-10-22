//! What every block of an agent's view is drawn with: whose view it is, so an action or a reference
//! inside it knows whom to ask, the path of the block, which keys what the window remembers about
//! it, and what a value is drawn against (the tokens, the clock, where its presses go).

use std::collections::HashSet;
use std::rc::Rc;

use argus_desktop_core::connection::page::PageDiagram;
use argus_desktop_core::system::clock::Clock;
use argus_desktop_core::views::nav::NavView;
use argus_desktop_core::views::saved::{PageSaved, SavedColumn};

use super::memory::{child, view_path};
use crate::connection::caller::Caller;
use crate::theme::Tokens;

/// Whose view it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An agent's, whose pane reads its references and takes its presses.
    Agent,
    /// A saved view, which has no pane: a reference in it is read against the checkout on screen.
    Saved,
}

/// The view a block is in and where in it the block is.
#[derive(Debug, Clone)]
pub struct ViewCtx {
    pub kind: Kind,
    /// The pane whose agent showed the view; 0 for a saved one.
    pub pane: u64,
    pub id: String,
    /// The block's path under the view, `pane/id.2.0`.
    pub path: String,
    /// Each diagram block's source as Rust read it.
    pub diagrams: Rc<Vec<PageDiagram>>,
    /// Ids of the view's actions pressed and not yet answered by the agent showing the view again;
    /// their buttons wait.
    pub pressed: Rc<Vec<String>>,
    /// Whether the daemon takes a button press; a button is disabled when not.
    pub can_press: bool,
    /// The paths under the view (`2`, `1.0.3`) of the blocks that changed since the person saw them.
    pub changed: Rc<HashSet<String>>,
    /// Which change the marks of `changed` belong to, so each change is washed once (`blocks/changed`).
    /// The element drawing the blocks counts it; a context made elsewhere has none.
    pub flash: u64,
    /// The column of a saved panel this is drawn in, which a link followed here gets its target the
    /// column after. Absent in an agent's view and an extension's table.
    pub saved: Option<SavedColumn>,
    /// The saved views being drawn, outermost first: an embed of one of these is a cycle.
    pub chain: Rc<Vec<String>>,
    /// How many embeds were followed to reach this view.
    pub depth: usize,
}

impl ViewCtx {
    /// The context of an agent's view, at its root.
    pub fn agent(pane: u64, id: &str) -> Self {
        Self {
            kind: Kind::Agent,
            pane,
            id: id.to_owned(),
            path: view_path(pane, id),
            diagrams: Rc::default(),
            pressed: Rc::default(),
            can_press: false,
            changed: Rc::default(),
            flash: 0,
            saved: None,
            chain: Rc::default(),
            depth: 0,
        }
    }

    /// The context of a saved view's root: it has no pane, and its path is per column and per embed
    /// (`place` is where it is drawn, `chain` the saved views drawn around it), so a tab chosen or a
    /// row opened is kept where the person made the choice.
    pub fn saved(view: &PageSaved, host: Option<SavedColumn>, chain: Vec<String>, depth: usize, place: &str, can_press: bool) -> Self {
        Self {
            kind: Kind::Saved,
            pane: 0,
            id: view.id.clone(),
            path: format!("saved/{place}/{}", chain.join(">")),
            diagrams: Rc::new(view.diagrams.clone()),
            pressed: Rc::new(view.pressed.clone()),
            can_press,
            changed: Rc::default(),
            flash: 0,
            saved: host,
            chain: Rc::new(chain),
            depth,
        }
    }

    /// The pane whose agent showed the view, for a reference in it to be read against; a saved view
    /// or an extension's table has none.
    pub fn pane_of(ctx: Option<&Self>) -> Option<u64> {
        ctx.filter(|ctx| ctx.kind == Kind::Agent).map(|ctx| ctx.pane)
    }

    /// The context of the block at `index` under this one.
    pub fn at(&self, index: impl std::fmt::Display) -> Self {
        Self { path: child(&self.path, index), ..self.clone() }
    }

    /// Whether this block changed since the person saw it. The path under the view is what follows
    /// `pane/id.` in the context's own; an id may hold dots itself, so the prefix is measured.
    pub fn is_changed(&self) -> bool {
        let root = format!("{}.", view_path(self.pane, &self.id));
        !self.changed.is_empty() && self.path.strip_prefix(&root).is_some_and(|under| self.changed.contains(under))
    }
}

/// What a value is drawn against.
#[derive(Clone, Copy)]
pub struct Draw<'a> {
    pub tokens: &'a Tokens,
    /// The clock every age is said by: read once per draw, so a screenful agrees with itself.
    pub clock: &'a Clock,
    /// Where a press of a link or a reference goes.
    pub caller: &'a Caller,
    /// The saved views the window holds, whose titles a link to one wears.
    pub saved: &'a [NavView],
    /// The view the value is in; an extension's table has none.
    pub view: Option<&'a ViewCtx>,
}

impl Draw<'_> {
    /// The title of the saved view held under `id`.
    pub fn saved_title(&self, id: &str) -> Option<&str> {
        self.saved.iter().find(|view| view.id == id).map(|view| view.title.as_str())
    }
}

#[cfg(test)]
mod tests;
