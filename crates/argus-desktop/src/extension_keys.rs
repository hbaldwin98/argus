//! The extensions' keys as the window's: each command, where its key is
//! heard and what the palette calls it, and each view's action keys — a
//! heading of the shortcuts sheet per extension, beside the keymap's own.
//!
//! The daemon already refused any key the window keeps
//! (`argus_protocol::extension_keys`), so a key here never shadows one of
//! `keymap`'s; the window only decides where each is heard.

use argus_protocol::{CommandScope, ExtCommand, ExtKey, ExtView};

use crate::keys::PageKey;
use crate::page::{ExtBinding, ExtChord, ExtCommandPage, ExtGroup};

#[derive(Default)]
pub struct ExtensionKeys {
    commands: Vec<ExtCommand>,
}

/// `press` as a binding's chord crosses, after the leader when `leader`.
fn chord(press: &ExtKey, leader: bool) -> ExtChord {
    ExtChord { key: press.key.clone(), ctrl: press.ctrl, alt: press.alt, shift: press.shift, meta: false, leader }
}

/// Whether `page` is the key `press`, as `keymap::Chord::matches` decides:
/// a character carries its own shift.
pub fn matches(press: &ExtKey, page: &PageKey) -> bool {
    page.key == press.key
        && page.ctrl == press.ctrl
        && page.alt == press.alt
        && !page.meta
        && (press.is_character() || page.shift == press.shift)
}

impl ExtensionKeys {
    pub fn receive(&mut self, commands: Vec<ExtCommand>) {
        self.commands = commands;
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Whether `extension` has a command `name`.
    pub fn holds(&self, extension: &str, name: &str) -> bool {
        self.commands.iter().any(|c| c.extension == extension && c.name == name)
    }

    /// The command a key after the leader in a pane runs: one heard
    /// anywhere, or on a pane.
    pub fn after_leader(&self, page: &PageKey) -> Option<&ExtCommand> {
        self.commands.iter().find(|c| {
            matches!(c.scope, CommandScope::Anywhere | CommandScope::Pane) && c.key.as_ref().is_some_and(|k| matches(k, page))
        })
    }

    /// Every command, as the palette lists it and the page dispatches its
    /// key.
    pub fn page(&self) -> Vec<ExtCommandPage> {
        self.commands
            .iter()
            .map(|c| ExtCommandPage {
                extension: c.extension.clone(),
                command: c.name.clone(),
                title: c.title.clone(),
                chords: c.key.iter().map(|k| chord(k, false)).collect(),
                scope: match c.scope {
                    CommandScope::Anywhere => "anywhere",
                    CommandScope::View(_) => "view",
                    CommandScope::Checkout => "checkout",
                    CommandScope::Pane => "pane",
                },
                view: match &c.scope {
                    CommandScope::View(view) => Some(view.clone()),
                    _ => None,
                },
            })
            .collect()
    }

    /// A heading per extension: its commands' keys, where each is heard,
    /// and its views' action keys.
    pub fn groups(&self, views: &[ExtView]) -> Vec<ExtGroup> {
        let mut groups: Vec<ExtGroup> = Vec::new();
        let mut add = |extension: &str, binding: ExtBinding| {
            let title = format!("Extensions · {extension}");
            match groups.iter_mut().find(|g| g.title == title) {
                Some(group) => group.bindings.push(binding),
                None => groups.push(ExtGroup { title, bindings: vec![binding] }),
            }
        };
        for command in &self.commands {
            let Some(press) = &command.key else { continue };
            let (label, chords) = match &command.scope {
                CommandScope::Anywhere => (command.title.clone(), vec![chord(press, false), chord(press, true)]),
                CommandScope::Pane => (format!("{} (on a pane)", command.title), vec![chord(press, false), chord(press, true)]),
                CommandScope::Checkout => (format!("{} (on a checkout)", command.title), vec![chord(press, false)]),
                CommandScope::View(view) => (format!("{} (in {view})", command.title), vec![chord(press, false)]),
            };
            add(&command.extension, ExtBinding { label, chords });
        }
        for view in views {
            // A view has no key of its own, the digits being the built-in
            // stages'; the sheet says where it is reached.
            let label = format!("Go to {} (the sidebar's Extensions, or the palette)", view.title);
            add(&view.extension, ExtBinding { label, chords: Vec::new() });
            for action in &view.actions {
                let Some(key) = &action.key else { continue };
                let label = format!("{} (in {})", action.label, view.title);
                add(&view.extension, ExtBinding { label, chords: vec![chord(&ExtKey::plain(key), false)] });
            }
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> ExtensionKeys {
        let commands = r#"[
            {"extension": "docker", "name": "up", "title": "Compose up", "key": {"key": "u", "ctrl": true}},
            {"extension": "docker", "name": "shell", "title": "Shell in", "key": {"key": "T"}, "scope": "pane"},
            {"extension": "docker", "name": "prune", "title": "Prune", "key": {"key": "I"}, "scope": {"view": "containers"}},
            {"extension": "docker", "name": "pick", "title": "Pick a service"}
        ]"#;
        let mut keys = ExtensionKeys::default();
        keys.receive(serde_json::from_str(commands).unwrap());
        keys
    }

    fn page(key: &str) -> PageKey {
        PageKey { key: key.into(), ..PageKey::default() }
    }

    #[test]
    fn after_the_leader_only_a_command_heard_anywhere_or_on_a_pane_runs() {
        let keys = keys();
        assert_eq!(keys.after_leader(&PageKey { ctrl: true, ..page("u") }).unwrap().name, "up");
        assert_eq!(keys.after_leader(&PageKey { shift: true, ..page("T") }).unwrap().name, "shell");
        assert!(keys.after_leader(&page("I")).is_none(), "a view's own");
        assert!(keys.after_leader(&page("u")).is_none(), "ctrl is asked");
    }

    #[test]
    fn an_extensions_keys_are_one_heading_of_the_sheet_and_every_command_is_in_the_palette() {
        let keys = keys();
        let views: Vec<ExtView> = serde_json::from_str(
            r#"[{"extension": "docker", "name": "containers", "title": "Containers", "columns": [{"title": "Name"}],
                 "actions": [{"id": "restart", "label": "Restart", "key": "r"}, {"id": "logs", "label": "Logs"}]}]"#,
        )
        .unwrap();
        let groups = keys.groups(&views);
        assert_eq!(groups.len(), 1);
        let labels: Vec<&str> = groups[0].bindings.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, [
            "Compose up",
            "Shell in (on a pane)",
            "Prune (in containers)",
            "Go to Containers (the sidebar's Extensions, or the palette)",
            "Restart (in Containers)"
        ]);
        assert!(groups[0].bindings[0].chords[1].leader, "and through the leader from a pane");
        assert_eq!(keys.page().len(), 4, "a command with no key is still the palette's");
    }
}
