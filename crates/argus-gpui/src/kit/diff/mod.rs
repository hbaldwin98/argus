//! A diff drawn: a file's rows in a virtual list, so only what is on screen is built however long
//! the diff is.
//!
//! The rows are `argus-desktop-core`'s `DiffRow`s, unified or split, and what each one is (a hunk,
//! a fold, a line, a pair; the cursor, the pick; what the highlighter cut) is already said in
//! them. This draws them and reports what was pressed: a click on a line, with Shift held or not,
//! and a fold's buttons. It sends no call and changes no row, so a refused click leaves the diff
//! as the session says it is.
//!
//! One diff view serves review and history, and the patch block and the Code stage after them, so
//! it knows none of their calls: a surface that does not take clicks asks for a view with none. A
//! surface may put one thing among the rows, a slot of a height it names, under a row: the
//! review's comment box. The scroll offset stays across every update and the cursor's row is
//! brought into view when the rows change, "nearest" (SHARED-5): no movement while it is already
//! visible.
//!
//! A long line is never cut, as in the page. Both scrollbars always show, as the page's native box
//! does; a held middle button drags the diff, and the stage's Left and Right nudge it.

pub mod fold;
pub mod gutter;
pub mod items;
pub mod measure;
pub mod menu;
pub mod paint;
pub mod pan;
pub mod rows;
pub mod selection;
pub mod shape;
pub mod text;
pub mod tone;
pub mod wrap;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::DiffRow;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::{v_virtual_list, ActiveTheme as _, VirtualListScrollHandle};
use gpui_kit::{
    canvas, div, prelude::*, px, AnyElement, AnyView, Bounds, Context, EventEmitter, FocusHandle, Font, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollStrategy, SharedString, Size, TestSupportExt as _, Window,
};

use crate::kit::drag::{self, Drag};
use crate::theme::scale::{pixels, zoom};
use crate::theme::Tokens;
use items::Item;
use paint::Layouts;
use selection::{Held, Pos};
use text::Code;

/// What a press on the diff means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffEvent {
    /// A line, pair or fold; `extend` is Shift held.
    Row { row: usize, extend: bool },
    /// One of a fold's buttons: a step of its lines, or all of them.
    Fold { row: usize, all: bool },
    /// A drag that began on one row and ended on another, in a view that
    /// picks: the rows it began and ended on, for a click then a shift-click.
    Range { first: usize, last: usize },
    /// `Copy` chosen from the menu a right-click on selected text opens: the owner sends the
    /// selected text (`selection_text`) to Rust.
    CopySelection,
}

/// A thing drawn under a row, among the rows.
#[derive(Clone)]
pub struct Slot {
    pub view: AnyView,
    pub height: Pixels,
}

pub struct DiffView {
    id: SharedString,
    interactive: bool,
    /// Whether a drag across rows is told to the owner as a range to pick.
    picks: bool,
    rows: Rc<Vec<DiffRow>>,
    split: bool,
    slot: Option<Slot>,
    items: Rc<Vec<Item>>,
    sizes: Rc<Vec<Size<Pixels>>>,
    scroll: VirtualListScrollHandle,
    built: Cell<usize>,
    /// Each row's left and right side as drawn (tabs stopped), made once per
    /// rows rather than per frame.
    drawn: Vec<[Code; 2]>,
    /// Each row's height: a pair's grows with the lines its longer half wraps to.
    heights: Vec<f32>,
    /// The cells of the widest line, and of the widest hunk header or note.
    widest: (usize, usize),
    /// The list's width as the last frame laid it out, and the advance of one
    /// mono cell in the diff's font, once rendered.
    pane: Rc<Cell<Pixels>>,
    advance: Pixels,
    /// How far the window is zoomed, which the rows' sizes (the page's pixels) are measured at, and
    /// the zoom `heights` was made at, so a zoom remeasures them.
    zoom: f32,
    sized_at: f32,
    /// The cells a split half's code is wrapped at when `heights` was made.
    per_line: usize,
    /// Where a middle-button pan was pressed and the scroll offset then.
    panning: Option<(Point<Pixels>, Point<Pixels>)>,
    drag: Drag<Held>,
    keys: FocusHandle,
    /// Where the selection began and where it is now, either way round.
    selection: Option<(Pos, Pos)>,
    layouts: Layouts,
    /// Where the list was laid out last, for the edges a drag scrolls at.
    area: Rc<Cell<Bounds<Pixels>>>,
}

#[cfg(test)]
thread_local! {
    /// What each diff view by id has built, for a test that cannot reach the
    /// entity inside a whole window.
    static BUILT: std::cell::RefCell<std::collections::HashMap<String, usize>> = Default::default();
}

/// How many items the diff view named `id` has built on this thread, over all
/// its lives, as [`DiffView::built`] counts them.
#[cfg(test)]
pub(crate) fn built_by(id: &str) -> usize {
    BUILT.with(|built| built.borrow().get(id).copied().unwrap_or(0))
}

impl EventEmitter<DiffEvent> for DiffView {}

impl DiffView {
    /// A view named `id`, which takes clicks when `interactive`, and tells a
    /// drag across rows when `picks`. Its text can always be selected.
    pub fn new(id: impl Into<SharedString>, interactive: bool, picks: bool, cx: &mut Context<Self>) -> Self {
        Self {
            id: id.into(),
            interactive,
            picks,
            rows: Rc::default(),
            split: false,
            slot: None,
            items: Rc::default(),
            sizes: Rc::default(),
            scroll: VirtualListScrollHandle::new(),
            built: Cell::new(0),
            drawn: Vec::new(),
            heights: Vec::new(),
            widest: (0, 0),
            pane: Rc::default(),
            advance: px(0.0),
            zoom: 1.0,
            sized_at: 1.0,
            per_line: 0,
            panning: None,
            drag: Drag::new(),
            keys: cx.focus_handle(),
            selection: None,
            layouts: Rc::default(),
            area: Rc::default(),
        }
    }

    /// Draws `rows`, as split pairs or unified lines, with `slot` under the
    /// row it belongs to. Nothing changes, and nothing is notified, when it is
    /// what was drawn already: the session says the same thing again whenever
    /// anything else about the window changes.
    pub fn show(&mut self, rows: &[DiffRow], split: bool, slot: Option<Slot>, cx: &mut Context<Self>) {
        let same_rows = self.rows.as_slice() == rows;
        let same_slot = match (&self.slot, &slot) {
            (None, None) => true,
            (Some(a), Some(b)) => a.view.entity_id() == b.view.entity_id() && a.height == b.height,
            _ => false,
        };
        if same_rows && same_slot && self.split == split {
            return;
        }
        if !same_rows {
            self.rows = Rc::new(rows.to_vec());
            self.selection = None;
            self.draw();
        }
        self.split = split;
        self.slot = slot;
        self.items = Rc::new(items::items(&self.rows, self.slot.is_some()));
        self.per_line = self.wrap_at();
        self.size();
        if !same_rows {
            if let Some(at) = items::cursor_item(&self.rows, &self.items) {
                self.scroll.scroll_to_item(at, ScrollStrategy::Top);
            }
        }
        cx.notify();
    }

    /// How many items the list holds: its rows and the slot.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// How many items were ever built for the list: a long diff builds the few on screen.
    pub fn built(&self) -> usize {
        self.built.get()
    }

    pub fn scroll(&self) -> &VirtualListScrollHandle {
        &self.scroll
    }

    fn item(&self, ix: usize, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        self.built.set(self.built.get() + 1);
        #[cfg(test)]
        BUILT.with(|built| *built.borrow_mut().entry(self.id.to_string()).or_default() += 1);
        match self.items.get(ix) {
            Some(Item::Row(row)) => self.row(*row, tokens, cx),
            Some(Item::Comment) => match &self.slot {
                Some(slot) => div()
                    .id(("diff-slot", 0usize))
                    .test_support()
                    .flex_shrink_0()
                    .w_full()
                    .h(slot.height)
                    .child(slot.view.clone())
                    .into_any_element(),
                None => div().into_any_element(),
            },
            None => div().into_any_element(),
        }
    }
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        // `.dbody`: the mono face at `--fs-sm`, which is smaller than the terminal's.
        let tokens = *Tokens::global(cx);
        let (family, text_size) = (theme.mono_font_family.clone(), pixels(tokens.fs_sm, window.rem_size()));
        // One cell is the advance of `M`, as the terminal panes measure it.
        let text = window.text_system();
        let font_id = text.resolve_font(&Font { family: family.clone(), ..Font::default() });
        self.advance = text.advance(font_id, text_size, 'M').map_or(text_size * 0.6, |advance| advance.width);
        self.zoom = zoom(window.rem_size());
        if self.wrap_at() != self.per_line || self.sized_at != self.zoom {
            self.per_line = self.wrap_at();
            self.size();
        }
        let (pane, area, diff) = (self.pane.clone(), self.area.clone(), cx.entity());
        let body = div()
            .id(SharedString::from(format!("{}-body", self.id)))
            .test_support()
            .relative()
            .size_full()
            .min_h_0()
            .bg(tokens.bg_sunken)
            .font_family(family)
            .text_size(text_size)
            .text_color(tokens.fg_muted)
            .child(
                // Where the list ends up is only known once it is laid out; a
                // change asks for one more render that reads it.
                canvas(
                    move |bounds, _, cx| {
                        area.set(bounds);
                        if pane.get() != bounds.size.width {
                            pane.set(bounds.size.width);
                            cx.defer(move |cx| diff.update(cx, |_, cx| cx.notify()));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                v_virtual_list(
                    cx.entity(),
                    SharedString::from(format!("{}-list", self.id)),
                    self.sizes.clone(),
                    move |this, range, _, cx| {
                        // Only what this build draws is on screen to be read.
                        this.layouts.borrow_mut().clear();
                        range.map(|ix| this.item(ix, &tokens, cx)).collect::<Vec<_>>()
                    },
                )
                .track_scroll(&self.scroll),
            )
            // The page's diff scrolls in a native box with both bars; without
            // them a mouse with no sideways wheel cannot reach a long line.
            // Laid over the list, as gpui-kit's table lays its own: a bar in
            // the flow would be given what is left under the list.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(Scrollbar::new(&self.scroll).viewport_from_layout().mode(ScrollbarMode::Always)),
            )
            // A held middle button drags the diff under the pointer, both ways.
            .on_mouse_down(MouseButton::Middle, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                this.begin_pan(event);
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| this.pan(event, cx)))
            .on_mouse_up(MouseButton::Middle, cx.listener(|this, _: &MouseUpEvent, _, _| this.end_pan()))
            .child(
                // What the drag takes the keys with, so Escape reaches it first.
                div()
                    .id(SharedString::from(format!("{}-keys", self.id)))
                    .size_0()
                    .track_focus(&self.keys)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| drag::escape(this, event, window, cx))),
            )
            .child(drag::watch(cx.weak_entity()));
        self.offer_copy(body, cx)
    }
}

#[cfg(test)]
mod tests;
