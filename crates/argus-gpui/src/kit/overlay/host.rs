//! The window's overlays, opened and closed in one place, over the kit's dialog layer.
//!
//! The kit keeps a stack of dialogs and closes the last. What it does not know is which of them
//! is the palette, that opening the settings must close it, that the stopped card answers no
//! Escape, or that a pane that had the keys is to have them back. This is that knowledge, over a
//! `Stack` that holds the rules. Every overlay is opened and closed through here, never by
//! `close_dialog` directly, so the two stacks agree.
//!
//! The kit runs a dialog's builder again whenever its layer is rebuilt, so what a builder makes
//! must not be a new editing state each time. A builder here is given whether this is its first
//! run, once, for what is done once (the field takes the keys). Escape, a press on the scrim and
//! the dialog's own confirm all come back through here; a dialog sets neither `on_cancel` nor
//! `on_close`, and says how it is dismissed in its [`Spec`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{App, FocusHandle, Global, Window};

use super::kind::Overlay;
use super::stack::{pane_regains_the_keys, Opening, Stack, Up};

/// What a dismissal runs before its overlay closes.
type Answer = Rc<dyn Fn(&mut Window, &mut App)>;

/// What Escape and a press on the scrim do to an overlay.
#[derive(Clone)]
pub enum Dismiss {
    /// Close it.
    Closes,
    /// Run this, then close it: a question answers no.
    Answers(Answer),
    /// Nothing: the stopped card.
    Never,
}

/// How an overlay opens.
#[derive(Clone)]
pub struct Spec {
    kind: Overlay,
    dismiss: Dismiss,
    keep_focus: bool,
}

impl Spec {
    pub fn new(kind: Overlay) -> Self {
        Self { kind, dismiss: Dismiss::Closes, keep_focus: false }
    }

    pub fn dismissed(mut self, dismiss: Dismiss) -> Self {
        self.dismiss = dismiss;
        self
    }

    /// Opens without taking the keys from what has them: an extension's question that arrived on
    /// its own is not to take them from a terminal being typed in (EXTDIALOG-7).
    pub fn keeping_focus(mut self) -> Self {
        self.keep_focus = true;
        self
    }
}

/// Builds a dialog; the last argument is whether this is the first time it has run.
type Build = Rc<dyn Fn(Dialog, &mut Window, &mut App, bool) -> Dialog>;

#[derive(Clone)]
struct Entry {
    spec: Spec,
    build: Build,
}

/// The pane that takes the keys back.
#[derive(Clone)]
struct Pane {
    focus: FocusHandle,
    shown: Rc<dyn Fn(&App) -> bool>,
}

#[derive(Default)]
struct State {
    stack: Stack,
    entries: Vec<Entry>,
    pane: Option<Pane>,
    /// Whether the pane had the keys when the first overlay that takes them opened.
    pane_had_keys: bool,
}

/// The window's overlays. A handle: clones share one state.
#[derive(Clone, Default)]
pub struct Overlays(Rc<RefCell<State>>);

impl Global for Overlays {}

impl Overlays {
    /// Makes the host the app's. Called once when the window starts.
    pub fn install(cx: &mut App) {
        cx.set_global(Overlays::default());
    }

    pub fn get(cx: &App) -> Self {
        cx.global::<Overlays>().clone()
    }

    /// The pane that gets the keys back when the dialogs that took them are gone, and whether a
    /// pane is on screen to type in.
    pub fn watch_pane(&self, focus: FocusHandle, shown: impl Fn(&App) -> bool + 'static) {
        self.0.borrow_mut().pane = Some(Pane { focus, shown: Rc::new(shown) });
    }

    pub fn is_open(&self, kind: Overlay) -> bool {
        self.0.borrow().stack.contains(kind)
    }

    /// The overlays up, bottom to top.
    pub fn open_kinds(&self) -> Vec<Overlay> {
        self.0.borrow().stack.kinds()
    }

    /// Opens an overlay, closing first what its kind is exclusive of. Opening one that is up
    /// changes nothing; whether it opened.
    pub fn open(
        &self,
        spec: Spec,
        build: impl Fn(Dialog, &mut Window, &mut App, bool) -> Dialog + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let up = Up { kind: spec.kind, dismissable: !matches!(spec.dismiss, Dismiss::Never) };
        // Before anything is closed: closing may hand the keys back to the pane.
        let before = self.pane_keys(window, cx);
        let opening = self.0.borrow().stack.clone().open(up);
        let Opening::Opened { closed } = opening else { return false };
        for kind in closed {
            self.close(kind, window, cx);
        }
        let entry = Entry { spec, build: Rc::new(build) };
        let taking = entry.spec.kind.takes_the_keys_from_a_pane();
        {
            let mut state = self.0.borrow_mut();
            if taking && !state.stack.takes_from_pane() {
                state.pane_had_keys = before;
            }
            state.stack.open(up);
            state.entries.push(entry.clone());
        }
        let focused = window.focused(cx);
        window.open_dialog(cx, self.builder(&entry));
        if entry.spec.keep_focus {
            if let Some(focused) = focused {
                window.focus(&focused, cx);
            }
        }
        true
    }

    /// Opens it, or closes it when it is up: the shortcuts sheet's `?` (SHELL-KEYS-13).
    pub fn toggle(
        &self,
        spec: Spec,
        build: impl Fn(Dialog, &mut Window, &mut App, bool) -> Dialog + 'static,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !self.close(spec.kind, window, cx) {
            self.open(spec, build, window, cx);
        }
    }

    /// Closes `kind`, wherever it is in the stack; whether it was up.
    pub fn close(&self, kind: Overlay, window: &mut Window, cx: &mut App) -> bool {
        let top = {
            let mut state = self.0.borrow_mut();
            let top = state.stack.is_top(kind);
            let Some(at) = state.stack.close(kind) else { return false };
            state.entries.remove(at);
            top
        };
        if top {
            window.close_dialog(cx);
        } else {
            self.rebuild(window, cx);
        }
        self.keys_back(window, cx);
        true
    }

    /// Escape: answers the topmost overlay that can be dismissed, and closes it. Whether there
    /// was one; false lets Escape go on to what is under the overlays.
    pub fn escape(&self, window: &mut Window, cx: &mut App) -> bool {
        let Some(kind) = self.0.borrow().stack.escape() else { return false };
        self.dismiss(kind, window, cx);
        self.close(kind, window, cx);
        true
    }

    /// Escape for one kind of overlay, wherever it is in the stack: answers it as dismissing does
    /// and closes it. Whether it was up and could be dismissed.
    pub fn escape_of(&self, kind: Overlay, window: &mut Window, cx: &mut App) -> bool {
        if !self.0.borrow().stack.dismissable(kind) {
            return false;
        }
        self.dismiss(kind, window, cx);
        self.close(kind, window, cx);
        true
    }

    /// Runs what dismissing `kind` says before it closes.
    fn dismiss(&self, kind: Overlay, window: &mut Window, cx: &mut App) {
        let answer = self.0.borrow().entries.iter().find(|entry| entry.spec.kind == kind).and_then(|entry| match &entry.spec.dismiss {
            Dismiss::Answers(answer) => Some(answer.clone()),
            _ => None,
        });
        if let Some(answer) = answer {
            answer(window, cx);
        }
    }

    /// The kit closed the top dialog itself, by its confirm or its cancel; the host's own books
    /// agree and a pane may have the keys back.
    fn closed_by_the_kit(&self, kind: Overlay, window: &mut Window, cx: &mut App) {
        {
            let mut state = self.0.borrow_mut();
            if let Some(at) = state.stack.close(kind) {
                state.entries.remove(at);
            }
        }
        self.keys_back(window, cx);
    }

    /// The kit's cancel (Escape, the scrim): whether it may close the dialog. It runs the dialog's
    /// answer first.
    fn cancelled(&self, kind: Overlay, window: &mut Window, cx: &mut App) -> bool {
        if !self.0.borrow().stack.dismissable(kind) {
            return false;
        }
        self.dismiss(kind, window, cx);
        true
    }

    /// The dialog layer's builder for `entry`: what the entry builds, held to the host's rules.
    fn builder(&self, entry: &Entry) -> impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static {
        let (host, entry, fresh) = (self.clone(), entry.clone(), Cell::new(true));
        move |dialog, window, cx| {
            let kind = entry.spec.kind;
            let dismissable = !matches!(entry.spec.dismiss, Dismiss::Never);
            let (cancelled, closed) = (host.clone(), host.clone());
            (entry.build)(dialog, window, cx, fresh.replace(false))
                .keyboard(true)
                .overlay_closable(dismissable)
                .close_button(false)
                .on_cancel(move |_, window, cx| cancelled.cancelled(kind, window, cx))
                .on_close(move |_, window, cx| closed.closed_by_the_kit(kind, window, cx))
        }
    }

    /// Closes every dialog and opens those still up again, in order: the kit closes only the
    /// last, and one under others was closed.
    fn rebuild(&self, window: &mut Window, cx: &mut App) {
        window.close_all_dialogs(cx);
        let entries = self.0.borrow().entries.clone();
        for entry in &entries {
            window.open_dialog(cx, self.builder(entry));
        }
    }

    fn pane_keys(&self, window: &Window, cx: &App) -> bool {
        self.0.borrow().pane.as_ref().is_some_and(|pane| pane.focus.contains_focused(window, cx))
    }

    /// Hands the keys back to the pane once no overlay that took them is left (SHELL-SEAM-10),
    /// and takes them from a pane nobody sees, which the kit's restore may have given them to.
    fn keys_back(&self, window: &mut Window, cx: &mut App) {
        let (pane, had, taken) = {
            let state = self.0.borrow();
            (state.pane.clone(), state.pane_had_keys, state.stack.takes_from_pane())
        };
        let Some(pane) = pane else { return };
        let shown = (pane.shown)(cx);
        if pane_regains_the_keys(had, taken, shown) {
            window.focus(&pane.focus, cx);
            self.0.borrow_mut().pane_had_keys = false;
        } else if !shown && pane.focus.is_focused(window) {
            window.blur(cx);
        }
    }
}

#[cfg(test)]
mod tests;
