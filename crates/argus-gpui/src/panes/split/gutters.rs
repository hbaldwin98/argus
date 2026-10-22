//! The gutters between the cells, and the drag that sizes the two panes either side of one
//! (SHELL-GRID-7 to -10).
//!
//! A vertical gutter sits after every pane but the last in a row, a horizontal one under each row
//! that has a row below it. What a drag does to the sizes, and the least a pane may be, is
//! `argus-desktop-core`'s (`workspace_grid::gutter`); this follows the pointer, draws the cells at
//! the sizes it has reached, and sends them once, when it is let go (`set_shares`). The panes are
//! held from reporting their room for as long as a gutter is held, so no child program is
//! reflowed once a frame (`Holder::Grid`).
//!
//! The sizes are then held here until the session echoes them or [`HOLD`] passes, because the
//! session declines a set that is not the grid's shape without a word, and a layout it does not
//! have must not be kept. A double-click on a gutter evens the row, or the rows (`even_shares`).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::panes::shown::workspace_grid::gutter::{least_across, least_down, Axis, Shares};
use argus_desktop_core::panes::shown::workspace_grid::rects::rects;
use argus_desktop_core::panes::shown::workspace_grid::GridPage;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    div, prelude::*, px, relative, AnyElement, Bounds, Context, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Point, Role,
    SharedString, Task, Window,
};

use super::SplitView;
use crate::kit::drag::{self, Drag, Dragger, Step};
use crate::layout::holders::Holder;
use crate::theme::Tokens;

/// How long sizes let go of wait for the session to say the same, before the session's are drawn.
pub const HOLD: Duration = Duration::from_millis(600);

/// What a press of a gutter holds: which one, where the pointer was along its axis, and the sizes
/// the grid had, so the gutter does not jump to put its edge under the pointer.
#[derive(Debug, Clone, PartialEq)]
pub struct Grip {
    axis: Axis,
    row: usize,
    at: usize,
    from: f32,
    start: Shares,
}

pub struct Gutters {
    drag: Drag<Grip>,
    /// The handle the drag takes the keys with, so Escape reaches it.
    keys: FocusHandle,
    /// The area the cells fill, for the pixels a share is worth.
    area: Rc<Cell<Bounds<Pixels>>>,
    /// The sizes a held gutter has reached.
    live: Option<Shares>,
    /// The sizes a gutter was let go at, until the session answers.
    held: Option<Shares>,
    waiting: Option<Task<()>>,
}

impl Gutters {
    pub fn new(cx: &mut Context<SplitView>) -> Self {
        Self { drag: Drag::new(), keys: cx.focus_handle(), area: Rc::default(), live: None, held: None, waiting: None }
    }

    pub fn area(&self) -> Rc<Cell<Bounds<Pixels>>> {
        self.area.clone()
    }

    /// The sizes to draw: a held gutter's, else the ones it was let go at, else the session's.
    pub fn shares(&self, page: &GridPage) -> Shares {
        self.live.clone().or_else(|| self.held.clone()).unwrap_or_else(|| shares_of(page))
    }

    /// The session said `page`: the sizes a gutter was let go at are no longer waited for if it
    /// says the same.
    pub fn echoed(&mut self, page: &GridPage) {
        if self.held.as_ref().is_some_and(|held| *held == shares_of(page)) {
            self.held = None;
            self.waiting = None;
        }
    }
}

/// The sizes a page carries.
pub fn shares_of(page: &GridPage) -> Shares {
    Shares { heights: page.heights.clone(), widths: page.widths.clone() }
}

/// The pointer's place along an axis.
fn along(axis: Axis, at: Point<Pixels>) -> f32 {
    match axis {
        Axis::Across => at.x.as_f32(),
        Axis::Down => at.y.as_f32(),
    }
}

impl SplitView {
    /// The sizes a gutter reaches with the pointer at `at`.
    fn reached(&self, grip: &Grip, at: Point<Pixels>, cx: &Context<Self>) -> Shares {
        let tokens = Tokens::global(cx);
        let area = self.gutters.area.get();
        let total = match grip.axis {
            Axis::Across => area.size.width.as_f32(),
            Axis::Down => area.size.height.as_f32(),
        };
        let (cell_w, cell_h) = self.workspace.read(cx).cell.get();
        // What a cell keeps round its columns: the screen's padding, the card's gap and hairline.
        let round = 2.0 * (tokens.sp_8 + tokens.sp_4 + tokens.hairline);
        let least = match grip.axis {
            Axis::Across => least_across(self.drawn.1.min_cols, cell_w.as_f32(), round),
            Axis::Down => least_down(self.drawn.1.min_rows, cell_h.as_f32(), round, tokens.pane_head_h),
        };
        grip.start.dragged(grip.axis, grip.row, grip.at, along(grip.axis, at) - grip.from, total, least)
    }

    /// Lets the sizes go to the session, and keeps them until it answers.
    fn let_go(&mut self, shares: Shares, cx: &mut Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::SetShares { heights: shares.heights.clone(), widths: shares.widths.clone() });
        }
        self.gutters.held = Some(shares);
        self.gutters.waiting = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOLD).await;
            this.update(cx, |this, cx| {
                this.gutters.held = None;
                this.gutters.waiting = None;
                cx.notify();
            })
            .ok();
        }));
    }

    /// Holds the panes from reporting, or lets them.
    fn hold_panes(&self, holding: bool, cx: &mut Context<Self>) {
        self.heard.update(cx, |heard, cx| {
            let changed = if holding { heard.hold_resize(Holder::Grid) } else { heard.release_resize(Holder::Grid) };
            if changed {
                cx.notify();
            }
        });
    }

    /// Every gutter of the grid at `shares`: one after each pane but the last of a row, one under
    /// each row that has another below.
    pub(super) fn gutter_elements(&self, page: &GridPage, shares: &Shares, tokens: &Tokens, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let lines = rects(&shares.heights, &shares.widths);
        let mut out = Vec::new();
        for (r, row) in page.grid.iter().enumerate() {
            let Some(line) = lines.get(r) else { continue };
            for (c, rect) in line.iter().enumerate().take(row.len().saturating_sub(1)) {
                
                let held = self.gutters.drag.payload().is_some_and(|grip| grip.axis == Axis::Across && grip.row == r && grip.at == c);
                let element = div()
                    .absolute()
                    .left(relative(rect.x + rect.w))
                    .ml(px(-tokens.sp_12))
                    .top(relative(rect.y))
                    .h(relative(rect.h))
                    .w(px(tokens.sp_24))
                    .cursor_col_resize();
                out.push(self.gutter(format!("gutter-across-{r}-{c}"), Axis::Across, r, c, held, element, tokens, cx));
            }
            if let Some(next) = lines.get(r + 1).and_then(|row| row.first()) {
                let held = self.gutters.drag.payload().is_some_and(|grip| grip.axis == Axis::Down && grip.at == r);
                let element = div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(relative(next.y))
                    .mt(px(-tokens.sp_12))
                    .h(px(tokens.sp_24))
                    .cursor_row_resize();
                out.push(self.gutter(format!("gutter-down-{r}"), Axis::Down, r, r, held, element, tokens, cx));
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn gutter(
        &self,
        id: String,
        axis: Axis,
        row: usize,
        at: usize,
        held: bool,
        placed: gpui_kit::Div,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = SharedString::from(id);
        let group = id.clone();
        let accent = tokens.accent;
        let (inset, edge, rule) = (tokens.sp_8, tokens.sp_12 - tokens.rule / 2.0, tokens.rule);
        placed
            .id(id)
            .test_support()
            .role(Role::Splitter)
            .aria_label("Resize the panes")
            .group(group.clone())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    // The press is the stage's to take the keys with, as any press on it is.
                    window.prevent_default();
                    if this.heard.read(cx).held() {
                        return;
                    }
                    if event.click_count >= 2 {
                        // A double-click evens the row (a vertical gutter) or the rows.
                        let row = (axis == Axis::Across).then_some(row as u8);
                        this.caller.call(Call::EvenShares { row });
                        return;
                    }
                    let page = this.drawn.1.clone();
                    let start = this.gutters.shares(&page);
                    let grip = Grip { axis, row, at, from: along(axis, event.position), start };
                    this.gutters.drag.press(event.position, grip);
                }),
            )
            // The line a gutter lights, centred in the 24 px it can be grabbed by.
            .child({
                let line = div().absolute().rounded_full().bg(accent).opacity(if held { 1.0 } else { 0.0 });
                match axis {
                    Axis::Across => line.top(px(inset)).bottom(px(inset)).left(px(edge)).w(px(rule)),
                    Axis::Down => line.left(px(inset)).right(px(inset)).top(px(edge)).h(px(rule)),
                }
                .group_hover(group, |style| style.opacity(1.0))
            })
            .into_any_element()
    }

    /// The handle the drag takes the keys with when it begins, so Escape reaches it first.
    pub(super) fn escape_keys(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("gutter-keys")
            .size_0()
            .track_focus(&self.gutters.keys)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| drag::escape(this, event, window, cx)))
    }
}

impl Dragger for SplitView {
    type Payload = Grip;

    fn drag(&self) -> &Drag<Grip> {
        &self.gutters.drag
    }

    fn drag_mut(&mut self) -> &mut Drag<Grip> {
        &mut self.gutters.drag
    }

    fn keys(&self) -> &FocusHandle {
        &self.gutters.keys
    }

    fn dragged(&mut self, step: Step<Grip>, _: &mut Window, cx: &mut Context<Self>) {
        match step {
            Step::Began(grip) => {
                self.hold_panes(true, cx);
                if let Some(at) = self.gutters.drag.at() {
                    self.gutters.live = Some(self.reached(&grip, at, cx));
                }
            }
            Step::Moved(grip) => {
                if let Some(at) = self.gutters.drag.at() {
                    self.gutters.live = Some(self.reached(&grip, at, cx));
                }
            }
            Step::Dropped(grip, at) => {
                let shares = self.reached(&grip, at, cx);
                self.gutters.live = None;
                self.hold_panes(false, cx);
                self.let_go(shares, cx);
            }
            Step::Cancelled(_) => {
                self.gutters.live = None;
                self.hold_panes(false, cx);
            }
            Step::Clicked(_) => {}
        }
    }
}
