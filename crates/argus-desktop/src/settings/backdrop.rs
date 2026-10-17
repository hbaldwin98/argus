//! What shows through the window where its background is see-through: the
//! backdrops this machine can draw, and putting the chosen one on the live
//! window.
//!
//! The window is always built transparent, since that can only be set when
//! it is made; how much shows through is the page's alpha on its fills, so
//! neither the opacity nor the backdrop ever rebuilds the window. Mica and
//! Acrylic are Windows' own, through `window-vibrancy`, which puts one on
//! and takes it off a live window. Elsewhere only Clear is offered: Linux
//! has no blur to ask for, and macOS's own vibrancy is left for later.

use tauri::WebviewWindow;

use crate::connection::page::BackdropOffer;
use crate::settings::remembered::Backdrop;

/// The backdrops to offer, each greyed out with the reason where this
/// machine cannot draw it.
pub fn offered() -> Vec<BackdropOffer> {
    offered_on(windows_build())
}

/// `build` is the Windows build, `None` off Windows.
fn offered_on(build: Option<u32>) -> Vec<BackdropOffer> {
    let clear = BackdropOffer { backdrop: Backdrop::Clear, unavailable: None };
    let Some(build) = build else {
        return vec![clear];
    };
    let needs = |least: u32, reason: &str| (build < least).then(|| reason.to_string());
    vec![
        clear,
        BackdropOffer { backdrop: Backdrop::Mica, unavailable: needs(MICA_BUILD, "Mica needs Windows 11") },
        BackdropOffer {
            backdrop: Backdrop::Acrylic,
            unavailable: needs(ACRYLIC_BUILD, "Acrylic needs Windows 10 version 1809 or later"),
        },
    ]
}

/// Whether `backdrop` may be chosen here.
pub fn can_draw(backdrop: Backdrop) -> bool {
    offered().iter().any(|offer| offer.backdrop == backdrop && offer.unavailable.is_none())
}

/// The first Windows 11 build, where `window-vibrancy` has Mica.
const MICA_BUILD: u32 = 22000;
/// Windows 10 1809, the first with the Acrylic `window-vibrancy` asks for.
const ACRYLIC_BUILD: u32 = 17763;

#[cfg(windows)]
fn windows_build() -> Option<u32> {
    Some(windows_version::OsVersion::current().build)
}

#[cfg(not(windows))]
fn windows_build() -> Option<u32> {
    None
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn names(offers: &[BackdropOffer]) -> Vec<(Backdrop, bool)> {
        offers.iter().map(|o| (o.backdrop, o.unavailable.is_none())).collect()
    }

    #[test]
    fn off_windows_only_clear_is_offered() {
        assert_eq!(names(&offered_on(None)), [(Backdrop::Clear, true)]);
    }

    #[test]
    fn windows_11_offers_all_three() {
        let offers = offered_on(Some(26100));
        assert_eq!(names(&offers), [(Backdrop::Clear, true), (Backdrop::Mica, true), (Backdrop::Acrylic, true)]);
    }

    #[test]
    fn windows_10_greys_mica_out_and_says_why() {
        let offers = offered_on(Some(19045));
        assert_eq!(names(&offers), [(Backdrop::Clear, true), (Backdrop::Mica, false), (Backdrop::Acrylic, true)]);
        assert_eq!(offers[1].unavailable.as_deref(), Some("Mica needs Windows 11"));
    }

    #[test]
    fn clear_can_always_be_drawn() {
        assert!(can_draw(Backdrop::Clear));
    }
}
