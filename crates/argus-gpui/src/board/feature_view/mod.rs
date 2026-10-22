//! A feature opened from the index (BOARD-HEAD, BOARD-WORK, BOARD-RECORD, BOARD-FLOWS): its header, the
//! lens it is read through, and the key bar, with the form being written held across every push.
//!
//! Core builds the feature (`FeatureView`) and the session holds the cursor of each lens, so this draws
//! what it says and sends what is pressed: a refused edit leaves the feature as it was, since it changes
//! only when the pushed board, tasks and flows do. What the window keeps of its own is the form being
//! written (`lenses::forms`), the legend, the option chosen in an open decision, the expanded diagram
//! and where each list is scrolled; they survive a push by being held here, and the form is dropped
//! when its row is gone.
//!
//! The Outline layer of the key walk is up while a feature is open, and the Diagram layer over it while
//! its flow is expanded (`keys`); what each key does is `act`. Escape puts away a form, then the legend,
//! then the expanded diagram, then the feature (BOARD-WORK-22).

pub mod act;
pub mod columns;
mod diagram;
pub mod expanded;
pub mod key_bar;
pub mod keys;
mod menus;
pub mod more;
pub mod scroll;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, FeatureEdit, Features, Lens, Stage};
use argus_desktop_core::keys::keymap::Layer;
use argus_desktop_core::layout::dragged_width::OUTLINE;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, px, Bounds, Context, Entity, FocusHandle, Pixels, Subscription, TestSupportExt as _, Window};

use crate::board::lenses::forms::{edit::Editing, Form};
use crate::board::lenses::sequence::{expanded as overlay, SequenceView};
use crate::board::lenses::{flows, head, record, work};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::layer::Layers;
use crate::kit::menu::at::At;
use crate::kit::split::Split;
use crate::layout::holders::Holder;
use crate::theme::scale::css;
use crate::theme::Tokens;
use act::Act;
pub use expanded::Expanded;
use scroll::{lit, Scrolls};

pub struct OpenFeature {
    pub(crate) heard: Entity<Heard>,
    pub(crate) caller: Caller,
    /// The one form open, whatever it is about.
    pub(crate) form: Entity<Form>,
    /// Where the keys rest while no field holds them.
    pub(crate) focus: FocusHandle,
    /// The sash between the outline and the detail, and the box it was laid out in.
    pub(crate) outline: Entity<Split>,
    pub(crate) outline_area: Rc<Cell<Bounds<Pixels>>>,
    /// Whether the legend is asked for.
    pub(crate) legend: bool,
    /// The feature whose brief is read whole, by its slug, and the box its brief's column was laid out
    /// in (`head::brief`).
    pub(crate) brief_read: Option<String>,
    pub(crate) brief_area: Rc<Cell<Bounds<Pixels>>>,
    /// The option chosen in an open decision, not yet taken: the decision and the option.
    pub(crate) chosen: Option<(i64, usize)>,
    /// The flow's diagram opened over the window, while it is.
    pub(crate) expanded: Option<Expanded>,
    pub(crate) scroll: Scrolls,
    /// The diagram inline in the Flows lens and the one expanded over the window, with the boxes they
    /// are laid out in.
    pub(crate) inline: Entity<SequenceView>,
    pub(crate) big: Entity<SequenceView>,
    pub(crate) holder_area: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) canvas_area: Rc<Cell<Bounds<Pixels>>>,
    /// The box of the lit step's row in the steps list, which the bind menu opens under.
    pub(crate) lit_row: Rc<Cell<Bounds<Pixels>>>,
    /// The size the canvas was when the zoom that fits it was drawn.
    pub(super) canvas_seen: Cell<(f32, f32)>,
    /// The bind menu or the flow switcher, while one is open.
    pub(super) menu: Option<At>,
    /// The cursor's row when the lists were last scrolled to it, and whether the diagram is still to be
    /// scrolled to the lit step once its boxes have been measured.
    lit: Option<scroll::Lit>,
    bring_pending: bool,
    /// The holder's width when the diagram was last brought to the lit step: the box settles over a frame
    /// or two as the panes split, and the step is brought again where it ends up (`fitted` runs once laid out).
    holder_seen: f32,
    /// Whether a feature was open at the last update, to take the keys once as it opens.
    was_open: bool,
    _following: Subscription,
}

impl OpenFeature {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tokens = *Tokens::global(cx);
        let focus = cx.focus_handle();
        let form = cx.new(|cx| Form::new(heard.clone(), caller.clone(), focus.clone(), window, cx));
        let (holding, reporting) = (heard.clone(), caller.clone());
        let outline = cx.new(|cx| {
            Split::new("work-sash", "Resize the outline", tokens.outline_usual_w, OUTLINE.min as f32, OUTLINE.max as f32, cx)
                .reporting(move |width, _, _| reporting.call(Call::OutlineWidth { width }))
                .holding(move |on, cx| {
                    holding.update(cx, |heard, cx| {
                        let changed = if on { heard.hold_resize(Holder::Outline) } else { heard.release_resize(Holder::Outline) };
                        if changed {
                            cx.notify();
                        }
                    });
                })
        });
        let (small, large) = (heard.clone(), heard.clone());
        let (inline_caller, big_caller) = (caller.clone(), caller.clone());
        let inline = cx.new(|cx| SequenceView::new("seq", false, small, inline_caller, cx));
        let big = cx.new(|cx| SequenceView::new("seq-big", true, large, big_caller, cx));
        let following = cx.observe_in(&heard, window, |this, heard, window, cx| this.follow(&heard, window, cx));
        Layers::of(cx).register(Layer::Outline, Rc::new(keys::OutlineKeys { feature: cx.weak_entity() }));
        Layers::of(cx).register(Layer::Diagram, Rc::new(keys::DiagramKeys { feature: cx.weak_entity() }));
        Self {
            heard,
            caller,
            form,
            focus,
            outline,
            outline_area: Rc::default(),
            legend: false,
            brief_read: None,
            brief_area: Rc::default(),
            chosen: None,
            expanded: None,
            scroll: Scrolls::default(),
            inline,
            big,
            holder_area: Rc::default(),
            canvas_area: Rc::default(),
            lit_row: Rc::default(),
            canvas_seen: Cell::new((0.0, 0.0)),
            menu: None,
            lit: None,
            bring_pending: false,
            holder_seen: 0.0,
            was_open: false,
            _following: following,
        }
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// The feature the session has open, with the page it is in.
    pub fn current(&self, cx: &gpui_kit::App) -> Option<Rc<Features>> {
        let heard = self.heard.read(cx);
        heard.features.clone().filter(|features| features.open.is_some() && heard.rail.stage == Stage::Feature)
    }

    /// Whether a feature is open and on screen: what the Outline layer is up for.
    pub fn is_open(&self, cx: &gpui_kit::App) -> bool {
        self.current(cx).is_some()
    }

    /// Whether the flow's diagram is expanded over the window and can be: the Diagram layer's.
    pub fn is_expanded(&self, cx: &gpui_kit::App) -> bool {
        self.expanded.is_some() && self.current(cx).is_some_and(|features| features.open.as_ref().is_some_and(|view| view.lens == Lens::Flows && view.flows.as_ref().is_some_and(|flows| flows.flow.is_some())))
    }

    /// Follows the session: the form is dropped when its row has gone, the expanded diagram when its
    /// flow has (while flows have not arrived it stays), the outline's sash takes the width Rust keeps,
    /// the keys come to the feature once as it opens, and the cursor's row is brought into view when it
    /// has moved.
    fn follow(&mut self, heard: &Entity<Heard>, window: &mut Window, cx: &mut Context<Self>) {
        let (features, rail) = {
            let heard = heard.read(cx);
            (heard.features.clone(), heard.rail.clone())
        };
        let open = features.as_ref().and_then(|features| features.open.as_ref()).filter(|_| rail.stage == Stage::Feature);
        let Some(view) = open else {
            self.expanded = None;
            self.legend = false;
            self.brief_read = None;
            self.lit = None;
            self.menu = None;
            if self.was_open {
                self.form.update(cx, |form, cx| form.cancel(window, cx));
            }
            self.was_open = false;
            cx.notify();
            return;
        };
        if !self.was_open {
            window.focus(&self.focus, cx);
        }
        self.was_open = true;
        self.form.update(cx, |form, cx| form.follow(view, window, cx));
        if self.expanded.is_some() {
            let loading = view.lens == Lens::Flows && view.flows.as_ref().is_some_and(|flows| !flows.loaded);
            let drawable = view.lens == Lens::Flows && view.flows.as_ref().is_some_and(|flows| flows.flow.is_some());
            if !drawable && !loading {
                self.expanded = None;
                self.menu = None;
            }
        }
        let tokens = *Tokens::global(cx);
        let drawn = match rail.outline_width {
            Some(width) => Some(width as f32),
            None => match self.outline_area.get().size.width {
                width if width > Pixels::ZERO => Some(css(width, window.rem_size()) * tokens.outline_share),
                _ => None,
            },
        };
        if let Some(width) = drawn {
            self.outline.update(cx, |split, cx| split.follow(width, cx));
        }
        let now = lit(view);
        if now != self.lit {
            if let Some(row) = &now {
                if self.lit.as_ref().map(|before| &before.identity) != Some(&row.identity) {
                    self.scroll.bring(row);
                    self.bring_pending = true;
                }
            }
            self.lit = now;
        }
        cx.notify();
    }
    /// Sends a call to the session; a held window (the daemon gone) sends nothing.
    pub fn send(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// Opens a form on what the feature says now, in place of any open.
    pub fn begin(&mut self, editing: Editing, window: &mut Window, cx: &mut Context<Self>) {
        let Some(features) = self.current(cx) else { return };
        let Some(view) = features.open.as_ref() else { return };
        self.form.update(cx, |form, cx| form.begin(editing, view, window, cx));
        if matches!(editing, Editing::Flow { .. }) {
            self.expanded = self.expanded.map(Expanded::with_panel);
        }
        cx.notify();
    }

    /// Opens what the cursor is on to be edited (BOARD-WORK-14).
    pub fn edit_cursor_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(features) = self.current(cx) else { return };
        if let Some(view) = features.open.as_ref() {
            self.begin(act::edit_row(view), window, cx);
        }
    }

    /// Opens the decision under the Record lens's cursor as its record.
    pub fn edit_cursor_decision(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(features) = self.current(cx) else { return };
        if let Some(decision) = features.open.as_ref().and_then(|view| view.record.as_ref()).and_then(|record| record.decision.as_ref()) {
            self.begin(Editing::Record { id: decision.id }, window, cx);
        }
    }

    /// Reads the feature through another lens: any form being written is put away first.
    pub fn lens(&mut self, lens: Lens, window: &mut Window, cx: &mut Context<Self>) {
        self.form.update(cx, |form, cx| form.cancel(window, cx));
        self.send(Call::SetLens { lens }, cx);
        cx.notify();
    }

    /// Reads the brief of feature `slug` whole, or back to its two lines.
    pub fn read_brief(&mut self, slug: &str, whole: bool, cx: &mut Context<Self>) {
        self.brief_read = whole.then(|| slug.to_owned());
        cx.notify();
    }

    pub fn toggle_legend(&mut self, cx: &mut Context<Self>) {
        self.legend = !self.legend;
        cx.notify();
    }

    /// Lights an option of an open decision: the choice is the window's until Take sends it.
    pub fn choose(&mut self, decision: i64, option: usize, cx: &mut Context<Self>) {
        self.chosen = Some((decision, option));
        cx.notify();
    }

    /// Puts the expanded diagram away; a step or source form being written goes with it, unasked
    /// (BOARD-SEQ-20).
    pub fn collapse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.expanded = None;
        self.menu = None;
        self.form.update(cx, |form, cx| form.cancel(window, cx));
        cx.notify();
    }

    /// The first crumb: the diagram away and the feature with it.
    pub fn close_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.collapse(window, cx);
        self.send(Call::OpenFeature { slug: None }, cx);
    }

    /// Says a notice the window knows from what the feature shows.
    fn say(&self, text: String, cx: &mut Context<Self>) {
        self.heard.update(cx, |heard, cx| {
            heard.hear(argus_desktop_core::connection::page::Event::Notice { text });
            cx.notify();
        });
    }
    /// Does what a key or a button comes to.
    pub fn act(&mut self, act: Act, window: &mut Window, cx: &mut Context<Self>) {
        match act {
            Act::Nothing => {}
            Act::Call(call) => self.send(call, cx),
            Act::Edit(edit) => self.send(Call::EditFeature { edit }, cx),
            Act::Begin(editing) => self.begin(editing, window, cx),
            Act::Lens(lens) => self.lens(lens, window, cx),
            Act::Say(text) => self.say(text, cx),
            Act::Expand => {
                self.expanded.get_or_insert_with(Expanded::default);
            }
            Act::Zoom(by) => self.expanded = self.expanded.map(|held| held.zoomed(by)),
            Act::ZoomReset => self.expanded = self.expanded.map(Expanded::actual),
            Act::Fit => self.expanded = self.expanded.map(Expanded::fitted),
            Act::Panel => self.expanded = self.expanded.map(Expanded::panelled),
            Act::Bind { expanded } => self.open_bind(None, expanded, window, cx),
            Act::MoveStep { diagram, step, to } => {
                self.send(Call::EditFeature { edit: FeatureEdit::MoveStep { diagram, step, to } }, cx);
                self.send(Call::SelectStep { diagram, step: Some(to - 1) }, cx);
            }
        }
        cx.notify();
    }

    /// Runs a key of the Outline or Diagram place against the feature as it stands; whether it did.
    pub fn run_key(&mut self, action: argus_desktop_core::keys::keymap::Action, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(features) = self.current(cx) else { return false };
        let Some(view) = features.open.as_ref() else { return false };
        match act::outline(view, self.is_expanded(cx), action) {
            Some(act) => {
                self.act(act, window, cx);
                true
            }
            None => false,
        }
    }

    /// Escape on an open feature: the form being written goes first, then the legend, then the
    /// expanded diagram, then the feature, back to the index (BOARD-WORK-22).
    pub fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.big.update(cx, |seq, cx| seq.cancel_drag(cx)) {
            window.focus(&self.focus, cx);
        } else if self.form.read(cx).editing().is_some() {
            self.form.update(cx, |form, cx| form.cancel(window, cx));
        } else if self.legend {
            self.legend = false;
        } else if self.expanded.is_some() {
            self.expanded = None;
        } else {
            self.send(Call::OpenFeature { slug: None }, cx);
        }
        cx.notify();
        true
    }
}

impl Render for OpenFeature {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let root = div().id("feature").test_support().key_context("Outline").track_focus(&self.focus).size_full().text_color(tokens.fg).text_size(crate::theme::scale::rem(tokens.fs_md));
        let Some(features) = self.current(cx) else { return root };
        let Some(view) = features.open.as_ref() else { return root };
        self.show_diagrams(view, window, cx);
        let holder_now = f32::from(self.holder_area.get().size.width);
        if holder_now != self.holder_seen && view.lens == Lens::Flows {
            self.holder_seen = holder_now;
            self.bring_pending = true;
        }
        if self.bring_pending && self.holder_area.get().size.width > Pixels::ZERO {
            self.bring_pending = false;
            self.bring_diagram(view, window, &tokens);
        }
        let me = cx.entity();
        let open = self.form.read(cx).editing();
        let body = match view.lens {
            Lens::Work => work::lens(self, view, open, &me, &tokens, cx),
            Lens::Record => record::lens(self, view, open, &me, &tokens, cx),
            Lens::Flows => flows::lens(self, view, open, &me, &tokens, cx),
        };
        let expanded = overlay::overlay(self, view, open, &me, window, &tokens, cx);
        root.child(
            v_flex()
                .size_full()
                .child(head::status_line(self, view, &me, open, window, &tokens, cx))
                .child(div().flex_1().min_h_0().child(body))
                .child(key_bar::key_bar(view.lens, &tokens)),
        )
        .children(expanded)
        .children(self.menu.as_ref().map(|menu| menu.element(window)))
        .min_h(px(0.0))
    }
}

#[cfg(test)]
mod tests;
