//! Dictation's keys: the push-to-talk chord held and let go, Escape, Enter and C while the panel
//! answers to them, and a chord being recorded in the Settings sheet.
//!
//! The chord is kept from a pane's program as the leader is: heard first at the root
//! (`keys::root`), so neither a binding nor the pane's text sees it. Letting it go is not a
//! keystroke, so it is heard three ways, any of which ends the run: the key's own key-up, the
//! modifiers' change, both heard only where the keys are under the shell, and a look at the
//! modifiers every few frames while it is held, which also ends it when the window loses the
//! keyboard to another window, as nothing else would.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::dictation::hold::Hold;
use argus_desktop_core::dictation::preferences::DictationChange;
use argus_desktop_core::dictation::run::Mode;
use argus_desktop_core::dictation::{DictationGesture, DictationPage};
use argus_desktop_core::keys::page_key::PageKey;
use gpui_kit::{canvas, prelude::*, App, Entity, FocusHandle, Global, KeyUpEvent, Keystroke, Modifiers, Task, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::page_key;

/// How often a held chord's modifiers are looked at.
const WATCH: Duration = Duration::from_millis(60);

/// The kit's name for the context a text input holds the keys in.
const INPUT: &str = "Input";

struct State {
    heard: Entity<Heard>,
    caller: Caller,
    /// The chord that is down, which started the run it holds.
    holding: Option<Hold>,
    /// The text field that had the keys at the press, which a run of its own types into.
    field: Option<FocusHandle>,
    /// The Settings sheet waits for a chord to keep.
    recording: bool,
    watching: Option<Task<()>>,
}

/// Dictation's keys. A handle: clones share one state.
#[derive(Clone)]
pub struct DictationKeys(Rc<RefCell<State>>);

impl Global for DictationKeys {}

impl DictationKeys {
    pub fn install(heard: Entity<Heard>, caller: Caller, cx: &mut App) {
        cx.set_global(DictationKeys(Rc::new(RefCell::new(State { heard, caller, holding: None, field: None, recording: false, watching: None }))));
    }

    pub fn get(cx: &App) -> Option<DictationKeys> {
        cx.try_global::<DictationKeys>().cloned()
    }

    fn page(&self, cx: &App) -> Option<Rc<DictationPage>> {
        let heard = self.0.borrow().heard.clone();
        heard.read(cx).dictation.clone()
    }

    fn call(&self, gesture: DictationGesture) {
        let caller = self.0.borrow().caller.clone();
        caller.call(Call::Dictation { gesture });
    }

    /// The Settings sheet's push-to-talk control waits for the next chord, or stops waiting.
    pub fn record(&self, on: bool) {
        self.0.borrow_mut().recording = on;
    }

    pub fn recording(cx: &App) -> bool {
        Self::get(cx).is_some_and(|keys| keys.0.borrow().recording)
    }

    /// The chord let go, if it was down.
    fn release(&self) {
        let held = {
            let mut state = self.0.borrow_mut();
            state.watching = None;
            state.holding.take()
        };
        if held.is_some() {
            self.call(DictationGesture::Release);
        }
    }

    /// Looks at the modifiers while the chord is held: one let go, or the window gone from under
    /// the keyboard, is the chord let go.
    fn watch(&self, window: &mut Window, cx: &mut App) {
        let keys = self.clone();
        let task = window.spawn(cx, async move |cx| loop {
            cx.background_executor().timer(WATCH).await;
            let held = cx.update(|window, _| {
                let state = keys.0.borrow();
                state.holding.as_ref().is_some_and(|hold| window.is_window_active() && holds(hold, &window.modifiers()))
            });
            if !matches!(held, Ok(true)) {
                keys.release();
                return;
            }
        });
        self.0.borrow_mut().watching = Some(task);
    }

    /// Types `text` into the field that had the keys at the run's press, as an input method's text
    /// is typed; a field that no longer has the keys holds the text in the panel instead.
    pub fn type_into(&self, run: u64, text: &str, window: &mut Window, cx: &mut App) {
        let field = self.0.borrow_mut().field.take();
        if !field.is_some_and(|field| field.is_focused(window)) {
            self.call(DictationGesture::TargetGone { run, text: text.to_owned() });
            return;
        }
        for typed in text.chars() {
            let key = typed.to_lowercase().to_string();
            window.dispatch_keystroke(Keystroke { key, key_char: Some(typed.to_string()), modifiers: Modifiers::default() }, cx);
        }
    }
}

/// Whether `modifiers` still hold every modifier the chord has.
fn holds(hold: &Hold, modifiers: &Modifiers) -> bool {
    (!hold.ctrl || modifiers.control) && (!hold.alt || modifiers.alt) && (!hold.shift || modifiers.shift) && (!hold.meta || modifiers.platform)
}

/// What the person's key does to dictation, heard before anything else at the root; whether it was
/// dictation's, so nothing else hears it.
pub fn heard(page_key: &PageKey, window: &mut Window, cx: &mut App) -> bool {
    let Some(keys) = DictationKeys::get(cx) else { return false };
    let Some(page) = keys.page(cx) else { return false };
    if keys.0.borrow().recording {
        return recorded(&keys, page_key, window);
    }
    let plain = !(page_key.ctrl || page_key.alt || page_key.meta);
    let chosen = &page.section.chosen;
    if chosen.enabled && chosen.hold.matches(page_key) {
        // The key repeating while it is held: already heard.
        if keys.0.borrow().holding.is_some() {
            return true;
        }
        let field = window.context_stack().iter().any(|context| context.contains(INPUT));
        {
            let mut state = keys.0.borrow_mut();
            state.holding = Some(chosen.hold.clone());
            state.field = if field { window.focused(cx) } else { None };
        }
        keys.call(DictationGesture::Press { mode: Mode::Hold, field });
        keys.watch(window, cx);
        return true;
    }
    let Some(panel) = &page.panel else { return false };
    let gesture = match page_key.key.as_str() {
        "Escape" if plain && (panel.open || panel.held || panel.opens_settings) => DictationGesture::Escape,
        "Enter" if plain && panel.held => DictationGesture::PasteHeld,
        "c" | "C" if plain && panel.held => DictationGesture::CopyHeld,
        "Enter" if plain && panel.opens_settings => {
            keys.call(DictationGesture::Dismiss { run: panel.run });
            crate::settings::preferences::open_at(crate::settings::preferences::DICTATION, window, cx);
            return true;
        }
        _ => return false,
    };
    keys.call(gesture);
    true
}

/// The next chord, kept as push-to-talk; Escape stops waiting for one.
fn recorded(keys: &DictationKeys, page_key: &PageKey, window: &mut Window) -> bool {
    keys.record(false);
    if page_key.key == "Escape" && !(page_key.ctrl || page_key.alt || page_key.meta || page_key.shift) {
        window.refresh();
        return true;
    }
    // A letter is kept as its key, whichever case Shift gave it.
    let key = if page_key.key.chars().count() == 1 { page_key.key.to_lowercase() } else { page_key.key.clone() };
    let hold = Hold { key, ctrl: page_key.ctrl, alt: page_key.alt, shift: page_key.shift, meta: page_key.meta };
    keys.call(DictationGesture::Set { change: DictationChange::Hold { hold } });
    window.refresh();
    true
}

/// An element that hears the chord let go where the keys are under the shell: its key's key-up,
/// or a modifier it has let go. It is put among the shell's children and draws nothing.
pub fn listener() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |_, _, window, _| {
            window.on_key_event(|event: &KeyUpEvent, _, _, cx| {
                let Some(keys) = DictationKeys::get(cx) else { return };
                let up = page_key::from_keystroke(&event.keystroke).map(|page| page.key);
                let lets_go = keys.0.borrow().holding.as_ref().is_some_and(|hold| up.as_deref().is_some_and(|up| up.eq_ignore_ascii_case(&hold.key)));
                if lets_go {
                    keys.release();
                }
            });
            window.on_modifiers_changed(|event, _, cx| {
                let Some(keys) = DictationKeys::get(cx) else { return };
                let lets_go = keys.0.borrow().holding.as_ref().is_some_and(|hold| !holds(hold, &event.modifiers));
                if lets_go {
                    keys.release();
                }
            });
        },
    )
    .absolute()
    .size_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chord_is_held_while_every_one_of_its_modifiers_is() {
        let hold = Hold::default();
        let both = Modifiers { control: true, shift: true, ..Modifiers::default() };
        assert!(holds(&hold, &both));
        assert!(!holds(&hold, &Modifiers { control: true, ..Modifiers::default() }), "Shift let go");
        assert!(!holds(&hold, &Modifiers::default()));
        let extra = Modifiers { control: true, shift: true, alt: true, ..Modifiers::default() };
        assert!(holds(&hold, &extra), "another modifier pressed meanwhile does not let it go");
    }
}
