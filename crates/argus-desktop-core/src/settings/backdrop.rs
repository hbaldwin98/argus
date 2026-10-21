//! Which backdrops this machine can draw behind a see-through window, and
//! whether one may be chosen. Putting one on a window is the window's
//! (`settings::vibrancy` in the binary that draws).
//!
//! Mica and Acrylic are Windows' own, so what is offered is a question of
//! the Windows build. Elsewhere only Clear is offered: Linux has no blur to
//! ask for, and macOS's own vibrancy is left for later.

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
