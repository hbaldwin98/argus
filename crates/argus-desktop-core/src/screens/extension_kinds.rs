//! The scenes of an extension's other asks and views: the one-line prompt, the pick from a list,
//! the form of named lines, and a view that is a console. The confirm and the table are `mod.rs`'s.

use argus_protocol::{ExtDialog, ExtDialogKind, ExtField, ExtItem, ServerMsg};
use serde_json::json;

use super::{extension_views, scene, Scene, Window, P_SHELL, P_TRIM};
use crate::connection::page::{Call, Side};
use crate::layout::docks::Panel;

fn asking(title: &str, kind: ExtDialogKind) -> Window {
    let mut w = Window::showing(P_TRIM);
    w.receive(ServerMsg::ExtensionDialog { extension: "docker".into(), id: 9, dialog: ExtDialog { title: title.into(), kind, answers: None }, prompted: false });
    w
}

/// The docker extension's views, and `dev`, a console running the dev server in the shell's pane.
fn with_a_console() -> ServerMsg {
    let ServerMsg::ExtensionViews(mut views) = extension_views() else { unreachable!("it is the views") };
    let dev = json!({
        "extension": "docker", "name": "dev", "title": "Dev server", "icon": "terminal",
        "console": {"command": ["npm", "run", "dev"], "pane": P_SHELL},
        "actions": [{"id": "restart", "label": "Restart", "view_wide": true}]
    });
    views.push(serde_json::from_value(dev).unwrap());
    ServerMsg::ExtensionViews(views)
}

pub fn scenes() -> Vec<Scene> {
    vec![
        scene("extension-prompt", "Extension - a question answered in a line", || {
            asking("Rename api-web", ExtDialogKind::Prompt { message: "New name for the container".into(), initial: "api-web".into() }).s
        }),
        scene("extension-pick", "Extension - a choice from a list", || {
            let item = |key: &str, label: &str| ExtItem { key: key.into(), label: label.into() };
            asking("Attach a shell", ExtDialogKind::Pick { message: "Which container?".into(), items: vec![item("api-web", "api-web"), item("api-worker", "api-worker"), item("postgres", "postgres"), item("redis", "redis")] }).s
        }),
        scene("extension-form", "Extension - a few named lines sent together", || {
            let field = |name: &str, label: &str, initial: &str| ExtField { name: name.into(), label: label.into(), initial: initial.into() };
            asking("Run a container", ExtDialogKind::Form { fields: vec![field("image", "Image", "postgres:16-alpine"), field("name", "Name", ""), field("ports", "Ports", "5432:5432")] }).s
        }),
        scene("extension-console", "Extension - a view that is a console, as the page", || {
            let panel = || Panel::extension("docker", "dev");
            let mut w = Window::showing(P_TRIM);
            w.receive(with_a_console())
                .call(Call::OpenExtensionView { extension: "docker".into(), view: "dev".into() })
                .call(Call::ExtensionMode { panel: panel(), page: true })
                .call(Call::Resize { side: Side::View(0), rows: 30, cols: 120 });
            w.s
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::page::Event;
    use crate::screens::NOW;

    fn heard(name: &str) -> Vec<Event> {
        let scene = scenes().into_iter().find(|scene| scene.name == name).expect(name);
        let mut s = (scene.build)();
        let mut events = s.everything(NOW);
        events.extend(s.take_frames());
        events
    }

    #[test]
    fn each_dialog_scene_asks_the_kind_it_is_named_for() {
        for (name, is) in [
            ("extension-prompt", (|kind: &ExtDialogKind| matches!(kind, ExtDialogKind::Prompt { .. })) as fn(&ExtDialogKind) -> bool),
            ("extension-pick", |kind| matches!(kind, ExtDialogKind::Pick { items, .. } if items.len() == 4)),
            ("extension-form", |kind| matches!(kind, ExtDialogKind::Form { fields } if fields.len() == 3)),
        ] {
            let asked = heard(name).into_iter().find_map(|event| if let Event::ExtensionDialog(dialog) = event { Some(dialog) } else { None });
            let asked = asked.unwrap_or_else(|| panic!("{name} asks nothing"));
            let kind = asked.dialog.expect("a dialog is up").kind;
            assert!(is(&kind), "{name}: {kind:?}");
        }
    }

    #[test]
    fn the_console_scene_is_a_console_page_with_a_screen() {
        let events = heard("extension-console");
        let page = events.iter().find_map(|event| if let Event::Extensions(page) = event { Some(page) } else { None }).expect("the views are sent");
        let table = page.tables.iter().find(|table| table.panel == Panel::extension("docker", "dev")).expect("the console view is on screen");
        assert!(table.page);
        let console = table.console.as_ref().expect("a console");
        assert_eq!(console.side, Some(Side::View(0)), "the window gave it a screen");
        assert!(events.iter().any(|event| matches!(event, Event::Frame(_))), "and the screen is drawn");
    }
}
