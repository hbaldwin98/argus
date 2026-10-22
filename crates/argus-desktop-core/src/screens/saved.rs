//! The scenes of a saved view's panel in the states the docked one does not show: followed into
//! more columns, built long ago, and a column whose view the daemon no longer holds.

use argus_protocol::ServerMsg;

use super::{saved_views, scene, Scene, Window, P_TRIM};
use crate::connection::page::{Call, SettingChange};
use crate::views::saved::SavedColumn;
use crate::layout::docks::Panel;
use crate::settings::remembered::SavedLinks;

/// `release-0-34` opened with its links followed into columns: the release, then the architecture
/// it points at, which is stale.
fn followed() -> Window {
    let mut w = Window::showing(P_TRIM);
    w.call(Call::Setting { change: SettingChange::SavedLinks { opens: SavedLinks::Column } })
        .call(Call::OpenSavedView { id: "release-0-34".into(), from: None })
        .call(Call::OpenSavedView { id: "architecture".into(), from: Some(SavedColumn { panel: Panel::Saved("release-0-34".into()), column: 0 }) });
    w
}

pub fn scenes() -> Vec<Scene> {
    vec![
        scene("saved-view-columns", "Saved view - a link followed into the next column", || followed().s),
        scene("saved-view-stale", "Saved view - one built long ago, stale", || {
            let mut w = Window::showing(P_TRIM);
            w.call(Call::OpenSavedView { id: "architecture".into(), from: None });
            w.s
        }),
        scene("saved-view-missing", "Saved view - a column whose view is gone", || {
            let mut w = followed();
            // The daemon lists only the release now: the column that followed it names a view it lost.
            let kept: Vec<_> = saved_views().into_iter().filter(|view| view.id == "release-0-34").collect();
            w.receive(ServerMsg::SavedViews(kept));
            w.s
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::page::Event;
    use crate::screens::NOW;

    fn panel(name: &str) -> crate::views::saved::SavedPanel {
        let scene = scenes().into_iter().find(|scene| scene.name == name).expect(name);
        let events = (scene.build)().everything(NOW);
        let page = events.into_iter().find_map(|event| if let Event::SavedViews(page) = event { Some(*page) } else { None }).expect("the saved views are sent");
        page.panels.into_iter().find(|panel| panel.panel == Panel::Saved("release-0-34".into()) || panel.panel == Panel::Saved("architecture".into())).expect("a panel is open")
    }

    #[test]
    fn a_followed_link_is_a_second_column_of_the_same_panel() {
        assert_eq!(panel("saved-view-columns").columns, ["release-0-34", "architecture"]);
    }

    #[test]
    fn the_stale_scene_opens_the_view_the_daemon_marks_stale() {
        assert_eq!(panel("saved-view-stale").columns, ["architecture"]);
        assert!(saved_views().iter().any(|view| view.id == "architecture" && view.stale));
    }

    #[test]
    fn the_missing_scene_keeps_the_column_of_a_view_the_daemon_no_longer_lists() {
        let scene = scenes().into_iter().find(|scene| scene.name == "saved-view-missing").unwrap();
        let events = (scene.build)().everything(NOW);
        let page = events.into_iter().find_map(|event| if let Event::SavedViews(page) = event { Some(*page) } else { None }).unwrap();
        assert_eq!(page.views.iter().map(|view| view.id.as_str()).collect::<Vec<_>>(), ["release-0-34"]);
        let open = page.panels.iter().find(|panel| panel.panel == Panel::Saved("release-0-34".into())).expect("its panel stays");
        assert_eq!(open.columns, ["release-0-34", "architecture"], "the page draws the second as missing");
    }
}
