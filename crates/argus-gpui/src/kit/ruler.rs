//! A change ruler, which is a list's scrollbar: each change where it is in the whole list, and the
//! band of the list on screen as its thumb (`ruler`, `placeMarks`, `trackView`, `grab`,
//! `code/stage.ts`; the Changes prototype's `.ruler`). The Code stage's file and the Changes pane's
//! diff each draw one.
//!
//! What it measures is its owner's (`Track`): the rows as a list holds them, so a removal marker or
//! a comment box between lines moves the marks with them rather than sliding one past the other.
//! The arithmetic is core's (`code::ruler`); this reads the measures every frame, so a scroll by any
//! means moves the band. A press on the band holds it there, a press elsewhere on the track centres
//! the band on the pointer first, and the drag that follows goes the decided 5 px before it is one;
//! a mark is a button of its own and its owner says what it does.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::code::ruler::{self, grab, mark, scale, scrolled_to, Hold, Span};
use gpui_kit::{div, prelude::*, px, App, Bounds, Context, Entity, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Role, SharedString, TestSupportExt as _, Window};

use crate::kit::area;
use crate::kit::diff::DiffView;
use crate::kit::drag::{self, Drag, Dragger, Step};
use crate::kit::tip::tip;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// What a ruler is the scrollbar of.
pub trait Track {
    /// How far down the list is, how much of it shows, and how tall it is, in pixels.
    fn measures(&self, cx: &App) -> (f32, f32, f32);
    /// Where rows `first` to `last`, by their numbers, stand in the whole list.
    fn span(&self, first: usize, last: usize, cx: &App) -> Option<(f32, f32)>;
    /// Scrolls the list to `y` from its top.
    fn scroll_to(&self, y: f32, cx: &mut App);
}

impl Track for Entity<DiffView> {
    fn measures(&self, cx: &App) -> (f32, f32, f32) {
        let diff = self.read(cx);
        (diff.scrolled_px(), diff.viewport_px(), diff.content_px())
    }

    fn span(&self, first: usize, last: usize, cx: &App) -> Option<(f32, f32)> {
        self.read(cx).rows_span(first, last)
    }

    fn scroll_to(&self, y: f32, cx: &mut App) {
        self.read(cx).scroll_to_px(y);
    }
}

/// A mark: the rows it stands for, by their numbers, what it is (`added`, `removed`, `mixed` or
/// `hit`), and its words.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub first: usize,
    pub last: usize,
    pub kind: &'static str,
    pub title: SharedString,
    pub label: SharedString,
}

/// The ids a ruler's parts are found by, and what a screen reader calls it.
#[derive(Debug, Clone, Copy)]
pub struct Ids {
    pub ruler: &'static str,
    pub band: &'static str,
    pub mark: &'static str,
    pub label: &'static str,
}

/// What a press on a mark does.
pub type Pressed = Rc<dyn Fn(&Mark, &mut App)>;

pub struct Ruler {
    ids: Ids,
    track: Rc<dyn Track>,
    pressed: Pressed,
    marks: Vec<Mark>,
    /// Where the track was laid out, for the pointer's place on it and the height the list is scaled to.
    area: Rc<Cell<Bounds<Pixels>>>,
    drag: Drag<Hold>,
    keys: FocusHandle,
}

impl Ruler {
    pub fn new(ids: Ids, track: Rc<dyn Track>, pressed: Pressed, cx: &mut Context<Self>) -> Self {
        Self { ids, track, pressed, marks: Vec::new(), area: Rc::default(), drag: Drag::new(), keys: cx.focus_handle() }
    }

    /// The marks, over `track`.
    pub fn show(&mut self, marks: Vec<Mark>, track: Rc<dyn Track>, cx: &mut Context<Self>) {
        if self.marks != marks || !Rc::ptr_eq(&self.track, &track) {
            self.marks = marks;
            self.track = track;
            cx.notify();
        }
    }

    /// The band now, in the window's pixels, from the track's top.
    fn band(&self, tokens: &Tokens, window: &Window, cx: &App) -> Span {
        let (scrolled, viewport, content) = self.track.measures(cx);
        let track = f32::from(self.area.get().size.height);
        let least = f32::from(pixels(tokens.ruler_view_min_h, window.rem_size()));
        ruler::band(scrolled, viewport, track, content, least)
    }

    /// Scrolls the list as the pointer at `y` (the window's) asks, holding the band as it was held.
    fn follow_pointer(&self, y: Pixels, hold: Hold, cx: &mut App) {
        let (_, viewport, content) = self.track.measures(cx);
        let top = self.area.get().origin.y;
        let at = scrolled_to(f32::from(y - top), hold, f32::from(self.area.get().size.height), content, viewport);
        self.track.scroll_to(at, cx);
    }
}

impl Dragger for Ruler {
    type Payload = Hold;

    fn drag(&self) -> &Drag<Hold> {
        &self.drag
    }

    fn drag_mut(&mut self) -> &mut Drag<Hold> {
        &mut self.drag
    }

    fn keys(&self) -> &FocusHandle {
        &self.keys
    }

    fn dragged(&mut self, step: Step<Hold>, _: &mut Window, cx: &mut Context<Self>) {
        match step {
            Step::Began(hold) | Step::Moved(hold) => {
                if let Some(at) = self.drag.at() {
                    self.follow_pointer(at.y, hold, cx);
                }
            }
            // The press already put the band under the pointer; nothing travelled, nothing is dropped.
            Step::Clicked(_) | Step::Dropped(..) | Step::Cancelled(_) => {}
        }
    }
}

impl Render for Ruler {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let rem_size = window.rem_size();
        let ids = self.ids;
        let track = f32::from(self.area.get().size.height);
        let (_, _, content) = self.track.measures(cx);
        let scale = scale(track, content);
        let least = f32::from(pixels(tokens.ruler_mark_min_h, rem_size));
        let band = self.band(&tokens, window, cx);
        let marks: Vec<_> = self
            .marks
            .iter()
            .enumerate()
            .filter_map(|(at, found)| {
                let (first, last) = self.track.span(found.first, found.last, cx)?;
                Some((at, found.clone(), mark(first, last, scale, least)))
            })
            .collect();
        let dragging = self.drag.is_dragging();
        let (view, view_on) = (tokens.fg.opacity(tokens.share_ruler_view), tokens.fg.opacity(tokens.share_ruler_view_on));
        div()
            .id(ids.ruler)
            .role(Role::ScrollBar)
            .test_support()
            .aria_label(ids.label)
            .group(ids.ruler)
            .relative()
            .flex_shrink_0()
            .w(rem(tokens.ruler_w))
            .h_full()
            .border_l_1()
            .border_color(tokens.line)
            .bg(tokens.bg_chrome)
            .child(area::measure(self.area.clone()))
            .when(content > 0.0, |track| {
                track.child(
                    div()
                        .id(ids.band)
                        .test_support()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(band.top))
                        .h(px(band.height))
                        .bg(if dragging { view_on } else { view })
                        .group_hover(ids.ruler, move |style| style.bg(view_on))
                        .border_t_1()
                        .border_b_1()
                        .border_color(tokens.line_strong),
                )
            })
            .children(marks.into_iter().map(|(at, found, span)| {
                let hit = found.kind == "hit";
                let pressed = self.pressed.clone();
                div()
                    .id((ids.mark, at))
                    .test_support()
                    .aria_label(found.label.clone())
                    .tooltip(tip(found.title.clone()))
                    .absolute()
                    .top(px(span.top))
                    .h(px(if hit { span.height.min(least) } else { span.height }))
                    .left(rem(if hit { tokens.sp_4 } else { tokens.ruler_mark_inset }))
                    .right(rem(if hit { tokens.sp_4 } else { tokens.ruler_mark_inset }))
                    .rounded(rem(tokens.hairline))
                    .cursor_pointer()
                    .bg(match found.kind {
                        "removed" => tokens.danger,
                        "hit" => tokens.attention,
                        "mixed" => tokens.changed,
                        _ => tokens.ok,
                    })
                    // A press on a mark is the mark's own click, not a grab of the band.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, _, cx| pressed(&found, cx))
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    let tokens = *Tokens::global(cx);
                    let band = this.band(&tokens, window, cx);
                    let hold = grab(f32::from(event.position.y - this.area.get().origin.y), band);
                    if !hold.on_band {
                        this.follow_pointer(event.position.y, hold, cx);
                    }
                    this.drag.press(event.position, hold);
                    cx.notify();
                }),
            )
            .child(
                div()
                    .size_0()
                    .track_focus(&self.keys)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| drag::escape(this, event, window, cx))),
            )
            .child(drag::watch(cx.weak_entity()))
    }
}
