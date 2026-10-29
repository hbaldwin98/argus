//! The Settings sheet (SHELL-SETTINGS-1..13): the theme, the background and the fonts, where a file
//! opens and with what, when an agent needs you, dictation, and how the feature index and a saved
//! view's links open, in seven sections down one scrolling page with a nav beside it.
//!
//! Every control sends its choice at once (`Call::Setting`) and draws what the session sends back
//! (`Heard::settings`), updated in place and never rebuilt, so a slider being dragged or a command
//! being typed keeps its hold while the choices come back. The sheet asks for its own keys: it takes
//! them from the pane by focusing its close button without a ring, and the settings chord or Escape
//! put it away (`keys/layers/own`). The look the choices give the whole window is not here: it is
//! put on by `settings/look` whether this is open or not.

mod appearance;
mod cards;
mod dictation;
mod nav;
mod plain;
mod rows;

use std::collections::HashSet;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, SettingChange, SettingsPage};
use argus_desktop_core::dictation::preferences::DictationChange;
use argus_desktop_core::dictation::DictationGesture;
use argus_desktop_core::settings::remembered::MIN_OPACITY;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, Context, Entity, FocusHandle, Role, ScrollHandle, Subscription, TestSupportExt as _, Window};

use super::font_picker::FontPickerView;
use super::look::system_dark;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::dialog::sheet_head;
use crate::kit::field::Field;
use crate::kit::icons::Glyph;
use crate::kit::overlay::host::{Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::{derive, Tokens};

/// Where the sections after Appearance start in [`nav::SECTIONS`].
const SECTIONS_AT: usize = 1;

/// The Dictation section's name, which the dictation panel and chip open the sheet at.
pub const DICTATION: &str = "Dictation";

/// A choice the sheet sends: the window's preferences and dictation's are each their own call.
pub(super) trait Asked {
    fn call(self) -> Call;
}

impl Asked for SettingChange {
    fn call(self) -> Call {
        Call::Setting { change: self }
    }
}

impl Asked for DictationChange {
    fn call(self) -> Call {
        Call::Dictation { gesture: DictationGesture::Set { change: self } }
    }
}

/// Opens the sheet at the section named `name`, or goes there if it is up.
pub fn open_at(name: &str, window: &mut Window, cx: &mut App) {
    let Some(surfaces) = crate::keys::over::Surfaces::get(cx) else { return };
    let at = nav::SECTIONS.iter().position(|section| section.name == name).unwrap_or(0);
    if Overlays::get(cx).is_open(Overlay::Settings) {
        surfaces.settings.update(cx, |view, cx| view.go(at, cx));
        return;
    }
    surfaces.settings.update(cx, |view, cx| {
        view.toggle(window, cx);
        view.opening = Some(at);
    });
}

pub struct PreferencesView {
    heard: Entity<Heard>,
    caller: Caller,
    command: Field,
    code_font: Entity<FontPickerView>,
    ui_font: Entity<FontPickerView>,
    opacity: Entity<SliderState>,
    /// A thumb is being dragged: the session's echo of a value must not be written under it.
    dragging: bool,
    pub(crate) body: ScrollHandle,
    /// The section the sheet was opened at, gone to once the body has been laid out: the kit drops a
    /// scroll asked of a body that has never been drawn.
    opening: Option<usize>,
    /// Where the keys are while the sheet is up: the close button, with no ring.
    close: FocusHandle,
    /// The choices the fonts' notes were last read for, so a thousand family names are checked once
    /// for each thing the session sent and not on every frame.
    read: Option<Rc<SettingsPage>>,
    code_missing: Option<String>,
    ui_missing: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl PreferencesView {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let command = Field::new("nvim", window, cx);
        let code_font = {
            let caller = caller.clone();
            cx.new(|cx| FontPickerView::new("Code font", |family| SettingChange::Font { family }, |tokens| tokens.font_mono_bundled, caller, window, cx))
        };
        let ui_font = {
            let caller = caller.clone();
            cx.new(|cx| FontPickerView::new("Interface font", |family| SettingChange::UiFont { family }, |tokens| tokens.font_ui_bundled, caller, window, cx))
        };
        let opacity = cx.new(|_| SliderState::new().min(f32::from(MIN_OPACITY)).max(100.0).step(1.0).default_value(100.0));
        let subscriptions = vec![
            cx.observe_in(&heard, window, |this, _, window, cx| this.follow(window, cx)),
            cx.subscribe(command.input(), |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let command = this.command.text(cx).to_string();
                    this.set(SettingChange::EditorCommand { command });
                }
            }),
            cx.subscribe(&opacity, |this, _, event: &SliderEvent, _| match event {
                SliderEvent::Change(value) => {
                    this.dragging = true;
                    this.set(SettingChange::Opacity { percent: value.end().round().clamp(0.0, 100.0) as u8 });
                }
                SliderEvent::Release(_) => this.dragging = false,
            }),
        ];
        Self {
            heard,
            caller,
            command,
            code_font,
            ui_font,
            opacity,
            dragging: false,
            body: ScrollHandle::new(),
            opening: None,
            close: cx.focus_handle(),
            read: None,
            code_missing: None,
            ui_missing: None,
            _subscriptions: subscriptions,
        }
    }

    /// Asks the session for one choice. Nothing is drawn from it until the session says it back.
    fn set(&self, change: impl Asked) {
        self.caller.call(change.call());
    }

    /// The choices the session last sent, which the sheet draws.
    fn page(&self, cx: &App) -> Option<Rc<SettingsPage>> {
        self.heard.read(cx).settings.clone()
    }

    /// Lets the controls that hold a value of their own follow what the session said: the fonts and
    /// their lists, the slider and the command, none of them while the person has a hold of it.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = self.page(cx) else { return };
        if self.read.as_ref().is_some_and(|read| Rc::ptr_eq(read, &page)) {
            return;
        }
        let own = derive::tokens(crate::theme::choice::theme(page.chosen.theme, system_dark(window, cx)));
        let mono: Vec<String> = page.fonts.iter().filter(|family| family.mono).map(|family| family.name.clone()).collect();
        let all: Vec<String> = page.fonts.iter().map(|family| family.name.clone()).collect();
        let (font, ui_font) = (page.chosen.font.clone(), page.chosen.ui_font.clone());
        self.code_font.update(cx, |picker, cx| picker.update(mono, &font, own.font_mono_bundled, window, cx));
        self.ui_font.update(cx, |picker, cx| picker.update(all, &ui_font, own.font_ui_bundled, window, cx));
        if !self.dragging {
            let percent = f32::from(page.chosen.opacity);
            self.opacity.update(cx, |slider, cx| slider.set_value(percent, window, cx));
        }
        self.command.show(&page.chosen.editor_command, window, cx);
        // Every name the text system lists, which includes the window's own faces: those draw.
        let installed: HashSet<String> = cx.text_system().all_font_names().into_iter().collect();
        self.code_missing = Self::missing(&font, &installed, own.font_mono_bundled);
        self.ui_missing = Self::missing(&ui_font, &installed, own.font_ui_bundled);
        self.read = Some(page);
        cx.notify();
    }

    /// Opens the sheet, or puts it away when it is up: the settings chord says both (SHELL-KEYS-13).
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = Overlays::get(cx);
        if host.is_open(Overlay::Settings) {
            host.close(Overlay::Settings, window, cx);
            return;
        }
        // Nothing has been sent yet: there is nothing to show, and the page opens nothing either.
        if self.page(cx).is_none() {
            return;
        }
        self.follow(window, cx);
        let (view, close) = (cx.entity(), self.close.clone());
        host.open(
            Spec::new(Overlay::Settings),
            move |dialog: Dialog, window, cx, first| {
                // The keys are on the close button, so the pane is blurred and a letter is held.
                if first {
                    window.focus(&close, cx);
                }
                let tokens = *Tokens::global(cx);
                let viewport = window.viewport_size();
                // The page centres the sheet (`.scrim`'s `align-items: center`); the kit's default is a tenth down.
                let height = pixels(tokens.settings_h, window.rem_size()).min(viewport.height * tokens.sheet_max_share);
                dialog
                    .w(pixels(tokens.settings_w, window.rem_size()).min(viewport.width * tokens.settings_max_w_share))
                    .margin_top((viewport.height - height) / 2.0)
                    .p_0().on_ok(|_, _, _| false).child(view.clone())
            },
            window,
            cx,
        );
        cx.notify();
    }

    /// A press on a name in the nav: the body goes to that section.
    pub(crate) fn go(&mut self, at: usize, cx: &mut Context<Self>) {
        self.body.scroll_to_item(at);
        cx.notify();
    }

    /// The close button, which the sheet's keys rest on.
    pub fn close_focus(&self) -> &FocusHandle {
        &self.close
    }
}

impl Render for PreferencesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        // Before the session has sent a choice the sheet is its head and its nav and nothing in the body.
        let page = self.page(cx);
        let me = cx.entity();
        let presses: Vec<nav::Press> = (0..nav::SECTIONS.len())
            .map(|at| {
                let me = me.clone();
                Rc::new(move |_: &mut Window, cx: &mut App| me.update(cx, |this, cx| this.go(at, cx))) as nav::Press
            })
            .collect();
        if let Some(at) = self.opening {
            if self.body.bounds().size.height > gpui_kit::px(0.0) {
                self.body.scroll_to_top_of_item(at);
                self.opening = None;
            } else {
                window.request_animation_frame();
            }
        }
        let lit = nav::in_view(&self.body, &tokens);
        let saved = h_flex()
            .id("settings-saved")
            .test_support()
            .aria_label("Saved as you change them")
            .gap(rem(tokens.sp_6))
            .items_center()
            .typed(tokens.type_small)
            .text_color(tokens.fg_muted)
            .child(div().text_color(tokens.ok).child(Glyph::Check.at(tokens.icon_xs)))
            .child("Saved as you change them");
        let sections: Vec<_> = match &page {
            Some(page) => std::iter::once(self.appearance(page, window, &tokens, cx)).chain(self.plain(page, &tokens, cx)).collect(),
            None => Vec::new(),
        };
        let version = page.as_ref().map_or("", |page| page.version.as_str());
        v_flex()
            .id("settings")
            .role(Role::Dialog)
            .test_support()
            .aria_label("Settings")
            .w_full()
            .h(pixels(tokens.settings_h, window.rem_size()).min(window.viewport_size().height * tokens.sheet_max_share))
            .child(sheet_head(
                "settings",
                "Settings",
                saved,
                "Close",
                |window, cx| {
                    Overlays::get(cx).close(Overlay::Settings, window, cx);
                },
                Some(&self.close),
                &tokens,
            ))
            .child(
                h_flex().flex_1().min_h_0().overflow_hidden().child(nav::column(lit, version, &tokens, presses)).child(
                    v_flex()
                        .id("settings-body")
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.body)
                        // The body scrolls itself; the sheet is drawn again so the nav lights the section now in view.
                        .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                        .gap(rem(tokens.sp_24))
                        .px(rem(tokens.sp_24))
                        .py(rem(tokens.sp_16))
                        .children(sections),
                ),
            )
    }
}

#[cfg(test)]
mod tests;
