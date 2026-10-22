//! A saved panel: the trail of columns side by side under the panel's head, one element wherever
//! the panel is shown, docked or floating (CHECKOUTS-CODE-VIEWS-SAVED-1, -10, -12).
//!
//! Rust holds the saved views and which panel shows which trail; the window holds what the person
//! did to the panel: how far it is scrolled across, how far each column is scrolled down (by
//! `<column>|<view id>`), and, in `views/memory`, which Details and delete questions are open. All of
//! it outlives a redraw, as the page's does. When a column was added since the last draw the panel
//! scrolls to its right end, at once and again once laid out, since a panel just brought out has no
//! width until it is.

use std::collections::HashMap;
use std::rc::Rc;

use argus_desktop_core::layout::docks::Panel;
use argus_desktop_core::system::clock::Clock;
use argus_desktop_core::views::column::{floating, headed};
use argus_desktop_core::views::saved::{SavedColumn, SavedViewsPage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{point, prelude::*, px, AnyElement, Context, Entity, Role, ScrollHandle, SharedString, Subscription, Window};

use super::column::{column, missing};
use super::head::{strip, Strip};
use super::header::Head;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::theme::Tokens;
use crate::views::blocks::kept::Kept;
use crate::views::blocks::pen::Pen;
use crate::views::blocks::view::Blocks;
use crate::views::ctx::{Draw, ViewCtx};

pub struct SavedPanel {
    panel: Panel,
    heard: Entity<Heard>,
    caller: Caller,
    /// How far the columns are scrolled across.
    across: ScrollHandle,
    /// How many columns the panel had when last drawn.
    seen: Option<usize>,
    /// The element drawing each column's blocks, by `<column>|<view id>`.
    bodies: HashMap<String, Entity<Blocks>>,
    /// How far each column is scrolled down, by the same key.
    scrolls: HashMap<String, ScrollHandle>,
    /// The page the bodies were last handed.
    pushed: Option<Rc<SavedViewsPage>>,
    _observing: Subscription,
}

/// Whether the panel scrolls to show a column added since the last draw: not on its first draw.
pub fn grew(seen: Option<usize>, now: usize) -> bool {
    seen.is_some_and(|before| now > before)
}

fn to_end(across: &ScrollHandle) {
    let max = across.max_offset();
    across.set_offset(point(-max.x, px(0.0)));
}

impl SavedPanel {
    pub fn new(panel: Panel, heard: Entity<Heard>, caller: Caller, cx: &mut Context<Self>) -> Self {
        let observing = cx.observe(&heard, |this, heard, cx| {
            let page = heard.read(cx).saved.clone();
            super::registry::forget_if_closed(&this.panel, page.as_deref(), cx);
            cx.notify();
        });
        Self { panel, heard, caller, across: ScrollHandle::new(), seen: None, bodies: HashMap::new(), scrolls: HashMap::new(), pushed: None, _observing: observing }
    }
}

impl Render for SavedPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let (page, docks, held, titles, memory) = {
            let heard = self.heard.read(cx);
            (heard.saved.clone(), heard.docks.clone(), heard.held(), heard.saved_views.clone(), heard.memory.clone())
        };
        let shell = || v_flex().id(SharedString::from(format!("saved-panel-{}", self.panel))).role(Role::Region).test_support().aria_label("Saved view").size_full().min_w_0().bg(tokens.bg_surface);
        let Some((page, columns)) = page.and_then(|page| page.panels.iter().find(|p| p.panel == self.panel).map(|p| p.columns.clone()).map(|columns| (page, columns))) else {
            return shell().into_any_element();
        };
        let fresh = self.pushed.as_ref().is_none_or(|held| !Rc::ptr_eq(held, &page));
        self.pushed = Some(page.clone());
        let name = self.panel.to_string();

        // The bodies are handed the views as the daemon last said them; each draws itself.
        let mut shown: Vec<Option<(String, ViewCtx)>> = Vec::new();
        for (index, id) in columns.iter().enumerate() {
            let Some(view) = page.views.iter().find(|view| &view.id == id) else {
                shown.push(None);
                continue;
            };
            let key = format!("{index}|{id}");
            let ctx = ViewCtx::saved(view, Some(SavedColumn { panel: self.panel.clone(), column: index }), vec![id.clone()], 0, &format!("{name}/{index}"), page.can_press);
            let body = match self.bodies.get(&key) {
                Some(body) => (body.clone(), fresh),
                None => (cx.new(|_| Blocks::new(memory.clone(), self.caller.clone())), true),
            };
            self.bodies.insert(key.clone(), body.0.clone());
            self.scrolls.entry(key.clone()).or_default();
            if body.1 {
                let blocks = Rc::new(view.view.blocks.clone());
                let saved = Some(page.clone());
                body.0.update(cx, |blocks_view, cx| blocks_view.show(blocks, ctx.clone(), saved, cx));
            }
            shown.push(Some((key, ctx)));
        }

        let grown = grew(self.seen, columns.len());
        self.seen = Some(columns.len());
        if grown {
            to_end(&self.across);
            let across = self.across.clone();
            let me = cx.entity_id();
            window.on_next_frame(move |window, _| {
                to_end(&across);
                window.on_next_frame(move |_, cx| {
                    to_end(&across);
                    cx.notify(me);
                });
            });
        }

        let first = page.views.iter().find(|view| columns.first() == Some(&view.id));
        let drawn_head = headed(columns.len(), &self.panel, &docks);
        let title = first.map_or("Saved view", |view| view.view.title.as_str()).to_owned();
        // A floating panel's head is its frame's title bar (`layout/float`).
        let top = (drawn_head && !floating(&self.panel, &docks)).then(|| {
            strip(&Strip { panel: &self.panel, title: &title, columns: columns.len(), docks: &docks, page: &page, held }, &self.heard, &self.caller, &tokens, cx)
                .into_any_element()
        });

        let clock = Clock::local();
        let mut kept = Kept::default();
        let repaint = cx.entity_id();
        let draw = Draw { tokens: &tokens, clock: &clock, caller: &self.caller, saved: &titles, view: None };
        let mut pen = Pen { draw, memory: &memory, saved: Some(&page), repaint, kept: &mut kept, window, cx };
        let lone = columns.len() == 1;
        let drawn: Vec<AnyElement> = columns
            .iter()
            .enumerate()
            .zip(shown)
            .map(|((index, id), shown)| {
                let last = index + 1 == columns.len();
                let Some((key, ctx)) = shown else { return missing(&self.panel, index, id, last, held, &pen) };
                let view = page.views.iter().find(|view| &view.id == id).expect("a column shown names a view the page holds");
                let head = Head { panel: &self.panel, index, view, lone, headed: drawn_head, held };
                let body = self.bodies[&key].clone();
                column(&head, &ctx, body.into_any_element(), &self.scrolls[&key], last, &mut pen)
            })
            .collect();
        shell()
            .children(top)
            .child(h_flex().id(SharedString::from(format!("saved-cols-{}", self.panel))).test_support().flex_1().min_h_0().overflow_x_scroll().restrict_scroll_to_axis().track_scroll(&self.across).children(drawn))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
