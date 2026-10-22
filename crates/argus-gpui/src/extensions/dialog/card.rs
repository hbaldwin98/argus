//! The card of a question an extension asks: what it holds while it is up (its fields' text, the lit
//! choice of a pick, where the keys are in it) and the answers it sends.
//!
//! The card is made once for a question and kept as long as it is open, so what is typed survives
//! the dialog layer building its content again and a page that only changed the count waiting
//! behind it (EXTDIALOG-2). It reads the count live and the question it was made for: a
//! replacement with the same id is not drawn again, as the page does not.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::extensions::ask::{form_answer, picked};
use argus_protocol::{ExtAnswer, ExtDialog, ExtDialogKind};
use gpui_kit::{App, Context, Entity, Focusable as _, FocusHandle, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::field::Field;
use argus_desktop_core::keys::keymap::{Action, Place};

pub struct AskView {
    pub(super) heard: Entity<Heard>,
    pub(super) caller: Caller,
    pub(super) id: u64,
    pub(super) extension: String,
    pub(super) dialog: ExtDialog,
    /// It answers a command the person ran, so it takes the keys as it opens.
    pub(super) prompted: bool,
    /// The card's body: a click anywhere in it gives the card the keys, and the layer is up only
    /// while something in it has them (EXTDIALOG-8).
    pub(super) card: FocusHandle,
    pub(super) cancel: FocusHandle,
    pub(super) yes: FocusHandle,
    /// A prompt's field, or each of a form's, in order.
    pub(super) fields: Vec<Field>,
    /// A pick's choices, each focusable, and the one lit.
    pub(super) rows: Vec<FocusHandle>,
    pub(super) lit: usize,
    /// Each field's Enter and Escape, ended with the card.
    _keys: Vec<Registered>,
    _observing: Subscription,
}

impl AskView {
    pub fn new(heard: Entity<Heard>, caller: Caller, page: &argus_desktop_core::extensions::dialog::ExtDialogPage, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dialog = page.dialog.clone().unwrap_or(ExtDialog { title: String::new(), kind: ExtDialogKind::Form { fields: Vec::new() }, answers: None });
        let fields: Vec<Field> = match &dialog.kind {
            ExtDialogKind::Prompt { initial, .. } => {
                let field = Field::new("", window, cx);
                field.put(initial, window, cx);
                vec![field]
            }
            ExtDialogKind::Form { fields } => fields
                .iter()
                .map(|f| {
                    let field = Field::new("", window, cx);
                    field.put(&f.initial, window, cx);
                    field
                })
                .collect(),
            _ => Vec::new(),
        };
        let rows = match &dialog.kind {
            ExtDialogKind::Pick { items, .. } => items.iter().map(|_| cx.focus_handle()).collect(),
            _ => Vec::new(),
        };
        let me = cx.entity();
        let keys = fields
            .iter()
            .map(|field| {
                let (me, focus) = (me.clone(), field.input().read(cx).focus_handle(cx));
                Fields::of(cx).register_place(window, focus, Place::Writing, move |action, window, cx| {
                    me.update(cx, |this, cx| match action {
                        Action::Choose | Action::Save => {
                            this.submit(window, cx);
                            true
                        }
                        Action::CloseLayer => {
                            this.cancel(window, cx);
                            true
                        }
                        _ => false,
                    })
                })
            })
            .collect();
        let observing = cx.observe(&heard, |_, _, cx| cx.notify());
        Self {
            heard,
            caller,
            id: page.id,
            extension: page.extension.clone(),
            dialog,
            prompted: page.prompted,
            card: cx.focus_handle(),
            cancel: cx.focus_handle(),
            yes: cx.focus_handle(),
            fields,
            rows,
            lit: 0,
            _keys: keys,
            _observing: observing,
        }
    }

    /// How many more questions wait behind this one, as the session says now.
    pub(super) fn waiting(&self, cx: &App) -> usize {
        self.heard.read(cx).extension_dialog.as_ref().filter(|page| page.id == self.id).map_or(0, |page| page.waiting)
    }

    /// Whether anything in the card has the keys.
    pub fn has_keys(&self, window: &Window, cx: &App) -> bool {
        self.card.contains_focused(window, cx) || self.cancel.is_focused(window) || self.yes.is_focused(window)
    }

    /// Takes the keys as the question opens when it answers a command the person ran: the first field,
    /// else a pick's first choice, else the yes button.
    pub fn take_keys(card: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let (prompted, field, row, yes) = {
            let this = card.read(cx);
            (this.prompted, this.fields.first().cloned(), this.rows.first().cloned(), this.yes.clone())
        };
        if !prompted {
            return;
        }
        match (field, row) {
            (Some(field), _) => field.focus(window, cx),
            (None, Some(row)) => window.focus(&row, cx),
            (None, None) => window.focus(&yes, cx),
        }
    }

    /// The answer the card holds now: confirm yes, the text, the lit choice, or the form's fields.
    fn answer_now(&self, cx: &App) -> Option<ExtAnswer> {
        match &self.dialog.kind {
            ExtDialogKind::Confirm { .. } => Some(ExtAnswer::Yes),
            ExtDialogKind::Prompt { .. } => self.fields.first().map(|field| ExtAnswer::Text(field.text(cx).to_string())),
            ExtDialogKind::Pick { items, .. } => picked(items, self.lit),
            ExtDialogKind::Form { fields } => {
                let values: Vec<String> = self.fields.iter().map(|field| field.text(cx).to_string()).collect();
                Some(form_answer(fields, &values))
            }
        }
    }

    /// Sends the answer the card holds, and takes the dialog away.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let answer = self.answer_now(cx);
        self.answered(answer, true, window, cx);
    }

    /// Puts the question away unanswered.
    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.answered(None, true, window, cx);
    }

    /// Answers the choice `at` at once, as a click on it does.
    pub fn choose(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.lit = at;
        self.submit(window, cx);
    }

    /// Moves the lit choice and its focus.
    pub fn step(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        self.lit = argus_desktop_core::extensions::ask::step(self.lit, self.rows.len(), by);
        if let Some(row) = self.rows.get(self.lit) {
            window.focus(row, cx);
        }
        cx.notify();
    }

    /// Sends `answer` for this question. `closes` is whether the dialog is taken away here, as an
    /// answer does; Escape and the scrim leave that to the host.
    pub(super) fn answered(&mut self, answer: Option<ExtAnswer>, closes: bool, window: &mut Window, cx: &mut Context<Self>) {
        super::settle(self.id, closes, window, cx);
        self.caller.call(Call::AnswerDialog { dialog: self.id, answer });
    }
}
