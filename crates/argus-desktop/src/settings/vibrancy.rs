//! Putting the chosen backdrop on the live window, taking off whichever was
//! there. Which backdrops a machine offers is
//! `argus_desktop_core::settings::backdrop`.
//!
//! The window is always built transparent, since that can only be set when
//! it is made; how much shows through is the page's alpha on its fills, so
//! neither the opacity nor the backdrop ever rebuilds the window. Mica and
//! Acrylic are Windows' own, through `window-vibrancy`, which puts one on
//! and takes it off a live window.

use tauri::WebviewWindow;

use argus_desktop_core::settings::remembered::Backdrop;

/// Puts `backdrop` behind the window, taking off whichever was there.
/// `dark` is the theme drawn, which Mica takes its tint from.
pub fn show(window: &WebviewWindow, backdrop: Backdrop, dark: bool) {
    #[cfg(windows)]
    {
        // Clearing one that is not on is no error worth hearing.
        let _ = window_vibrancy::clear_mica(window);
        let _ = window_vibrancy::clear_acrylic(window);
        let shown = match backdrop {
            Backdrop::Clear => Ok(()),
            Backdrop::Mica => window_vibrancy::apply_mica(window, Some(dark)),
            // No tint of its own: the page's background, at the opacity
            // chosen, is the tint.
            Backdrop::Acrylic => window_vibrancy::apply_acrylic(window, Some((0, 0, 0, 0))),
        };
        if let Err(err) = shown {
            tracing::warn!("could not put {backdrop:?} behind the window: {err}");
        }
    }
    #[cfg(not(windows))]
    let _ = (window, backdrop, dark);
}
