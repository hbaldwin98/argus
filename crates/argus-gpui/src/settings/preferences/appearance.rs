//! The Appearance section: the theme cards, following the system, how solid the background is,
//! what shows through it, and the two fonts.
//!
//! Each control sends its choice at once and draws what the session sends back. The opacity slider
//! is the one that can be ahead of it: a thumb being dragged is where the person has it already, so
//! the echo is not written under it (`PreferencesView::dragging`).

use argus_desktop_core::connection::page::{SettingChange, SettingsPage};
use argus_desktop_core::settings::flavours::toggled_follow;
use argus_desktop_core::settings::fonts::missing_note;
use argus_desktop_core::settings::remembered::{Backdrop, Theme as Chosen};
use gpui_kit::component::slider::Slider;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, Context, SharedString};

use super::cards::cards;
use super::rows::{row, section};
use super::PreferencesView;
use crate::settings::look::system_dark;
use crate::kit::toggle::toggle;
use crate::theme::scale::rem;
use crate::theme::Tokens;

const BACKDROPS: [(Backdrop, &str); 3] = [(Backdrop::Clear, "Clear"), (Backdrop::Mica, "Mica"), (Backdrop::Acrylic, "Acrylic")];

impl PreferencesView {
    pub(super) fn appearance(&self, page: &SettingsPage, window: &gpui_kit::Window, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let chosen = &page.chosen;
        let dark = system_dark(window, cx);
        let drawn: Chosen = chosen.theme.resolve(dark);
        // Pressed, the rule is core's: following keeps the flavour it was drawing when it is left.
        let (theme, on) = (chosen.theme, chosen.theme == Chosen::System);
        let follow = toggle("settings-follow", "Follow the system", on, false, tokens)
            .on_click(cx.listener(move |this, _, _, _| this.set(SettingChange::Theme { theme: toggled_follow(theme, dark) })))
            .into_any_element();
        let slider = h_flex()
            .items_center()
            .gap(rem(tokens.sp_8))
            .child(div().w(rem(tokens.opacity_slider_w)).child(Slider::new(&self.opacity).accessibility_label("Background opacity")))
            .child(
                div()
                    .id("settings-percent")
                    .w(rem(tokens.percent_w))
                    .flex_shrink_0()
                    .text_right()
                    .child(SharedString::from(format!("{}%", chosen.opacity))),
            )
            .into_any_element();
        // Clear alone is no choice: where it is all this system draws, the row is left out.
        let backdrop = (page.backdrops.len() > 1).then(|| {
            let options: Vec<(Backdrop, &'static str)> = BACKDROPS.iter().filter(|(backdrop, _)| page.backdrops.iter().any(|offer| offer.backdrop == *backdrop)).copied().collect();
            let unavailable: Vec<(Backdrop, String)> = page.backdrops.iter().filter_map(|offer| offer.unavailable.clone().map(|why| (offer.backdrop, why))).collect();
            let reasons: Vec<String> = unavailable.iter().map(|(_, why)| why.clone()).collect();
            let notes = if reasons.is_empty() { Vec::new() } else { vec![reasons.join(". ")] };
            row(
                "What shows through",
                Some("Clear shows the desktop; Mica and Acrylic blur it"),
                notes,
                self.segment("What shows through", &options, chosen.backdrop, &unavailable, |backdrop| SettingChange::Backdrop { backdrop }, cx),
                tokens,
            )
        });
        let (code_note, ui_note) = (self.code_missing.clone(), self.ui_missing.clone());
        let rows = vec![
            cards(drawn, tokens, window.rem_size(), cx),
            row("Follow the system", Some("Mocha when it is dark, Latte when it is light"), vec![], follow, tokens),
            row("Background opacity", Some("The desktop shows through the background; text and borders stay solid"), vec![], slider, tokens),
        ]
        .into_iter()
        .chain(backdrop)
        .chain([
            row("Code font", Some("Panes and code, monospaced; a Nerd Font draws its icons"), code_note.into_iter().collect(), self.code_font.clone(), tokens),
            row("Interface font", Some("Menus, lists, headings and prose"), ui_note.into_iter().collect(), self.ui_font.clone(), tokens),
        ])
        .collect();
        let named = &super::nav::SECTIONS[0];
        section(("settings-section", 0usize), named.name, named.about, rows, tokens)
    }

    /// The notes under the font fields: what draws when no installed font is called what a field
    /// holds. Read once for each thing the session sent, since the names are a thousand.
    pub(super) fn missing(chosen: &str, installed: &std::collections::HashSet<String>, instead: &str) -> Option<String> {
        missing_note(chosen, installed.contains(chosen), instead)
    }
}
