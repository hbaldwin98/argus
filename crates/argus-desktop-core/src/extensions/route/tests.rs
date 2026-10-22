use super::*;
use crate::extensions::keys::ExtChord;
use crate::extensions::views::ExtTableAction;
use crate::layout::docks::Panel;

fn page(key: &str) -> PageKey {
    PageKey { key: key.into(), ..PageKey::default() }
}

fn view() -> ExtTable {
    let action = |id: &str, key: &str| ExtTableAction { id: id.into(), label: id.into(), key: Some(key.into()), view_wide: false };
    ExtTable {
        panel: Panel::extension("docker", "containers"),
        extension: "docker".into(),
        view: "containers".into(),
        title: "Containers".into(),
        page: false,
        columns: Vec::new(),
        rows: Vec::new(),
        actions: vec![action("restart", "r")],
        selected: None,
        default: None,
        console: None,
    }
}

fn command(name: &str, key: &str, scope: &'static str, view: Option<&str>) -> ExtCommandPage {
    ExtCommandPage {
        extension: "docker".into(),
        command: name.into(),
        title: name.into(),
        chords: vec![ExtChord { key: key.into(), ctrl: false, alt: false, shift: false, meta: false, leader: false }],
        scope,
        view: view.map(str::to_owned),
    }
}

#[test]
fn the_keymaps_mode_actions_move_the_view_and_its_steps_are_the_views_keys() {
    let (v, c) = (view(), Vec::new());
    assert_eq!(route(&v, &c, Some(Action::ExtensionPage), &page(">")), Some(Routed::Mode { page: true }));
    assert_eq!(route(&v, &c, Some(Action::ExtensionDock), &page("<")), Some(Routed::Mode { page: false }));
    for action in [Action::Next, Action::Previous, Action::First, Action::Last, Action::Choose] {
        assert_eq!(route(&v, &c, Some(action), &page("j")), Some(Routed::Key(action)));
    }
}

#[test]
fn another_action_of_the_window_is_the_layers_to_run() {
    assert_eq!(route(&view(), &[], Some(Action::Palette), &page("k")), Some(Routed::Window(Action::Palette)));
}

#[test]
fn a_key_the_keymap_does_not_bind_is_an_action_key_before_a_command() {
    let commands = vec![command("rebuild", "r", "view", Some("containers"))];
    assert_eq!(route(&view(), &commands, None, &page("r")), Some(Routed::Act("restart")));
}

#[test]
fn a_command_is_heard_in_its_own_view_and_anywhere_and_nowhere_else() {
    let commands = vec![
        command("prune", "p", "view", Some("containers")),
        command("pull", "l", "view", Some("images")),
        command("up", "u", "anywhere", None),
        command("down", "d", "checkout", None),
    ];
    let heard = |key: &str| match route(&view(), &commands, None, &page(key)) {
        Some(Routed::Command(c)) => Some(c.command.clone()),
        _ => None,
    };
    assert_eq!(heard("p").as_deref(), Some("prune"));
    assert_eq!(heard("l"), None, "another view's");
    assert_eq!(heard("u").as_deref(), Some("up"));
    assert_eq!(heard("d"), None, "a checkout's is the rail's");
    assert_eq!(heard("z"), None);
}

#[test]
fn a_key_with_a_modifier_is_no_action_key() {
    let ctrl = PageKey { ctrl: true, ..page("r") };
    assert_eq!(route(&view(), &[], None, &ctrl), None);
}
