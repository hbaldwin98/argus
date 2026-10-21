//! A diff drawn: file's rows in a virtual list, so only what is on screen is
//! built however long the diff is.
//!
//! The rows are `argus-desktop-core`'s `DiffRow`s, unified or split, and what
//! each one is (a hunk, a fold, a line, a pair; the cursor, the pick; what the
//! highlighter cut) is already said in them (`lines` makes the data of each).
//! This draws them and reports what was pressed: a click on a line, with
//! Shift held or not, and a fold's buttons. It sends no call and changes no
//! row, so a refused click leaves the diff as the session says it is.
//!
//! One diff view serves review and history, and the patch block and the Code
//! stage after them, so it knows none of their calls: a surface that does not
//! take clicks asks for a view with none. A surface may put one thing among
//! the rows, a slot of a height it names, under a row: the review's comment
//! box. The scroll offset stays across every update and the cursor's row is
//! brought into view when the rows change, "nearest" (SHARED-5): no movement
//! while it is already visible.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::{DiffRow, DiffSide};
use gpui_kit::component::{h_flex, ActiveTheme as _};
use gpui_kit::{
    div, prelude::*, px, AnyElement, AnyView, ClickEvent, Context, ElementId, EventEmitter, FontStyle, HighlightStyle,
    Pixels, ScrollStrategy, SharedString, Size, Styled, StyledText, TestSupportExt as _, Window,
};
use gpui_kit::component::{v_virtual_list, VirtualListScrollHandle};
use gpui_kit::size;

use crate::diff::lines::{self, Item, Shape};
use crate::theme::Ledger;

/// The width of a line-number column, and of the marker's.
const NUMBER: f32 = 44.0;
const MARKER: f32 = 16.0;
/// What a comment box's left edge clears: both number columns and the marker.
pub const GUTTER: f32 = NUMBER * 2.0 + MARKER + 4.0;

/// What a press on the diff means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffEvent {
    /// A line, pair or fold; `extend` is Shift held.
    Row { row: usize, extend: bool },
    /// One of a fold's buttons: a step of its lines, or all of them.
    Fold { row: usize, all: bool },
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
    rows: Rc<Vec<DiffRow>>,
    split: bool,
    slot: Option<Slot>,
    items: Rc<Vec<Item>>,
    sizes: Rc<Vec<Size<Pixels>>>,
    scroll: VirtualListScrollHandle,
    built: Cell<usize>,
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
    /// A view named `id`, which takes clicks when `interactive`.
    pub fn new(id: impl Into<SharedString>, interactive: bool) -> Self {
        Self {
            id: id.into(),
            interactive,
            rows: Rc::default(),
            split: false,
            slot: None,
            items: Rc::default(),
            sizes: Rc::default(),
            scroll: VirtualListScrollHandle::new(),
            built: Cell::new(0),
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
        }
        self.split = split;
        self.slot = slot;
        self.items = Rc::new(lines::items(&self.rows, self.slot.is_some()));
        let slot_height = self.slot.as_ref().map_or(px(0.0), |slot| slot.height);
        self.sizes = Rc::new(
            self.items
                .iter()
                .map(|item| match item {
                    Item::Row(i) => size(px(0.0), px(lines::height(&self.rows[*i]))),
                    Item::Comment => size(px(0.0), slot_height),
                })
                .collect(),
        );
        if !same_rows {
            if let Some(at) = lines::cursor_item(&self.rows, &self.items) {
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

    fn row_id(ix: usize) -> ElementId {
        ("diff-row", ix).into()
    }

    /// One row, `ix` of the rows.
    fn row(&self, ix: usize, ledger: &Ledger, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.rows[ix];
        let base = match lines::shape(row) {
            Shape::Hunk | Shape::Note => return self.heading(ix, row, ledger),
            Shape::Fold => return self.fold(ix, row, ledger, cx),
            Shape::Line => self.line(row, ledger),
            Shape::Pair => self.pair(row, ledger),
        };
        let (number, interactive) = (row.row, self.interactive);
        base.id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(px(lines::LINE))
            .w_full()
            .border_1()
            .border_color(if row.cursor || row.picked { ledger.line_strong } else { gpui_kit::transparent_black() })
            .when(interactive, |row| {
                row.cursor_text().on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                    cx.emit(DiffEvent::Row { row: number, extend: event.modifiers().shift });
                }))
            })
            .into_any_element()
    }

    /// A hunk's `@@` line or a note: said, never picked.
    fn heading(&self, ix: usize, row: &DiffRow, ledger: &Ledger) -> AnyElement {
        h_flex()
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(px(lines::HUNK))
            .w_full()
            .px_3()
            .items_center()
            .bg(ledger.chrome)
            .text_color(ledger.faint)
            .border_b_1()
            .border_color(ledger.line)
            .truncate()
            .child(SharedString::from(row.text.clone()))
            .into_any_element()
    }

    /// The lines between hunks the diff did not carry, and the buttons that
    /// ask for them. The cursor may rest on one, so it takes a click too.
    fn fold(&self, ix: usize, row: &DiffRow, ledger: &Ledger, cx: &mut Context<Self>) -> AnyElement {
        let number = row.row;
        let button = |id: &'static str, label: String, all: bool, cx: &mut Context<Self>| {
            div()
                .id((id, ix))
                .test_support()
                .aria_label(SharedString::from(label.clone()))
                .px_1()
                .h(px(22.0))
                .flex()
                .items_center()
                .rounded(px(3.0))
                .text_color(ledger.link)
                .cursor_pointer()
                .hover({
                    let raised = ledger.raised;
                    move |style| style.bg(raised)
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    // The row under it would take the click as a move first.
                    cx.stop_propagation();
                    cx.emit(DiffEvent::Fold { row: number, all });
                }))
                .child(SharedString::from(label))
        };
        h_flex()
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(px(lines::FOLD))
            .w_full()
            .px_3()
            .gap_2()
            .items_center()
            .bg(ledger.chrome)
            .text_color(ledger.muted)
            .text_sm()
            .border_t_1()
            .border_b_1()
            .border_dashed()
            .border_color(if row.cursor { ledger.line_strong } else { ledger.line })
            .child(SharedString::from(format!("\u{22ef} {}", row.text)))
            .when(self.interactive, |fold| {
                fold.children(match row.step {
                    Some(step) => vec![
                        button("diff-fold", format!("Show {step}"), false, cx),
                        button("diff-fold-all", "Show all".to_owned(), true, cx),
                    ],
                    None => vec![button("diff-fold", "Show".to_owned(), true, cx)],
                })
            })
            .when(self.interactive, |fold| fold.cursor_text())
            .when(self.interactive, |fold| {
                fold.on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                    cx.emit(DiffEvent::Row { row: number, extend: event.modifiers().shift });
                }))
            })
            .into_any_element()
    }

    /// A unified line: two numbers, a marker, then code.
    fn line(&self, row: &DiffRow, ledger: &Ledger) -> gpui_kit::Div {
        let side = row.left.as_ref();
        let kind = side.map_or("context", |side| side.kind);
        let fill = lines::fill(kind, row.picked, row.cursor, ledger);
        h_flex().items_center().when_some(fill, |row, fill| row.bg(fill)).child(half(side, true, ledger)).overflow_hidden()
    }

    /// A split pair: its two halves, each with its own number, marker and
    /// code, cut at its own half. A missing half is blank and recessed.
    fn pair(&self, row: &DiffRow, ledger: &Ledger) -> gpui_kit::Div {
        let side = |side: Option<&DiffSide>, second: bool| {
            let fill = match side {
                Some(side) => lines::fill(side.kind, row.picked, row.cursor, ledger),
                None => Some(ledger.chrome),
            };
            h_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .items_center()
                .overflow_hidden()
                .when(second, |half| half.border_l_1().border_color(ledger.line))
                .when_some(fill, |half, fill| half.bg(fill))
                .child(half(side, false, ledger))
        };
        h_flex().items_center().child(side(row.left.as_ref(), false)).child(side(row.right.as_ref(), true))
    }

    fn item(&self, ix: usize, ledger: &Ledger, cx: &mut Context<Self>) -> AnyElement {
        self.built.set(self.built.get() + 1);
        #[cfg(test)]
        BUILT.with(|built| *built.borrow_mut().entry(self.id.to_string()).or_default() += 1);
        match self.items.get(ix) {
            Some(Item::Row(row)) => self.row(*row, ledger, cx),
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

/// A line's number or numbers, its marker and its code.
fn half(side: Option<&DiffSide>, both: bool, ledger: &Ledger) -> impl IntoElement {
    let changed = side.is_some_and(|side| side.kind != "context");
    let number_colour = if changed { ledger.faint } else { ledger.fainter };
    let marker_colour = match side.map(|side| side.kind) {
        Some("added") => ledger.success,
        Some("removed") => ledger.danger,
        _ => ledger.faint,
    };
    let numbers = lines::numbers(side, both).into_iter().map(move |n| {
        div()
            .flex_shrink_0()
            .w(px(NUMBER))
            .pr_2()
            .text_right()
            .text_color(number_colour)
            .child(SharedString::from(n.map_or_else(String::new, |n| n.to_string())))
    });
    h_flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .children(numbers)
        .child(
            div()
                .flex_shrink_0()
                .w(px(MARKER))
                .text_center()
                .text_color(marker_colour)
                .child(SharedString::from(side.map_or("", |side| side.marker))),
        )
        .child(div().flex_1().min_w_0().truncate().children(side.map(|side| code(side, ledger))))
}

/// The side's text with its highlight kinds as colours.
fn code(side: &DiffSide, ledger: &Ledger) -> impl IntoElement {
    let drawn = lines::code(side);
    let highlights = drawn.spans.iter().filter_map(|(range, kind)| {
        let tone = lines::tone(kind, ledger)?;
        let style = HighlightStyle {
            color: Some(tone.colour),
            font_style: tone.italic.then_some(FontStyle::Italic),
            ..HighlightStyle::default()
        };
        Some((range.clone(), style))
    });
    StyledText::new(SharedString::from(drawn.text)).with_highlights(highlights)
}

impl Render for DiffView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (family, text_size) = (theme.mono_font_family.clone(), theme.mono_font_size);
        let ledger = *Ledger::global(cx);
        div()
            .id(SharedString::from(format!("{}-body", self.id)))
            .size_full()
            .min_h_0()
            .bg(ledger.background)
            .font_family(family)
            .text_size(text_size)
            .text_color(ledger.muted)
            .child(
                v_virtual_list(
                    cx.entity(),
                    SharedString::from(format!("{}-list", self.id)),
                    self.sizes.clone(),
                    move |this, range, _, cx| range.map(|ix| this.item(ix, &ledger, cx)).collect::<Vec<_>>(),
                )
                .track_scroll(&self.scroll),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{size, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions};

    fn line(row: usize) -> DiffRow {
        let side = DiffSide { old: Some(row as u32), new: Some(row as u32), marker: "", kind: "context", parts: vec![("x".into(), "")] };
        DiffRow { row, kind: "line", text: String::new(), hidden: None, step: None, left: Some(side), right: None, cursor: false, picked: false }
    }

    fn fold(row: usize) -> DiffRow {
        DiffRow { row, kind: "fold", text: "8 lines".into(), hidden: Some(8), step: None, left: None, right: None, cursor: false, picked: false }
    }

    struct Opened {
        handle: gpui_kit::AnyWindowHandle,
        view: Entity<DiffView>,
        pressed: Rc<RefCell<Vec<DiffEvent>>>,
        notified: Rc<Cell<u32>>,
    }

    fn open(cx: &mut TestAppContext, interactive: bool) -> Opened {
        cx.update(crate::window::init);
        let pressed: Rc<RefCell<Vec<DiffEvent>>> = Rc::default();
        let notified: Rc<Cell<u32>> = Rc::default();
        let mut built = None;
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(600.0), px(400.0))))),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    let view = cx.new(|_| DiffView::new("diff", interactive));
                    let sink = pressed.clone();
                    cx.subscribe(&view, move |_, event: &DiffEvent, _| sink.borrow_mut().push(*event)).detach();
                    let count = notified.clone();
                    cx.observe(&view, move |_, _| count.set(count.get() + 1)).detach();
                    built = Some(view.clone());
                    view
                },
            )
            .expect("open test window")
        });
        Opened { handle, view: built.unwrap(), pressed, notified }
    }

    fn show(cx: &mut TestAppContext, opened: &Opened, rows: &[DiffRow]) {
        cx.update(|cx| opened.view.update(cx, |view, cx| view.show(rows, false, None, cx)));
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }

    #[gpui_kit::test]
    fn a_diff_with_no_clicks_says_its_folds_without_buttons_and_reports_no_press(cx: &mut TestAppContext) {
        let opened = open(cx, false);
        show(cx, &opened, &[line(0), fold(1), line(2)]);
        cx.update_window(opened.handle, |_, window, cx| {
            assert!(window.try_find(("diff-row", 1usize)).is_some(), "the fold is still said");
            window.click(("diff-row", 0usize), cx);
            window.click(("diff-row", 1usize), cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert!(opened.pressed.borrow().is_empty(), "{:?}", opened.pressed.borrow());
        cx.update_window(opened.handle, |_, window, _| {
            assert!(window.try_find(("diff-fold", 1usize)).is_none(), "a view that takes no click offers no button");
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn a_diff_that_takes_clicks_reports_a_line_a_fold_and_a_shifted_line(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        show(cx, &opened, &[line(0), fold(1), line(2)]);
        cx.update_window(opened.handle, |_, window, cx| {
            window.click(("diff-row", 0usize), cx);
            window.click(("diff-fold", 1usize), cx);
            window.click_with_modifiers(("diff-row", 2usize), gpui_kit::Modifiers::shift(), cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(
            *opened.pressed.borrow(),
            [
                DiffEvent::Row { row: 0, extend: false },
                DiffEvent::Fold { row: 1, all: true },
                DiffEvent::Row { row: 2, extend: true },
            ]
        );
    }

    #[gpui_kit::test]
    fn a_long_diff_builds_only_the_rows_the_list_asks_for(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        let rows: Vec<DiffRow> = (0..20_000).map(line).collect();
        show(cx, &opened, &rows);
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        cx.update(|cx| {
            let view = opened.view.read(cx);
            assert_eq!(view.len(), 20_000);
            assert!(view.built() > 0 && view.built() < 300, "built {} items of 20000", view.built());
        });
    }

    #[gpui_kit::test]
    fn showing_what_is_drawn_already_redraws_nothing(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        let rows = [line(0), line(1)];
        show(cx, &opened, &rows);
        let before = opened.notified.get();
        show(cx, &opened, &rows);
        assert_eq!(opened.notified.get(), before, "the same rows said again repainted the list");
        show(cx, &opened, &[line(0)]);
        assert_eq!(opened.notified.get(), before + 1);
        cx.update(|cx| assert_eq!(opened.view.read(cx).len(), 1));
    }
}
