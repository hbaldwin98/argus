//! The add-a-project dialog (SHELL-ADD-1..11): a folder browsed on the daemon's machine, what
//! adding it would find, and what to call the project.
//!
//! The session holds the dialog (`Heard::add_project`) and says everything on it: the folder and its
//! listing, the preview, the default name, why Add is off, a refusal. This draws that page and sends
//! the gestures of `AddProjectGesture`; it predicts none of their results. What it keeps is the
//! person's typing. A field keeps what is typed over what was sent (the session's echo of a letter can
//! land after the next), the path is written only while its field has not the keys, and the name, skip
//! and new-folder fields are sent after a pause of `FIELDS_PAUSE` rather than a call per letter. The
//! fields are the view's own entities, so the dialog layer rebuilding its builder loses nothing.
//!
//! The dialog is an overlay of the host (`kit/overlay`) that takes the keys from a pane. It is up
//! exactly while the session holds it, except that Escape, Cancel and the close button send `Close`
//! and, as nothing can refuse that, put it away without waiting for the page.

mod browse;
mod outcome;

use argus_desktop_core::chrome::add_outcome::{add_label, can_add, footer, FIELDS_PAUSE};
use argus_desktop_core::chrome::add_project::{AddKind, AddProjectGesture, AddProjectPage};
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, Context, Entity, Focusable as _, Role, SharedString, Subscription, Task, TestSupportExt as _, Window};
use std::rc::Rc;

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::button::{button, off, Look};
use crate::kit::dialog::{esc, foot, head, why};
use crate::kit::field::Field;
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::kit::tip::tip;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

pub struct AddView {
    heard: Entity<Heard>,
    caller: Caller,
    path: Field,
    name: Field,
    skip: Field,
    folder: Field,
    /// The fields' send, waiting out the pause. A later edit replaces it; Add and Close drop it.
    pending: Option<Task<()>>,
    /// The dialog was put away by a `Close` the page has not yet answered, so a page still open in
    /// the events before the answer does not bring it back.
    dismissed: bool,
    _subscriptions: Vec<Subscription>,
    /// The path field's Enter, which browses to what is typed.
    _path_keys: Registered,
}

impl AddView {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let path = Field::new("", window, cx);
        let name = Field::new("", window, cx);
        let skip = Field::new(argus_desktop_core::chrome::add_outcome::SKIP_PLACEHOLDER, window, cx);
        let folder = Field::new(argus_desktop_core::chrome::add_outcome::NEW_FOLDER_PLACEHOLDER, window, cx);
        let mut subscriptions = vec![cx.observe_in(&heard, window, |this, _, window, cx| this.follow(window, cx))];
        for field in [&name, &skip, &folder] {
            subscriptions.push(cx.subscribe(field.input(), |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.schedule(cx);
                }
            }));
        }
        // `writing`'s Enter in the path field browses to what is typed (SHELL-ADD-4).
        let typed = path.clone();
        let (me, asked) = (cx.weak_entity(), caller.clone());
        let keys = Fields::of(cx).register_place(window, path.input().read(cx).focus_handle(cx), Place::Writing, move |action, _, cx| {
            if action != Action::Choose {
                return false;
            }
            asked.call(Call::AddProject { gesture: AddProjectGesture::Browse { path: typed.text(cx).to_string() } });
            me.update(cx, |_, _| {}).ok();
            true
        });
        Self { heard, caller, path, name, skip, folder, pending: None, dismissed: false, _subscriptions: subscriptions, _path_keys: keys }
    }

    /// What the session says the dialog is.
    pub(super) fn page(&self, cx: &App) -> Option<AddProjectPage> {
        self.heard.read(cx).add_project.clone()
    }

    /// Brings the dialog up and takes it away as the session holds it, and lets its fields follow
    /// what the page says of them.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = Overlays::get(cx);
        let Some(page) = self.page(cx) else {
            self.dismissed = false;
            self.pending = None;
            host.close(Overlay::AddProject, window, cx);
            for field in [&self.path, &self.name, &self.skip, &self.folder] {
                field.put("", window, cx);
            }
            return;
        };
        // A folder newly listed shows its path; one being typed keeps the typing (SHELL-ADD-4).
        self.path.show(page.path.as_deref().unwrap_or(""), window, cx);
        self.name.say_when_empty(&page.default_name, window, cx);
        if !host.is_open(Overlay::AddProject) && !self.dismissed {
            self.open(window, cx);
        }
        cx.notify();
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (view, path, closing) = (cx.entity(), self.path.clone(), cx.entity());
        Overlays::get(cx).open(
            Spec::new(Overlay::AddProject).dismissed(Dismiss::Answers(Rc::new(move |_, cx| {
                closing.update(cx, |this, cx| this.send_close(cx));
            }))),
            move |dialog: Dialog, window, cx, first| {
                // A dialog newly opened has the path field (SHELL-ADD-2), once.
                if first {
                    path.focus(window, cx);
                }
                let tokens = *Tokens::global(cx);
                dialog
                    .w(pixels(tokens.form_w, window.rem_size()))
                    .p_0()
                    // Enter in a field is no confirm of the dialog: nothing but Add adds.
                    .on_ok(|_, _, _| false)
                    .child(view.clone())
            },
            window,
            cx,
        );
    }

    fn send(&self, gesture: AddProjectGesture) {
        self.caller.call(Call::AddProject { gesture });
    }

    /// What the name, skip and new-folder fields say now: their own text, which an echo from the
    /// session never replaces (SHELL-ADD-8).
    fn fields_now(&self, cx: &App) -> (String, String, String) {
        (self.name.text(cx).to_string(), self.skip.text(cx).to_string(), self.folder.text(cx).to_string())
    }

    /// Typing waits out its pause before it is sent: a changed skip scans the folder again.
    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.page(cx).is_none() {
            return;
        }
        self.pending = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FIELDS_PAUSE).await;
            this.update(cx, |this, cx| {
                this.pending = None;
                let (name, skip, folder) = this.fields_now(cx);
                this.send(AddProjectGesture::Fields { name, skip, folder });
            })
            .ok();
        }));
    }

    /// Add, with what the fields say as it is pressed: a press cancels the pause, since calls need
    /// not arrive in the order sent (SHELL-ADD-9).
    fn add(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.page(cx) else { return };
        if !can_add(&page) {
            return;
        }
        self.pending = None;
        let (name, skip, folder) = self.fields_now(cx);
        self.send(AddProjectGesture::Add { name, skip, folder });
    }

    /// Close: the close button, Cancel and Escape. The page is told, and the dialog goes.
    fn send_close(&mut self, _: &mut Context<Self>) {
        self.pending = None;
        self.dismissed = true;
        self.send(AddProjectGesture::Close);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.send_close(cx);
        Overlays::get(cx).close(Overlay::AddProject, window, cx);
    }

}

impl Render for AddView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.page(cx) else { return div().into_any_element() };
        let name = add_label(&self.name.text(cx), &page);
        let me = cx.entity();
        let closing = cx.listener(|this, _, window, cx| this.close(window, cx));
        let shut = me.clone();
        let adding = if can_add(&page) {
            button("ap-add", name.clone(), Look::Primary, false, &tokens).child(SharedString::from(name)).on_click(cx.listener(|this, _, _, cx| this.add(cx))).into_any_element()
        } else {
            off("ap-add", name.clone(), false, &tokens).child(SharedString::from(name)).when_some(page.blocked.clone(), |node, reason| node.tooltip(tip(reason))).into_any_element()
        };
        v_flex()
            .id("add-project")
            .role(Role::Dialog)
            .test_support()
            .aria_label("Add a project")
            .max_h(rem(tokens.add_max_h))
            .child(head("ap", "Add a project", move |window, cx| shut.update(cx, |this, cx| this.close(window, cx)), None, &tokens))
            .child(
                v_flex()
                    .id("ap-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap(rem(tokens.sp_12))
                    .px(rem(tokens.sp_24))
                    .pb(rem(tokens.sp_24))
                    .child(self.kinds(&page, &tokens, cx))
                    .child(self.path_row(&page, &tokens, cx))
                    .child(
                        v_flex()
                            .gap(rem(tokens.sp_12))
                            .child(self.folders(&page, &tokens, cx))
                            .child(self.outcome(&page, &tokens, cx)),
                    )
                    .child(self.name_row(&page, &tokens))
                    .children((page.kind == AddKind::Folder).then(|| self.skip_row(&page, &tokens)))
                    .children(page.refusal.clone().map(|refusal| {
                        div().id("ap-refusal").role(Role::Alert).test_support().aria_label(SharedString::from(refusal.clone())).text_color(tokens.danger).child(SharedString::from(refusal))
                    })),
            )
            .child(
                foot(&tokens)
                    .child(why(footer(page.kind), &tokens))
                    .child(
                        button("ap-cancel", "Cancel", Look::Plain, false, &tokens)
                            .child("Cancel")
                            .child(esc(&tokens))
                            .on_click(closing),
                    )
                    .child(h_flex().child(adding)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
