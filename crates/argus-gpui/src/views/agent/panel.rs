//! The one content element of the views agents showed: the panel's tabs, header and blocks, or as
//! the page over the stage the header's meta line and the blocks (`drawViews`, `drawPopped`).
//!
//! Rust says which view each shows (`ViewsPage::open`, `popped`); this draws it from the page's
//! last event and keeps what the page keeps: the reader's place in a view, which stays when the
//! same view comes again or another is added and starts at the top for a different one, and the
//! age in the meta line, drawn again on a timer so it does not go stale between events.

use std::rc::Rc;
use std::time::Duration;

use argus_desktop_core::connection::page::{PageView, ViewsPage};
use argus_desktop_core::system::clock::Clock;
use argus_desktop_core::views::head;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, point, prelude::*, px, Context, Entity, ScrollHandle, Subscription, Task, Window};

use super::{acts, head as header, tabs};
use crate::connection::heard::Heard;
use crate::views::blocks::kept::Kept;
use crate::views::blocks::pen::Pen;
use crate::views::blocks::view::Blocks;
use crate::connection::caller::Caller;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::ctx::Draw;

/// How often the age is said again: it is told in minutes, so a half minute is never wrong by one.
const AGE_EVERY: Duration = Duration::from_secs(30);

/// Where the content is: in the dock, with the tabs over it, or as the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Panel,
    Page,
}

pub struct AgentView {
    place: Place,
    heard: Entity<Heard>,
    caller: Caller,
    blocks: Entity<Blocks>,
    /// The body's scroll, and which view it was last laid out for.
    scroll: ScrollHandle,
    tabs_scroll: ScrollHandle,
    shown: Option<String>,
    /// The page the blocks were last handed, so an event that is not about the views hands them nothing.
    handed: Option<Rc<ViewsPage>>,
    chosen: Option<usize>,
    /// What the header's buttons keep between draws.
    kept: Kept,
    _observing: Subscription,
    _ageing: Task<()>,
}

impl AgentView {
    pub fn new(place: Place, heard: Entity<Heard>, caller: Caller, cx: &mut Context<Self>) -> Self {
        let memory = heard.read(cx).memory.clone();
        let blocks = cx.new(|_| Blocks::new(memory, caller.clone()));
        let observing = cx.observe(&heard, |this, _, cx| {
            this.hand(cx);
            cx.notify();
        });
        let ageing = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(AGE_EVERY).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        });
        let mut view = Self {
            place,
            heard,
            caller,
            blocks,
            scroll: ScrollHandle::new(),
            tabs_scroll: ScrollHandle::new(),
            shown: None,
            handed: None,
            chosen: None,
            kept: Kept::default(),
            _observing: observing,
            _ageing: ageing,
        };
        view.hand(cx);
        view
    }

    /// The view this shows in `page`.
    fn view<'a>(&self, page: &'a ViewsPage) -> Option<&'a PageView> {
        match self.place {
            Place::Panel => head::open(page),
            Place::Page => head::popped(page),
        }
    }

    /// Hands the blocks the view when the page changed.
    fn hand(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.heard.read(cx).views.clone() else { return };
        if self.handed.as_ref().is_some_and(|held| Rc::ptr_eq(held, &page)) {
            return;
        }
        if let Some(view) = self.view(&page) {
            let ctx = acts::context(view, page.can_press);
            let saved = self.heard.read(cx).saved.clone();
            let blocks = Rc::new(view.view.blocks.clone());
            self.blocks.update(cx, |shown, cx| shown.show(blocks, ctx, saved, cx));
        }
        self.handed = Some(page);
    }

    /// Keeps the reader's place while the same view is shown, else starts it at the top.
    fn keep_place(&mut self, key: Option<String>) {
        if self.shown != key {
            self.scroll.set_offset(point(px(0.0), px(0.0)));
            self.shown = key;
        }
    }
}

impl Render for AgentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let (page, docks, memory, held) = {
            let heard = self.heard.read(cx);
            (heard.views.clone(), heard.docks.clone(), heard.memory.clone(), heard.held())
        };
        let Some(page) = page else { return div().into_any_element() };
        let Some(view) = self.view(&page).cloned() else {
            self.keep_place(None);
            // The panel with no view still has its strip to put away or move, as the page's.
            return div().into_any_element();
        };
        let popped = self.place == Place::Page;
        self.keep_place(Some(head::key(&view)));
        let clock = Clock::local();
        let chosen = page.views.iter().position(|other| (other.pane, &other.view.id) == (view.pane, &view.view.id)).unwrap_or(0);
        if !popped && self.chosen != Some(chosen) {
            self.chosen = Some(chosen);
            // The open tab is never the one the strip scrolled away.
            self.tabs_scroll.scroll_to_item(chosen);
        }
        let strip = (!popped).then(|| tabs::strip(tabs::Strip { page: &page, chosen, scroll: &self.tabs_scroll, docks: &docks, held, caller: &self.caller, heard: &self.heard }, &tokens, cx));
        let acts = if popped {
            Vec::new()
        } else {
            let draw = Draw { tokens: &tokens, clock: &clock, caller: &self.caller, saved: &[], view: None };
            let repaint = cx.entity_id();
            let pen = Pen { draw, memory: &memory, saved: None, repaint, kept: &mut self.kept, window, cx };
            acts::acts(&view, popped, page.can_press, &pen, &self.heard)
        };
        let head = header::header(&view, popped, acts, &clock, &self.caller, &tokens);
        v_flex()
            .id(if popped { "view-page" } else { "views-panel" })
            .test_support()
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(tokens.see_surface)
            .when(held, |panel| panel.opacity(tokens.opacity_held))
            .children(strip)
            .child(head)
            .child(
                div()
                    .id("view-body")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .p(rem(tokens.sp_16))
                    .child(div().w_full().min_w_0().child(self.blocks.clone())),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
