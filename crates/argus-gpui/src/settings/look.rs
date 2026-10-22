//! The look the person chose, put on the window: the theme and the fonts in the token space, what
//! shows through the window, the zoom, and the frame's border.
//!
//! The choices are the session's (`Heard::settings`, sent whenever one changes, dialog open or not),
//! so the look follows them on every event and is put on only when it differs from what is on
//! (SHELL-LOOK-1..6). The theme is installed over the one before, which is how every view reads
//! the new colours; the backdrop and the border are set on the platform window; the zoom is the
//! window's rem size, since every size in a view is `rem` of a token (decision #66). The kit's root
//! sets the rem size to the theme's font size each frame, so the zoom is put on the kit's theme
//! (`theme::kit::install`) and the window follows.

use argus_desktop_core::connection::page::SettingsPage;
use argus_desktop_core::settings::frame_border::frame_border;
use argus_desktop_core::settings::remembered::{Backdrop, Preferences, Theme as Chosen};
use gpui_kit::{px, App, Global, Pixels, Window, WindowAppearance, WindowBackgroundAppearance};

use crate::kit::fonts;
use crate::theme::scale::REM;
use crate::theme::{choice, kit, Tokens};
use crate::window::frame;

/// Everything a window's look is made of, as chosen and as the system is.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub theme: Chosen,
    pub opacity: u8,
    pub backdrop: Backdrop,
    /// The families chosen for the code and the interface; empty draws the theme's own.
    pub font: String,
    pub ui_font: String,
    pub zoom: u16,
    pub system_dark: bool,
}

impl Look {
    pub fn of(chosen: &Preferences, system_dark: bool) -> Self {
        Self {
            theme: chosen.theme,
            opacity: chosen.opacity,
            backdrop: chosen.backdrop,
            font: chosen.font.clone(),
            ui_font: chosen.ui_font.clone(),
            zoom: chosen.zoom,
            system_dark,
        }
    }

    /// Whether the theme drawn is a dark one.
    pub fn dark(&self) -> bool {
        self.theme.is_dark(self.system_dark)
    }

    /// The token space: the theme drawn, its large fills at the opacity, and the families chosen
    /// in front of the theme's own (the page's `"<chosen>", var(--font-mono-bundled)`).
    pub fn tokens(&self) -> Tokens {
        let mut tokens = choice::tokens(self.theme, self.system_dark, self.opacity);
        tokens.scale.font_ui = fonts::ahead(&self.ui_font, tokens.font_ui_bundled);
        tokens.scale.font_mono = fonts::ahead(&self.font, tokens.font_mono_bundled);
        tokens
    }

    /// What shows through. The window always opens `Transparent`, and the opacity is the alpha of
    /// the fills, so Clear is the same appearance at any opacity and the other two are the
    /// platform's own blur (SHELL-LOOK-3, 5).
    pub fn appearance(&self) -> WindowBackgroundAppearance {
        match self.backdrop {
            Backdrop::Clear => WindowBackgroundAppearance::Transparent,
            Backdrop::Mica => WindowBackgroundAppearance::MicaBackdrop,
            Backdrop::Acrylic => WindowBackgroundAppearance::Blurred,
        }
    }

    /// The size of a rem the window draws with: the zoom is the page's webview zoom, which GPUI has as the rem size.
    pub fn rem_size(&self) -> Pixels {
        px(REM * f32::from(self.zoom) / 100.0)
    }

    /// The border Windows 11 draws, which is the sidebar's rule in this theme.
    pub fn border(&self) -> u32 {
        frame_border(self.theme, self.system_dark)
    }
}

/// The look on the window now.
struct Applied(Look);

impl Global for Applied {}

/// A system that is dark or light whatever the window's appearance says: the screenshots are taken
/// in the dark, the page's are, and a machine set to light would otherwise draw Latte.
pub struct Forced(pub bool);

impl Global for Forced {}

/// Whether the system is dark: the window's appearance, vibrant or not.
pub fn system_dark(window: &Window, cx: &App) -> bool {
    match cx.try_global::<Forced>() {
        Some(Forced(dark)) => *dark,
        None => matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark),
    }
}

/// The look the app starts in, before the session has sent a choice: what a person who has chosen
/// nothing sees, which is the page's default (the system's, so Mocha in the dark).
pub fn start(dark: bool, cx: &mut App) {
    put(&Look::of(&Preferences::default(), dark), cx);
}

/// A window that has just opened takes the look the app is in, which the session may have
/// already sent: the backdrop, the zoom and the border are the window's, not the app's.
pub fn dress_new(window: &mut Window, cx: &mut App) {
    let Some(on) = cx.try_global::<Applied>().map(|applied| applied.0.clone()) else { return };
    let look = Look { system_dark: system_dark(window, cx), ..on };
    // A look already on the app has not been put on this window.
    if !follow_look(look.clone(), window, cx) {
        dress(&look, window);
    }
}

/// Follows what the session sent: puts the look on the window when it is not on already. Whether
/// it changed anything.
pub fn follow(page: &SettingsPage, window: &mut Window, cx: &mut App) -> bool {
    let look = Look::of(&page.chosen, system_dark(window, cx));
    follow_look(look, window, cx)
}

/// The system went dark or light: a theme that follows it draws the other.
pub fn system_changed(window: &mut Window, cx: &mut App) -> bool {
    let Some(Applied(on)) = cx.try_global::<Applied>() else { return false };
    let look = Look { system_dark: system_dark(window, cx), ..on.clone() };
    follow_look(look, window, cx)
}

fn follow_look(look: Look, window: &mut Window, cx: &mut App) -> bool {
    if cx.try_global::<Applied>().is_some_and(|on| on.0 == look) {
        return false;
    }
    put(&look, cx);
    dress(&look, window);
    true
}

/// Puts `look`'s theme and fonts on the app: every view reads them from the token space.
fn put(look: &Look, cx: &mut App) {
    kit::install(cx, look.tokens(), look.zoom);
    cx.set_global(Applied(look.clone()));
    cx.refresh_windows();
}

/// Puts what only a platform window has on `window`.
fn dress(look: &Look, window: &mut Window) {
    window.set_background_appearance(look.appearance());
    frame::paint(window, look.border(), look.dark());
    window.refresh();
}

#[cfg(test)]
mod tests;
