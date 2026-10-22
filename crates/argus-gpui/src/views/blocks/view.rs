//! The one element a surface holds to draw a view's blocks: an agent's view in its panel or as a
//! page, a saved view in its column, and later an extension's.
//!
//! A surface hands it the blocks and whose view they are (`show`) every time its model changes, and
//! puts it where the blocks go; it draws them whole, as the page redraws a view whole, from what the
//! daemon said and what the window remembers (`views/memory`). It owns nothing that must outlive a
//! republish: the tab being read, the fold, the half-typed form are the shared memory's, and what GPUI
//! needs kept (a Markdown's selection, a scroll) is `kept`'s.

use std::rc::Rc;

use argus_desktop_core::views::nav::{self, NavView};
use argus_desktop_core::views::saved::SavedViewsPage;
use argus_protocol::Block;
use gpui_kit::{prelude::*, Context, Window};

use super::dispatch::blocks;
use super::kept::Kept;
use super::pen::Pen;
use crate::connection::caller::Caller;
use argus_desktop_core::system::clock::Clock;
use crate::theme::Tokens;
use crate::views::ctx::{Draw, ViewCtx};
use crate::views::memory::Shared;

pub struct Blocks {
    memory: Shared,
    caller: Caller,
    shown: Option<(Rc<Vec<Block>>, ViewCtx)>,
    saved: Option<Rc<SavedViewsPage>>,
    /// The saved views' titles, which a link to one wears; read off `saved` when it changes.
    titles: Vec<NavView>,
    kept: Kept,
    /// How many changes the blocks were shown with, so each is washed once (`changed`).
    flash: u64,
}

impl Blocks {
    pub fn new(memory: Shared, caller: Caller) -> Self {
        Self { memory, caller, shown: None, saved: None, titles: Vec::new(), kept: Kept::default(), flash: 0 }
    }

    /// Draws `blocks` as the view `ctx` is. `saved` is the saved views the window holds, whose bodies
    /// an embed draws and whose titles a link wears.
    pub fn show(&mut self, blocks: Rc<Vec<Block>>, ctx: ViewCtx, saved: Option<Rc<SavedViewsPage>>, cx: &mut Context<Self>) {
        let same_saved = match (&self.saved, &saved) {
            (Some(held), Some(now)) => Rc::ptr_eq(held, now),
            (None, None) => true,
            _ => false,
        };
        if !same_saved {
            self.titles = saved.as_deref().map(nav::views_of).unwrap_or_default();
            self.saved = saved;
        }
        // A view that came again is drawn from the agent's order, as the page redraws it.
        if self.shown.as_ref().is_none_or(|(held, _)| !Rc::ptr_eq(held, &blocks)) {
            self.kept.sorts.clear();
        }
        // A new change, in the blocks or in which of them changed, is a new wash; the same view said
        // again, as the session does whenever anything else about the window changes, is not.
        let again = self.shown.as_ref().is_some_and(|(held, held_ctx)| (Rc::ptr_eq(held, &blocks) || **held == *blocks) && held_ctx.changed == ctx.changed);
        if !again {
            self.flash += 1;
        }
        let ctx = ViewCtx { flash: self.flash, ..ctx };
        self.shown = Some((blocks, ctx));
        cx.notify();
    }

    /// How many changes the blocks have been shown with, which names the wash of each.
    #[cfg(test)]
    pub(crate) fn flash(&self) -> u64 {
        self.flash
    }

    /// How many entities and handles the blocks keep between draws.
    #[cfg(test)]
    pub(crate) fn pickers(&self) -> &super::pickers::Pickers {
        &self.kept.pickers
    }

    pub fn kept(&self) -> usize {
        self.kept.held()
    }
}

impl Render for Blocks {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some((all, ctx)) = self.shown.clone() else { return gpui_kit::div().into_any_element() };
        let clock = Clock::local();
        let draw = Draw { tokens: &tokens, clock: &clock, caller: &self.caller, saved: &self.titles, view: None };
        let repaint = cx.entity_id();
        let saved = self.saved.clone();
        let mut pen = Pen { draw, memory: &self.memory, saved: saved.as_deref(), repaint, kept: &mut self.kept, window, cx };
        let drawn = blocks(&all, &ctx, &mut pen);
        self.kept.sweep();
        drawn
    }
}

#[cfg(test)]
pub(crate) mod tests;
