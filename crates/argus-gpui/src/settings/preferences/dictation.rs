//! The Dictation section: on or off, the two models and their files, the engine's line, the
//! language, device and microphone, the two keys, and what is done to the text (the Argus Dictation
//! design's Settings → Dictation).
//!
//! Every control sends its choice at once and draws what the session sends back (`DictationPage`),
//! as the rest of the sheet does. A window built without the engine hears no page, and says so.

use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::dictation::preferences::{DictationChange, Device};
use argus_desktop_core::dictation::section::{Act, Entry, ModelRow, SectionPage};
use argus_desktop_core::dictation::DictationGesture;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::{h_flex, v_flex, Disableable as _, Sizable as _};
use gpui_kit::{div, prelude::*, AnyElement, Anchor, Context, MouseButton, Role, SharedString, TestSupportExt as _, Window};

use super::rows::{meta, row, Group};
use super::{Asked, PreferencesView};
use crate::dictation::keys::DictationKeys;
use crate::dictation::panel::tone;
use crate::kit::button::{button, Look};
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::kit::progress::thin;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

const DEVICES: [(Device, &str); 3] = [(Device::Auto, "Auto"), (Device::Gpu, "GPU"), (Device::Cpu, "CPU")];

impl PreferencesView {
    /// The section's groups, or the line that says this window has no engine.
    pub(super) fn dictation(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Vec<Group> {
        let Some(page) = self.heard.read(cx).dictation.clone() else {
            let none = div().py(rem(tokens.sp_12)).child(meta("This build of the window has no dictation engine.", tokens).text_color(tokens.fg_faint)).into_any_element();
            return vec![Group::plain(vec![none])];
        };
        let page = &page.section;
        let chosen = &page.chosen;
        let off = !chosen.enabled;
        let on = vec![row(
            "Dictation",
            Some("On this computer. Audio never leaves it."),
            vec![],
            self.switch("settings-dictation", "Dictation", chosen.enabled, false, |on| DictationChange::Enabled { on }, cx),
            tokens,
        )];
        let mut model = vec![self.models(page, tokens, cx), status(page, tokens)];
        let languages: Vec<Entry<String>> = page.languages.clone();
        let language = languages.iter().find(|entry| entry.value == chosen.language).map_or_else(|| chosen.language.clone(), |entry| entry.label.clone());
        model.push(row(
            "Language",
            Some("Nemotron only. Auto picks it from what you say."),
            vec![],
            self.select("settings-dictation-language", "Language", language, languages, !page.language_applies || off, |language| DictationChange::Language { language }, tokens),
            tokens,
        ));
        model.push(row(
            "Device",
            Some("Auto uses the GPU when there is one"),
            vec![],
            self.segment("Device", &DEVICES, chosen.device, &[], |device| DictationChange::Device { device }, cx),
            tokens,
        ));
        let microphone = page.microphones.iter().find(|entry| entry.value == chosen.microphone).map_or_else(String::new, |entry| entry.label.clone());
        model.push(row(
            "Microphone",
            Some("The system default follows the system when it changes"),
            vec![],
            self.select("settings-dictation-microphone", "Microphone", microphone, page.microphones.clone(), off, |id| DictationChange::Microphone { id }, tokens),
            tokens,
        ));
        let keys = vec![
            row("Push-to-talk", Some("Hold to speak, release to paste. This chord is taken from programs in panes."), vec![], self.recorder(&page.hold, tokens, cx), tokens),
            row("Toggle", Some("After the leader. Press once to start and again to stop."), vec![], keyrec("settings-dictation-toggle", &page.toggle, tokens), tokens),
        ];
        let text = vec![
            row("Voice activity detection", Some("Skip silence before it reaches the model"), vec![], self.switch("settings-dictation-vad", "Voice activity detection", chosen.vad, off, |on| DictationChange::Vad { on }, cx), tokens),
            row("Remove filler words", Some("Drops \u{201c}uh\u{201d}, \u{201c}um\u{201d}, \u{201c}hmm\u{201d}\u{2026}"), vec![], self.switch("settings-dictation-fillers", "Remove filler words", chosen.remove_fillers, off, |on| DictationChange::RemoveFillers { on }, cx), tokens),
            row("Append trailing space", Some("End each dictation with a space"), vec![], self.switch("settings-dictation-space", "Append trailing space", chosen.trailing_space, off, |on| DictationChange::TrailingSpace { on }, cx), tokens),
            row("Press Enter after pasting", Some("Sends the prompt as soon as you let go"), vec![], self.switch("settings-dictation-enter", "Press Enter after pasting", chosen.enter_after, off, |on| DictationChange::EnterAfter { on }, cx), tokens),
        ];
        let dim = |rows: Vec<AnyElement>| if off { rows.into_iter().map(|row| div().opacity(tokens.opacity_dim).child(row).into_any_element()).collect() } else { rows };
        vec![Group::plain(on), Group::named("Model", dim(model)), Group::named("Keys", dim(keys)), Group::named("Text", dim(text))]
    }

    /// The model cards: a ring to choose it, its name and what it hears, its download, and what can
    /// be done with its file.
    fn models(&self, page: &SectionPage, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .id("settings-dictation-models")
            .role(Role::RadioGroup)
            .test_support()
            .aria_label("Model")
            .gap(rem(tokens.sp_6))
            .py(rem(tokens.sp_8))
            .children(page.models.iter().enumerate().map(|(at, model)| self.model(at, model, tokens, cx)))
            .into_any_element()
    }

    fn model(&self, at: usize, row: &ModelRow, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let model = row.model;
        // The kit's ring, drawn only: the whole card is the press.
        let ring = Radio::new(("dictation-model-ring", at)).checked(row.chosen);
        let act = row.act.map(|act| {
            let (words, gesture) = match act {
                Act::Download => ("Download", DictationGesture::Download { model }),
                Act::Resume => ("Resume", DictationGesture::Download { model }),
                Act::Cancel => ("Cancel", DictationGesture::CancelDownload { model }),
                Act::Delete => ("Delete", DictationGesture::Delete { model }),
            };
            let caller = self.caller.clone();
            button(("settings-dictation-act", at), words, Look::Quiet, true, tokens)
                .child(words)
                // The card under it chooses the model; this press is the file's alone.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, _| caller.call(Call::Dictation { gesture: gesture.clone() }))
        });
        let progress = row.progress.map(|at| thin(("settings-dictation-progress", model as usize), (at / 10) as u8, format!("{}% downloaded", at / 10), tokens));
        h_flex()
            .id(("settings-dictation-model", at))
            .role(Role::RadioButton)
            .test_support()
            .aria_label(SharedString::from(row.name.clone()))
            .aria_selected(row.chosen)
            .gap(rem(tokens.sp_12))
            .items_center()
            .px(rem(tokens.sp_12))
            .py(rem(tokens.sp_8))
            .rounded(rem(tokens.r_md))
            .border_1()
            .border_color(if row.chosen { tokens.fg_muted } else { tokens.line })
            .cursor_pointer()
            .when(!row.chosen, |card| card.hover(move |style| style.bg(tokens.hover_fill)))
            .on_click(cx.listener(move |this, _, _, _| this.set(DictationChange::Model { model })))
            .child(ring)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(rem(tokens.sp_2))
                    .child(SharedString::from(row.name.clone()))
                    .child(meta(&row.line, tokens))
                    .children(row.failed.as_ref().map(|why| meta(why, tokens).text_color(tokens.danger)))
                    .children(progress.map(|bar| div().pt(rem(tokens.sp_4)).child(bar))),
            )
            .children(act)
            .into_any_element()
    }

    /// A menu of `entries` under a button that says `shown`; `off` dims it and opens nothing.
    #[allow(clippy::too_many_arguments)]
    fn select<T: Clone + PartialEq + 'static>(&self, id: &'static str, label: &'static str, shown: String, entries: Vec<Entry<T>>, off: bool, ask: fn(T) -> DictationChange, tokens: &Tokens) -> AnyElement {
        let caller = self.caller.clone();
        let tokens = *tokens;
        let face = Button::new(id)
            .outline()
            .small()
            .child(div().min_w_0().truncate().child(SharedString::from(shown.clone())))
            .child(Glyph::Down.at(tokens.icon_xs));
        let face = if off {
            face.disabled(true).into_any_element()
        } else {
            let entries = Rc::new(entries);
            face.dropdown_menu_with_anchor(Anchor::TopRight, move |menu: PopupMenu, window: &mut Window, cx: &mut Context<PopupMenu>| {
                let items: Vec<MenuItem<T>> = entries
                    .iter()
                    .map(|entry| {
                        let choice = MenuItem::choice(entry.label.clone(), entry.value.clone());
                        if entry.label == shown { choice.noted("\u{2713}") } else { choice }
                    })
                    .collect();
                let caller = caller.clone();
                popup::fill(menu, &items, tokens, |_| false, move |value: &T, _, _| caller.call(ask(value.clone()).call()), window, cx)
            })
            .into_any_element()
        };
        div()
            .id(SharedString::from(format!("{id}-row")))
            .test_support()
            .aria_label(SharedString::from(label))
            .max_w(rem(tokens.setting_field_w))
            .flex_shrink_0()
            .child(face)
            .into_any_element()
    }

    /// The push-to-talk chord as written, which a press sets waiting for the next chord.
    fn recorder(&self, hold: &str, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let recording = DictationKeys::recording(cx);
        let words = if recording { "Press a chord\u{2026}".to_owned() } else { hold.to_owned() };
        keyrec("settings-dictation-hold", &words, tokens)
            .cursor_pointer()
            .when(recording, |keys| keys.border_color(tokens.accent))
            .hover(move |style| style.border_color(tokens.accent))
            .on_click(cx.listener(move |_, _, window, cx| {
                if let Some(keys) = DictationKeys::get(cx) {
                    keys.record(!recording);
                }
                window.refresh();
            }))
            .into_any_element()
    }
}

/// The design's `.keyrec`: a chord written in the code face on a sunken box.
fn keyrec(id: &'static str, words: &str, tokens: &Tokens) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .flex_none()
        .font_family(SharedString::from(tokens.font_mono))
        .typed(tokens.type_small)
        .px(rem(tokens.sp_8))
        .py(rem(tokens.sp_4))
        .rounded(rem(tokens.r_sm))
        .border_1()
        .border_color(tokens.line_strong)
        .bg(tokens.bg_sunken)
        .text_color(tokens.fg)
        .child(SharedString::from(words.to_owned()))
}

/// The engine's line under the model cards.
fn status(page: &SectionPage, tokens: &Tokens) -> AnyElement {
    let (said, words) = &page.status;
    h_flex()
        .id("settings-dictation-status")
        .test_support()
        .aria_label(SharedString::from(words.clone()))
        .gap(rem(tokens.sp_6))
        .items_center()
        .pb(rem(tokens.sp_8))
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted)
        .child(div().size(rem(tokens.dot_sm)).flex_none().rounded_full().bg(tone(*said, tokens)))
        .child(SharedString::from(words.clone()))
        .into_any_element()
}
