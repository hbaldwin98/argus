//! What the blocks of one view keep between draws that is not the person's to lose and so is not
//! `views/memory`'s: the entities and handles GPUI needs to be handed again and again (a Markdown
//! with its selection, a Copy button, a log's scroll) and the widths the boxes were measured at.
//!
//! A block is drawn from scratch each time, but an entity made anew each time would lose a text
//! selection under every redraw. They are kept by the key of the place that draws them, and let go
//! of when a draw does not ask for them (`sweep`): a tab switched away from, a block the agent
//! dropped.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{canvas, prelude::*, App, Entity, EntityId, ScrollHandle, Window};

use super::markdown::Markdown;
use super::pickers::Pickers;
use crate::connection::caller::Caller;
use crate::kit::copy::CopyButton;
use crate::kit::diff::DiffView;
use crate::theme::scale::css;

/// The width each measured box was last laid out at, in the page's pixels, by key.
#[derive(Clone, Default)]
pub struct Widths(Rc<RefCell<HashMap<String, f32>>>);

impl Widths {
    /// The width of the box under `key`; zero before it has been laid out once, which the grid
    /// rules read as one column.
    pub fn get(&self, key: &str) -> f32 {
        self.0.borrow().get(key).copied().unwrap_or(0.0)
    }

    /// Fills its parent without taking room, and tells the view named `repaint` when the parent's
    /// width changed by a pixel or more, so what depends on it is drawn again at the new width.
    pub fn watch(&self, key: String, repaint: EntityId) -> impl IntoElement {
        let store = self.0.clone();
        canvas(
            move |bounds, window, cx| {
                let now = css(bounds.size.width, window.rem_size());
                let before = store.borrow().get(&key).copied();
                if before.is_none_or(|before| (before - now).abs() >= 1.0) {
                    store.borrow_mut().insert(key, now);
                    cx.notify(repaint);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }
}

/// What a table's header clicks have sorted, by the block's path: the column and whether it runs
/// ascending. Shared by handle so a click can set it, and let go of when the view comes again, as
/// the page's is when it redraws the view.
#[derive(Clone, Default)]
pub struct Sorts(Rc<RefCell<HashMap<String, (usize, bool)>>>);

impl Sorts {
    pub fn get(&self, key: &str) -> Option<(usize, bool)> {
        self.0.borrow().get(key).copied()
    }

    pub fn set(&self, key: &str, column: usize, ascending: bool) {
        self.0.borrow_mut().insert(key.to_owned(), (column, ascending));
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}

/// How many lines a several-line field shows.
const AREA_ROWS: usize = 4;

#[derive(Default)]
pub struct Kept {
    markdowns: HashMap<String, Entity<Markdown>>,
    copies: HashMap<String, (Entity<CopyButton>, Rc<RefCell<String>>)>,
    scrolls: HashMap<String, ScrollHandle>,
    fields: HashMap<String, Entity<InputState>>,
    areas: HashMap<String, Entity<TextareaState>>,
    diffs: HashMap<String, Entity<DiffView>>,
    pub sorts: Sorts,
    pub widths: Widths,
    pub pickers: Pickers,
    /// The keys this draw asked for.
    asked: HashSet<String>,
}

impl Kept {
    /// The Markdown kept under `key`, made on the first ask and given `text` on every one.
    pub fn markdown(&mut self, key: &str, text: &str, muted: bool, caller: &Caller, cx: &mut App) -> Entity<Markdown> {
        self.asked.insert(key.to_owned());
        let entity = self.markdowns.entry(key.to_owned()).or_insert_with(|| {
            let (id, caller) = (key.to_owned(), caller.clone());
            cx.new(|cx| Markdown::new(id, text, caller, cx))
        });
        entity.update(cx, |markdown, cx| {
            markdown.set_text(text, cx);
            markdown.set_muted(muted, cx);
        });
        entity.clone()
    }

    /// The Copy button kept under `key`, reading `label`; it copies `text` as it is when it is pressed.
    pub fn copy(&mut self, key: &str, label: &str, text: &str, caller: &Caller, cx: &mut App) -> Entity<CopyButton> {
        self.asked.insert(key.to_owned());
        let (entity, held) = self.copies.entry(key.to_owned()).or_insert_with(|| {
            let held = Rc::new(RefCell::new(text.to_owned()));
            let (id, read, caller, label) = (key.to_owned(), held.clone(), caller.clone(), label.to_owned());
            (cx.new(|_| CopyButton::new(id, label, move |_| read.borrow().clone(), caller)), held)
        });
        text.clone_into(&mut held.borrow_mut());
        entity.clone()
    }

    /// The scroll handle kept under `key`, and whether it is new.
    pub fn scroll(&mut self, key: &str) -> (ScrollHandle, bool) {
        self.asked.insert(key.to_owned());
        let fresh = !self.scrolls.contains_key(key);
        (self.scrolls.entry(key.to_owned()).or_default().clone(), fresh)
    }

    /// The text field kept under `key`, made on the first ask with `text` in it and `placeholder`
    /// over it. `typed` is told what it says each time the person edits it, which is the only time it
    /// is written: what is drawn never writes into a field somebody may be typing in.
    pub fn field(&mut self, key: &str, placeholder: &str, text: &str, typed: impl Fn(&str, &mut App) + 'static, window: &mut Window, cx: &mut App) -> Entity<InputState> {
        self.asked.insert(key.to_owned());
        self.fields
            .entry(key.to_owned())
            .or_insert_with(|| {
                let placeholder = placeholder.to_owned();
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
                if !text.is_empty() {
                    input.update(cx, |input, cx| input.set_value(text, window, cx));
                }
                cx.subscribe(&input, move |input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        typed(&input.read(cx).value(), cx);
                    }
                })
                .detach();
                input
            })
            .clone()
    }

    /// The several-line field kept under `key`, as [`Kept::field`] keeps a one-line one.
    pub fn area(&mut self, key: &str, placeholder: &str, text: &str, typed: impl Fn(&str, &mut App) + 'static, window: &mut Window, cx: &mut App) -> Entity<TextareaState> {
        self.asked.insert(key.to_owned());
        self.areas
            .entry(key.to_owned())
            .or_insert_with(|| {
                let placeholder = placeholder.to_owned();
                let area = cx.new(|cx| TextareaState::new(window, cx).rows(AREA_ROWS).placeholder(placeholder));
                if !text.is_empty() {
                    area.update(cx, |area, cx| area.set_value(text, window, cx));
                }
                cx.subscribe(&area, move |area, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        typed(&area.read(cx).value(), cx);
                    }
                })
                .detach();
                area
            })
            .clone()
    }

    /// The diff view kept under `key`, which takes no clicks, so a diff an agent wrote keeps its scroll
    /// across a redraw as the review's does.
    pub fn diff(&mut self, key: &str, cx: &mut App) -> Entity<DiffView> {
        self.asked.insert(key.to_owned());
        self.diffs.entry(key.to_owned()).or_insert_with(|| cx.new(|cx| DiffView::new(key.to_owned(), false, false, cx))).clone()
    }

    /// Lets go of what the draw that just ended did not ask for.
    pub fn sweep(&mut self) {
        let asked = std::mem::take(&mut self.asked);
        self.markdowns.retain(|key, _| asked.contains(key));
        self.copies.retain(|key, _| asked.contains(key));
        self.scrolls.retain(|key, _| asked.contains(key));
        self.fields.retain(|key, _| asked.contains(key));
        self.areas.retain(|key, _| asked.contains(key));
        self.diffs.retain(|key, _| asked.contains(key));
        self.pickers.sweep();
    }

    /// How many entities and handles are held, for a test to see that a sweep lets go.
    pub fn held(&self) -> usize {
        self.markdowns.len() + self.copies.len() + self.scrolls.len() + self.fields.len() + self.areas.len() + self.diffs.len() + self.pickers.held()
    }
}

