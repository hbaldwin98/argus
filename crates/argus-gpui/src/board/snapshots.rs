//! The snapshots a test reads: what the session sends in the scenes the Tauri page is photographed
//! in, so a test of a surface holds the page core builds and not one written by hand.

use argus_desktop_core::connection::page::{Event, FeatureView, Features};
use argus_desktop_core::screens::{scenes, NOW};

/// Everything a fresh window would hear in the scene `name`, frames apart.
pub(crate) fn events(name: &str) -> Vec<Event> {
    let found = scenes().into_iter().find(|scene| scene.name == name).unwrap_or_else(|| panic!("no scene {name}"));
    let mut session = (found.build)();
    session.everything(NOW).into_iter().filter(|event| !matches!(event, Event::Frame(_))).collect()
}

/// The Feature stage's page in the scene `name`.
pub(crate) fn features(name: &str) -> Features {
    events(name)
        .into_iter()
        .find_map(|event| match event {
            Event::Features(page) => Some(*page),
            _ => None,
        })
        .unwrap_or_else(|| panic!("scene {name} sends no Feature stage"))
}

/// The feature open in the scene `name`.
pub(crate) fn open(name: &str) -> FeatureView {
    features(name).open.unwrap_or_else(|| panic!("scene {name} has no feature open"))
}


/// A window that has heard everything the scene `name` sends, its calls cleared: the board's surfaces
/// are drawn from what a session says, so a test starts from a session's word.
pub(crate) fn on(cx: &mut gpui_kit::TestAppContext, name: &str) -> crate::keys::root::tests::Opened {
    let opened = crate::keys::root::tests::open(cx);
    crate::keys::root::tests::hear(cx, &opened, events(name));
    crate::keys::root::tests::drain(&opened);
    opened
}
