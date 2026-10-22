//! The Workspace's panes side by side: a cell for each pane the rail's grid names, placed as
//! fractions of the stage body, with a gutter on each seam and a header on each cell when there
//! are two or more (`panes/split.ts`).
//!
//! Rust keeps the layout and the sizes; this draws what the rail says and holds nothing of its own
//! but the screen of each cell (one `Terminal` per `Beside(n)`, kept while the cell lasts and let
//! go with it, so a cell that comes back reports its room again), the sizes a gutter was let go at
//! until the session answers (`gutters`), and, while a pane is carried, the layout it drew when the
//! carrying began (SHELL-GRID-17: Rust's redraws do not move the cells under a drag).
//!
//! The cell's body is [`cells::Kind`]: a terminal, or the place a browser will fill. Where the
//! cells and the stage were drawn is published for the carried pane to land on (`kit/spots`); the
//! carrying and the drop are the work area's (`layout/carry`).

pub mod cells;
pub mod focus;
pub mod gutters;
pub mod header;

use argus_desktop_core::connection::page::{Shown, Side};
use argus_desktop_core::panes::shown::workspace_grid::rects::cells as placed;
use argus_desktop_core::panes::shown::workspace_grid::GridPage;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, relative, Context, Entity, FocusHandle, SharedString, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::area;
use crate::kit::drag;
use crate::kit::spots::{publish, Spot, Spots};
use crate::panes::terminal::Terminal;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use cells::Kind;
use gutters::Gutters;

pub struct SplitView {
    heard: Entity<Heard>,
    caller: Caller,
    /// The Workspace's own screen, cell 0, which the window also holds for its keys and its size.
    workspace: Entity<Terminal>,
    /// The screens of the cells beside it: `Beside(n)` is `beside[n]`.
    beside: Vec<Entity<Terminal>>,
    /// The pane in the first cell, and the grid, as last drawn.
    drawn: (Option<Shown>, GridPage),
    gutters: Gutters,
    keys: focus::Keys,
    _observing: Subscription,
    _focusing: Subscription,
}

impl SplitView {
    pub fn new(heard: Entity<Heard>, caller: Caller, workspace: Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let observing = cx.observe_in(&heard, window, |this, _, window, cx| {
            this.hear(window, cx);
            cx.notify();
        });
        let focusing = focus::redirect(&workspace, window, cx);
        let drawn = (heard.read(cx).rail.shown.clone(), heard.read(cx).rail.row.clone());
        let mut this = Self {
            heard,
            caller,
            workspace,
            beside: Vec::new(),
            drawn,
            gutters: Gutters::new(cx),
            keys: focus::Keys::default(),
            _observing: observing,
            _focusing: focusing,
        };
        this.hear(window, cx);
        this
    }

    /// The Workspace's own screen.
    pub fn workspace(&self) -> &Entity<Terminal> {
        &self.workspace
    }

    /// How many cells the grid draws: none with no pane on screen.
    fn wanted(&self) -> usize {
        if self.drawn.0.is_some() { 1 + self.drawn.1.beside.len() } else { 0 }
    }

    /// The pane in cell `n` as the grid draws it.
    fn pane_in(&self, n: u8) -> Option<&Shown> {
        match n {
            0 => self.drawn.0.as_ref(),
            n => self.drawn.1.beside.get(usize::from(n) - 1).and_then(Option::as_ref),
        }
    }

    /// The screen of cell `n`.
    fn screen_of(&self, n: u8) -> Option<&Entity<Terminal>> {
        match n {
            0 => Some(&self.workspace),
            n => self.beside.get(usize::from(n) - 1),
        }
    }

    /// Takes in what the session said: a cell comes for each pane beside the first and goes with
    /// it, the sizes a gutter was let go at are dropped once the session echoes them, and the keys
    /// are followed. None of it while a pane is carried, or the cells would move under it.
    fn hear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if Spots::of(cx).carrying() {
            return;
        }
        self.sync(window, cx);
        self.follow_keys(window, cx);
    }

    /// Whether the session has said something since the grid was last drawn from it.
    fn stale(&self, cx: &Context<Self>) -> bool {
        let heard = self.heard.read(cx);
        self.drawn.0 != heard.rail.shown || self.drawn.1 != heard.rail.row
    }

    /// Draws the grid from what the session last said.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (shown, page) = {
            let heard = self.heard.read(cx);
            (heard.rail.shown.clone(), heard.rail.row.clone())
        };
        self.gutters.echoed(&page);
        self.drawn = (shown, page);
        let wanted = self.wanted().saturating_sub(1);
        if self.beside.len() > wanted {
            self.beside.truncate(wanted);
        }
        while self.beside.len() < wanted {
            let side = Side::Beside(self.beside.len() as u8);
            let (heard, caller) = (self.heard.clone(), self.caller.clone());
            self.beside.push(cx.new(|cx| Terminal::new(heard, caller, side, window, cx)));
        }
    }
}

impl Render for SplitView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let spots = Spots::of(cx);
        // What was drawn last frame may be gone; every cell says where it is again as it paints.
        spots.forget(|spot| matches!(spot, Spot::Cell(_) | Spot::Stage));
        // A session that spoke while a pane was carried is drawn once the carrying is over.
        if !spots.carrying() && self.stale(cx) {
            self.sync(window, cx);
            cx.defer_in(window, |this, window, cx| this.follow_keys(window, cx));
        }
        // The sizes in thousandths: a gutter's own while it is held or waiting, else the session's.
        let page = self.drawn.1.clone();
        let wanted = self.wanted();
        if wanted == 0 {
            // No pane on screen: the Workspace's own screen alone, empty, which still has the keys.
            return div().id("split").test_support().size_full().child(self.workspace.clone()).into_any_element();
        }
        let split = wanted > 1;
        let shares = self.gutters.shares(&page);
        let rects = placed(&page.grid, &shares.heights, &shares.widths);
        let cells = rects.iter().map(|(n, rect)| self.cell(*n, *rect, split, &tokens, cx)).collect::<Vec<_>>();
        let gutters = if split { self.gutter_elements(&page, &shares, &tokens, cx) } else { Vec::new() };
        div()
            .id("split")
            .test_support()
            .relative()
            .size_full()
            // `#split` leaves half the gap a card keeps to its neighbour at the stage's edge.
            .p(rem(tokens.sp_4))
            .child(publish(Spot::Stage))
            .child(
                v_flex().relative().size_full().child(area::measure(self.gutters.area())).children(cells).children(gutters).child(self.escape_keys(cx)).child(drag::watch(cx.weak_entity())),
            )
            .into_any_element()
    }
}

impl SplitView {
    /// Cell `n`: its card, placed by `rect`, holding its header and its body.
    fn cell(&self, n: u8, rect: argus_desktop_core::panes::shown::workspace_grid::Rect, split: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = self.pane_in(n);
        let keyed = split && self.heard.read(cx).keyed_cell().unwrap_or(0) == n;
        let head = pane.filter(|_| split).map(|pane| self.head(n, pane, keyed, tokens, cx));
        let kind = Kind::of(pane);
        let body = match (kind, self.screen_of(n)) {
            (Kind::Terminal, Some(screen)) => div().size_full().child(screen.clone()).into_any_element(),
            _ => cells::reserved(n, kind, tokens).into_any_element(),
        };
        div()
            .id(SharedString::from(format!("cell-{n}")))
            .test_support()
            .absolute()
            .left(relative(rect.x))
            .top(relative(rect.y))
            .w(relative(rect.w))
            .h(relative(rect.h))
            // Cells abut at their seam; each keeps half the gap as the stage showing through.
            .p(rem(tokens.sp_4))
            .child(publish(Spot::Cell(n)))
            .child(
                v_flex()
                    .size_full()
                    .overflow_hidden()
                    .rounded(rem(tokens.r_md))
                    .border_1()
                    .border_color(tokens.line)
                    .bg(tokens.bg_surface)
                    .children(head)
                    .child(div().flex_1().min_h_0().child(body)),
            )
    }

    /// The handle of the screen of cell `n`.
    pub(crate) fn handle_of(&self, n: u8, cx: &gpui_kit::App) -> Option<FocusHandle> {
        self.screen_of(n).map(|screen| screen.read(cx).focus_handle().clone())
    }
}

#[cfg(test)]
mod tests;
