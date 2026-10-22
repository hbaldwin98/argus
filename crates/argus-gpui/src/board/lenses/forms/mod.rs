//! The one form an open feature has open: which it is about (`Editing`) and the two fields it is
//! typed in, held across every push that redraws the feature (BOARD-FORM-1 to -4, -8).
//!
//! A pushed feature never rebuilds a field from a snapshot: the entity lives as long as the feature
//! view and a field keeps its text, caret and focus however often the outline is sent again. Saving
//! sends exactly one `Call::EditFeature`, puts the form away and draws nothing new until the pushed
//! board, tasks or flows say so, so a refused edit (which Rust answers with a notice) leaves the
//! feature as it was. A form whose row has gone from the outline is dropped (`edit::still_there`).
//!
//! The fields answer the keymap's Writing place before they type: Ctrl or Cmd Enter saves, a plain
//! Enter saves a one-line field and is a newline in a multi-line one, and Escape puts the form away.

pub mod edit;
pub mod view;

use argus_desktop_core::connection::page::{Call, FeatureView};
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::input::TextareaState;
use gpui_kit::{App, AppContext as _, Context, Entity, FocusHandle, Focusable as _, SharedString, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::field::{self, Field};
pub use edit::{Editing, Focus};

pub struct Form {
    heard: Entity<Heard>,
    caller: Caller,
    editing: Option<Editing>,
    /// The one-line field: a title, a because, the line of a new row.
    line: Field,
    /// The multi-line field: a brief, a description, a record, an answer, a source.
    area: Entity<TextareaState>,
    /// Where the keys go when the form closes: a field that is not drawn would otherwise keep them.
    rest: FocusHandle,
    _keys: [Registered; 2],
}

impl Form {
    pub fn new(heard: Entity<Heard>, caller: Caller, rest: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let line = Field::new("", window, cx);
        let area = cx.new(|cx| TextareaState::new(window, cx).rows(3).soft_wrap(true));
        let asked = cx.weak_entity();
        let one = line.input().read(cx).focus_handle(cx);
        let line_keys = Fields::of(cx).register_place(window, one, Place::Writing, {
            let asked = asked.clone();
            move |action, window, cx| answer(&asked, action, true, window, cx)
        });
        let many = area.focus_handle(cx);
        let area_keys = Fields::of(cx).register_place(window, many, Place::Writing, move |action, window, cx| answer(&asked, action, false, window, cx));
        Self { heard, caller, editing: None, line, area, rest, _keys: [line_keys, area_keys] }
    }

    pub fn editing(&self) -> Option<Editing> {
        self.editing
    }

    pub fn line(&self) -> &Field {
        &self.line
    }

    pub fn area(&self) -> &Entity<TextareaState> {
        &self.area
    }

    /// What the one-line field says now.
    pub fn line_text(&self, cx: &App) -> SharedString {
        self.line.text(cx)
    }

    /// What the multi-line field says now.
    pub fn area_text(&self, cx: &App) -> SharedString {
        self.area.read(cx).value()
    }

    /// Opens `editing`, which replaces any form already open, its fields filled and the first of
    /// them given the keys with the caret at the end of what it says.
    pub fn begin(&mut self, editing: Editing, view: &FeatureView, window: &mut Window, cx: &mut Context<Self>) {
        let fill = edit::fill(editing, view);
        self.editing = Some(editing);
        self.line.fill(&fill.line, window, cx);
        self.line.say_when_empty(fill.line_hint, window, cx);
        field::fill_area(&self.area, &fill.area, window, cx);
        self.area.update(cx, |area, cx| area.set_placeholder(fill.area_hint, window, cx));
        match fill.focus {
            Focus::Line => self.line.focus(window, cx),
            Focus::Area => self.area.update(cx, |area, cx| area.focus(window, cx)),
        }
        cx.notify();
    }

    /// Puts the form away unsent.
    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.take().is_some() {
            window.focus(&self.rest, cx);
            cx.notify();
        }
    }

    /// Sends what the form holds as one edit and puts it away. Nothing changes on screen until the
    /// pushed board says so.
    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.editing.take() else { return };
        let (line, area) = (self.line.text(cx), self.area_text(cx));
        if !self.heard.read(cx).held() {
            self.caller.call(Call::EditFeature { edit: edit::edit(editing, &line, &area) });
        }
        window.focus(&self.rest, cx);
        cx.notify();
    }

    /// Drops a form whose row is no longer on screen, as the pushed feature says; whether it did.
    pub fn follow(&mut self, view: &FeatureView, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match self.editing {
            Some(editing) if !edit::still_there(editing, view) => {
                self.cancel(window, cx);
                true
            }
            _ => false,
        }
    }
}

/// A key of the Writing place in one of the form's fields: whether the form ran it.
fn answer(form: &gpui_kit::WeakEntity<Form>, action: Action, one_line: bool, window: &mut Window, cx: &mut App) -> bool {
    form.update(cx, |form, cx| match action {
        Action::Save => {
            form.save(window, cx);
            true
        }
        Action::Choose if one_line => {
            form.save(window, cx);
            true
        }
        Action::CloseLayer => {
            form.cancel(window, cx);
            true
        }
        _ => false,
    })
    .unwrap_or(false)
}
