//! A font field (SHELL-FONT-1..6): a family typed, or picked from the installed ones listed under
//! it, each drawn in its own face. Typing narrows the list; a name is chosen when it is picked, on
//! Enter, or when the field is left, so a half-typed one never redraws the window in the fallback.
//!
//! What the list does is core's (`settings/font_picker`, which has the tests of its keys); this
//! draws its rows and keeps what only a window has: the field, the list's scroll, and the keys. The
//! list opens on the field's focus and on a press in it, goes down and up with the palette's keys
//! (`Place::Palette`), and Escape closes it alone, so the next Escape closes the dialog. A press
//! on a row does not take the keys from the field, so leaving it does not commit what was typed
//! before the row is chosen. Thousands of rows are a `uniform_list`, so only those in view are built.

use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, SettingChange};
use argus_desktop_core::keys::keymap::{Action, Place};
use argus_desktop_core::settings::font_picker::{FontPicker, Row};
use gpui_kit::component::input::{Input, InputEvent};
use gpui_kit::component::h_flex;
use gpui_kit::{
    deferred, div, prelude::*, uniform_list, Context, Focusable as _, ListSizingBehavior, MouseButton, Role, ScrollStrategy,
    SharedString, Subscription, TestSupportExt as _, UniformListScrollHandle, Window,
};

use crate::connection::caller::Caller;
use crate::keys::field::{Fields, Registered};
use crate::kit::field::Field;
use crate::kit::icons::Glyph;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub struct FontPickerView {
    label: &'static str,
    field: Field,
    model: FontPicker,
    /// The list's rows as the model last said them, shared with the list that draws them.
    rows: Rc<Vec<Row>>,
    change: fn(String) -> SettingChange,
    /// The theme's own face this field falls back to: the one its default row is drawn in.
    bundled: fn(&Tokens) -> &'static str,
    caller: Caller,
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
    _keys: Registered,
}

impl FontPickerView {
    /// `bundled` names the theme's own family for the field (the code face or the interface face).
    pub fn new(
        label: &'static str,
        change: fn(String) -> SettingChange,
        bundled: fn(&Tokens) -> &'static str,
        caller: Caller,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let default = bundled(Tokens::global(cx));
        let field = Field::new(default, window, cx);
        let focus = field.input().read(cx).focus_handle(cx);
        let typing = cx.subscribe_in(field.input(), window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::Change => {
                let text = this.field.text(cx).to_string();
                this.model.typed(&text);
                this.refresh(cx);
            }
            InputEvent::Focus => {
                this.model.open();
                this.refresh(cx);
            }
            InputEvent::Blur => {
                let said = this.model.blur();
                this.field.put(this.model.chosen(), window, cx);
                this.refresh(cx);
                this.say(said);
            }
            InputEvent::PressEnter { .. } => {}
        });
        let me = cx.weak_entity();
        let keys = Fields::of(cx).register_place(window, focus, Place::Palette, move |action, window, cx| {
            me.update(cx, |this, cx| this.keys(action, window, cx)).unwrap_or(false)
        });
        Self {
            label,
            field,
            model: FontPicker::new(default),
            rows: Rc::default(),
            change,
            bundled,
            caller,
            scroll: UniformListScrollHandle::new(),
            _subscriptions: vec![typing],
            _keys: keys,
        }
    }

    /// The palette's keys, which the field answers while its list is up. Whether it ran one.
    fn keys(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match action {
            Action::Next | Action::Previous => self.model.step(action == Action::Next),
            Action::Choose => match self.rows.get(self.model.active()).cloned() {
                Some(Row::Family { family, .. }) if self.model.is_open() => self.pick(&family, window, cx),
                _ => {
                    let said = self.model.enter();
                    self.refresh(cx);
                    self.say(said);
                    return true;
                }
            },
            // Closes the list rather than the dialog, which the next Escape does.
            Action::CloseLayer if self.model.escape() => self.field.put(self.model.chosen(), window, cx),
            _ => return false,
        }
        self.refresh(cx);
        true
    }

    /// A row picked: the field says it now (the choices say it too once Rust has heard), and it is
    /// chosen if it is not already.
    fn pick(&mut self, family: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.field.put(family, window, cx);
        let said = self.model.pick(family);
        self.refresh(cx);
        self.say(said);
    }

    fn say(&self, family: Option<String>) {
        if let Some(family) = family {
            self.caller.call(Call::Setting { change: (self.change)(family) });
        }
    }

    /// What the choices say now: the families to offer, the one chosen and what an empty field draws.
    /// A field being typed in is ahead of what came back, so it keeps its text.
    pub fn update(&mut self, families: Vec<String>, chosen: &str, default_name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.model.update(families, chosen);
        self.model.set_default_name(default_name);
        self.field.say_when_empty(default_name, window, cx);
        self.field.show(chosen, window, cx);
        self.refresh(cx);
    }

    /// The list as the model says it, scrolled to the row Enter would pick.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let rows = self.model.rows();
        if *self.rows != rows {
            self.rows = Rc::new(rows);
        }
        if self.model.is_open() {
            self.scroll.scroll_to_item(self.model.active(), ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    /// Whether the list is up.
    pub fn is_open(&self) -> bool {
        self.model.is_open()
    }

    /// The names of the rows the list shows, for a test: a note is its words.
    #[cfg(test)]
    pub fn listed(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| match row {
                Row::Family { label, .. } => label.clone(),
                Row::Note(words) => words.clone(),
            })
            .collect()
    }

    /// The family a field names in its own face: the chosen, else the theme's own.
    fn face(&self, family: &str, tokens: &Tokens) -> SharedString {
        SharedString::from(if family.is_empty() { (self.bundled)(tokens).to_owned() } else { family.to_owned() })
    }
}

impl Render for FontPickerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let chosen_face = self.face(self.model.chosen(), &tokens);
        let this = cx.entity();
        let rows = self.rows.clone();
        let (active, chosen) = (self.model.active(), self.model.chosen().to_owned());
        let list = self.model.is_open().then(|| {
            let faces: Vec<SharedString> = rows
                .iter()
                .map(|row| match row {
                    Row::Family { family, .. } => self.face(family, &tokens),
                    Row::Note(_) => SharedString::from(tokens.font_ui),
                })
                .collect();
            let count = rows.len();
            deferred(
                div()
                    .id((self.label, 1usize))
                    .role(Role::ListBox)
                    .test_support()
                    .aria_label(SharedString::from(format!("{} families", self.label)))
                    .absolute()
                    .top(rem(tokens.control_h + tokens.sp_4))
                    .left_0()
                    .right_0()
                    .max_h(rem(tokens.font_list_max_h))
                    .overflow_hidden()
                    .p(rem(tokens.sp_4))
                    .bg(tokens.bg_surface)
                    .border_1()
                    .border_color(tokens.line_strong)
                    .rounded(rem(tokens.r_md))
                    .shadow(tokens.shadow_pop())
                    .occlude()
                    .child(
                        uniform_list((self.label, 2usize), count, move |range, _, _| {
                            range
                                .map(|at| {
                                    let row = rows[at].clone();
                                    match row {
                                        Row::Note(words) => div()
                                            .id(("font-note", at))
                                            .flex()
                                            .items_center()
                                            .h(rem(tokens.control_h))
                                            .px(rem(tokens.sp_8))
                                            .text_color(tokens.fg_faint)
                                            .font_family(SharedString::from(tokens.font_ui))
                                            .child(SharedString::from(words))
                                            .into_any_element(),
                                        Row::Family { family, label } => {
                                            let on = at == active;
                                            let picked = this.clone();
                                            let name = family.clone();
                                            h_flex()
                                                .id(("font-row", at))
                                                .role(Role::ListBoxOption)
                                                .test_support()
                                                .aria_label(SharedString::from(label.clone()))
                                                .aria_selected(family == chosen)
                                                .h(rem(tokens.control_h))
                                                .px(rem(tokens.sp_8))
                                                .gap(rem(tokens.sp_8))
                                                .items_center()
                                                .rounded(rem(tokens.r_sm))
                                                .text_size(rem(tokens.fs_md))
                                                .text_color(tokens.fg)
                                                .font_family(faces[at].clone())
                                                .cursor_pointer()
                                                .when(on, |row| row.bg(tokens.bg_selected))
                                                .hover(move |style| style.bg(tokens.fill))
                                                // The keys stay in the field, which a press would take.
                                                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                                    window.prevent_default();
                                                    cx.stop_propagation();
                                                })
                                                .on_click(move |_, window, cx| {
                                                    picked.update(cx, |picker, cx| picker.pick(&name, window, cx));
                                                })
                                                .child(div().flex_1().min_w_0().truncate().child(SharedString::from(label)))
                                                .when(family == chosen, |row| row.child(div().text_color(tokens.accent).child("\u{2713}")))
                                                .into_any_element()
                                        }
                                    }
                                })
                                .collect()
                        })
                        .with_sizing_behavior(ListSizingBehavior::Infer)
                        .track_scroll(&self.scroll),
                    ),
            )
        });
        let press = cx.listener(|this, _, _, cx| {
            this.model.open();
            this.refresh(cx);
        });
        div()
            .relative()
            .flex_shrink_0()
            .w(rem(tokens.setting_field_w))
            .max_w(gpui_kit::relative(0.5))
            .font_family(chosen_face)
            .on_mouse_down(MouseButton::Left, press)
            .child(Input::new(self.field.input()).aria_label(self.label).h(rem(tokens.control_h)))
            .child(
                div()
                    .absolute()
                    .right(rem(tokens.sp_8))
                    .top_0()
                    .h(rem(tokens.control_h))
                    .flex()
                    .items_center()
                    .text_color(tokens.fg_faint)
                    .child(Glyph::Down.at(tokens.icon_sm)),
            )
            .children(list)
    }
}
