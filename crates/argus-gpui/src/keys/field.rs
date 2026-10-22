//! The first layer of the walk, a field: wherever the keys are in something typed into, the
//! field's keys are its own.
//!
//! A pane's screen is a field of this layer, and its keys are the session's (`Call::Key`). A
//! field of the kit's has its own typing and nothing more, unless it registers the place it
//! answers (the jump field's `Jump`, the history filter's `Filter`, the comment box's `Writing`),
//! in which case that place's keys are run before the field's own handling of them, and a key
//! the place does not bind is typed. So there is one walk for every key, and a field is a layer
//! that holds what it hears and says what it ran.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, FocusHandle, Global, Window, WindowId};

use super::layer::{action_for, Handler, Press, Verdict};
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;

/// The kit's name for the context a text input holds the keys in.
const INPUT: &str = "Input";

/// What a registered field does with a key it has the keys for.
type Keys = Rc<dyn Fn(&Press, &mut Window, &mut App) -> Verdict>;

struct Entry {
    id: u64,
    /// A focus handle's id is only unique in its window, so the window is part of who it is.
    window: WindowId,
    focus: FocusHandle,
    /// The place the field answers, if it answers one.
    place: Option<Place>,
    keys: Keys,
}

#[derive(Default)]
struct State {
    entries: Vec<Entry>,
    next: u64,
}

/// The fields that answer keys of their own. A handle: clones share one list.
#[derive(Clone, Default)]
pub struct Fields(Rc<RefCell<State>>);

impl Global for Fields {}

/// A field's place in the list, until it is dropped with the view that owns the field.
pub struct Registered {
    fields: Rc<RefCell<State>>,
    id: u64,
}

impl Drop for Registered {
    fn drop(&mut self) {
        self.fields.borrow_mut().entries.retain(|entry| entry.id != self.id);
    }
}

impl Fields {
    pub fn of(cx: &mut App) -> Self {
        cx.default_global::<Fields>().clone()
    }

    /// `keys` hears every key while `focus` has the keys: a pane's screen, which hands them on whole.
    pub fn register(
        &self,
        window: &Window,
        focus: FocusHandle,
        keys: impl Fn(&Press, &mut Window, &mut App) -> Verdict + 'static,
    ) -> Registered {
        self.add(window, focus, None, Rc::new(keys))
    }

    fn add(&self, window: &Window, focus: FocusHandle, place: Option<Place>, keys: Keys) -> Registered {
        let mut state = self.0.borrow_mut();
        state.next += 1;
        let id = state.next;
        state.entries.push(Entry { id, window: window.window_handle().window_id(), focus, place, keys });
        Registered { fields: self.0.clone(), id }
    }

    /// The places the registered fields answer.
    pub fn places(&self) -> Vec<Place> {
        self.0.borrow().entries.iter().filter_map(|entry| entry.place).collect()
    }

    /// A field that answers the bindings of `place`: `answer` is told the action, and says
    /// whether it ran it. A key the place does not bind, or `answer` leaves, is the field's to type.
    pub fn register_place(
        &self,
        window: &Window,
        focus: FocusHandle,
        place: Place,
        answer: impl Fn(Action, &mut Window, &mut App) -> bool + 'static,
    ) -> Registered {
        let keys = move |press: &Press, window: &mut Window, cx: &mut App| match action_for(&[place], press.page) {
            Some(action) if answer(action, window, cx) => Verdict::Taken,
            _ => Verdict::Held,
        };
        self.add(window, focus, Some(place), Rc::new(keys))
    }

    fn focused(&self, window: &Window) -> Option<Keys> {
        let here = window.window_handle().window_id();
        self.0.borrow().entries.iter().find(|entry| entry.window == here && entry.focus.is_focused(window)).map(|entry| entry.keys.clone())
    }
}

impl Handler for Fields {
    fn open(&self, window: &Window, cx: &App) -> bool {
        self.focused(window).is_some()
            || window.context_stack().iter().any(|context| context.contains(INPUT))
            // The palette's search field has the keys while it is open (`Place::Palette`); until it
            // is drawn, its stand-in holds them so no key reaches the stage under it.
            || Overlays::get(cx).is_open(Overlay::Palette)
    }

    fn keys(&self, press: &Press, _: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some(keys) = self.focused(window) else { return Verdict::Held };
        match keys(press, window, cx) {
            Verdict::Taken => Verdict::Taken,
            _ => Verdict::Held,
        }
    }
}

#[cfg(test)]
mod tests;
