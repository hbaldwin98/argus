//! The change ruler, which is the file's scrollbar: each change and each search hit where it is in
//! the whole file, and the band of the file on screen as its thumb (`ruler`, `placeMarks`,
//! `trackView`, `grab`, `code/stage.ts`).
//!
//! Marks and band are measured off the rows as the list holds them (`DiffView::rows_span`), so a
//! removal marker or the comment box between lines moves them together rather than sliding one
//! past the other; while the editor stands in for the lines, off its lines, which never wrap. The arithmetic is core's (`code::ruler`); this reads the list's measures every
//! frame, so a scroll by any means moves the band. A press on the band holds it there, a press
//! elsewhere on the track centres the band on the pointer first, and the drag that follows goes the
//! decided 5 px before it is one; a mark is a button of its own and sends the line it stands for.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::code::ruler::{self, grab, mark, mark_title, scale, scrolled_to, Hold, Span};
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodeMark};
use argus_desktop_core::connection::page::Call;
use gpui_kit::{div, prelude::*, px, Bounds, Context, Entity, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Role, SharedString, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use super::editor::CodeEditor;
use crate::connection::heard::Heard;
use crate::kit::area;
use crate::kit::diff::DiffView;
use crate::kit::drag::{self, Drag, Dragger, Step};
use crate::kit::tip::tip;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// What the ruler is the scrollbar of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Over {
    Lines,
    Editor,
}

pub struct Ruler {
    heard: Entity<Heard>,
    caller: Caller,
    diff: Entity<DiffView>,
    editor: Entity<CodeEditor>,
    over: Over,
    marks: Vec<CodeMark>,
    /// Where the track was laid out, for the pointer's place on it and the height the file is scaled to.
    area: Rc<Cell<Bounds<Pixels>>>,
    drag: Drag<Hold>,
    keys: FocusHandle,
}

impl Ruler {
    pub fn new(heard: Entity<Heard>, caller: Caller, diff: Entity<DiffView>, editor: Entity<CodeEditor>, cx: &mut Context<Self>) -> Self {
        Self { heard, caller, diff, editor, over: Over::Lines, marks: Vec::new(), area: Rc::default(), drag: Drag::new(), keys: cx.focus_handle() }
    }

    /// The marks the session sends, over what stands for the file's lines.
    pub fn show(&mut self, marks: &[CodeMark], over: Over, cx: &mut Context<Self>) {
        if self.marks != marks || self.over != over {
            self.marks = marks.to_vec();
            self.over = over;
            cx.notify();
        }
    }

    /// How far down the file is, how much of it shows, and how tall it is, in pixels.
    fn measures(&self, cx: &gpui_kit::App) -> (f32, f32, f32) {
        match self.over {
            Over::Lines => {
                let diff = self.diff.read(cx);
                (diff.scrolled_px(), diff.viewport_px(), diff.content_px())
            }
            Over::Editor => self.editor.read(cx).measures(cx).map_or((0.0, 0.0, 0.0), |m| (m.scrolled, m.viewport, m.content)),
        }
    }

    /// Where lines `first` to `last` stand in the whole file.
    fn span(&self, first: usize, last: usize, cx: &gpui_kit::App) -> Option<(f32, f32)> {
        match self.over {
            Over::Lines => self.diff.read(cx).rows_span(first, last),
            Over::Editor => self.editor.read(cx).measures(cx).map(|m| m.span(first, last)),
        }
    }

    /// The band now, in the window's pixels, from the track's top.
    fn band(&self, tokens: &Tokens, window: &Window, cx: &gpui_kit::App) -> Span {
        let (scrolled, viewport, content) = self.measures(cx);
        let track = f32::from(self.area.get().size.height);
        let least = f32::from(pixels(tokens.ruler_view_min_h, window.rem_size()));
        ruler::band(scrolled, viewport, track, content, least)
    }

    /// Scrolls the file as the pointer at `y` (the window's) asks, holding the band as it was held.
    fn follow_pointer(&self, y: Pixels, hold: Hold, cx: &mut gpui_kit::App) {
        let (_, viewport, content) = self.measures(cx);
        let top = self.area.get().origin.y;
        let at = scrolled_to(f32::from(y - top), hold, f32::from(self.area.get().size.height), content, viewport);
        match self.over {
            Over::Lines => self.diff.read(cx).scroll_to_px(at),
            Over::Editor => self.editor.update(cx, |editor, cx| editor.scroll_to_px(at, cx)),
        }
    }

    fn ask(&self, gesture: CodeGesture, cx: &gpui_kit::App) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::Code { gesture });
        }
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
        let track = f32::from(self.area.get().size.height);
        let (_, _, content) = self.measures(cx);
        let scale = scale(track, content);
        let least = f32::from(pixels(tokens.ruler_mark_min_h, rem_size));
        let band = self.band(&tokens, window, cx);
        let marks: Vec<_> = self
            .marks
            .iter()
            .enumerate()
            .filter_map(|(at, found)| {
                let (first, last) = self.span(found.line as usize, (found.line + found.span.max(1) - 1) as usize, cx)?;
                Some((at, found.clone(), mark(first, last, scale, least)))
            })
            .collect();
        let dragging = self.drag.is_dragging();
        let (view, view_on) = (tokens.fg.opacity(tokens.share_ruler_view), tokens.fg.opacity(tokens.share_ruler_view_on));
        div()
            .id("code-ruler")
            .role(Role::ScrollBar)
            .test_support()
            .aria_label("Where this file changed")
            .group("code-ruler")
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
                        .id("code-view")
                        .test_support()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(band.top))
                        .h(px(band.height))
                        .bg(if dragging { view_on } else { view })
                        .group_hover("code-ruler", move |style| style.bg(view_on))
                        .border_t_1()
                        .border_b_1()
                        .border_color(tokens.line_strong),
                )
            })
            .children(marks.into_iter().map(|(at, found, span)| {
                let (title, label) = mark_title(found.kind, found.line);
                let line = found.line;
                let hit = found.kind == "hit";
                div()
                    .id(("code-mark", at))
                    .test_support()
                    .aria_label(SharedString::from(label))
                    .tooltip(tip(title))
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
                        _ => tokens.ok,
                    })
                    // A press on a mark is the mark's own click, not a grab of the band.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| this.ask(CodeGesture::Click { target: CodeClick::Go { line } }, cx)))
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
                    .id("code-ruler-keys")
                    .size_0()
                    .track_focus(&self.keys)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| drag::escape(this, event, window, cx))),
            )
            .child(drag::watch(cx.weak_entity()))
    }
}
