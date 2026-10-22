//! The brand's menu: the project switcher (SHELL-SWITCH-1..5), as the kit's menu draws it.
//!
//! What it offers and what each entry says is core's (`chrome::projects`); this makes it a list of
//! `MenuItem<Does>` like the rail's other menus. A choice names the calls it sends and changes
//! nothing: a project, workspace or host is shown chosen when the daemon says so. The page unfolds
//! Switch workspace and Connect to a host in place; here each opens beside the menu (`MenuItem::fold`).

use argus_desktop_core::chrome::projects::{menu, Entry, Tone, HostRow, WorkspaceRow, NO_HOSTS};
use argus_desktop_core::connection::page::{Call, Rail};

use super::menus::Does;
use crate::kit::menu::item::{MenuItem, Tone as Voice};

fn voice(tone: Tone) -> Voice {
    match tone {
        Tone::Loud => Voice::Loud,
        Tone::Active => Voice::Active,
        Tone::Muted => Voice::Muted,
    }
}

fn asked(call: Option<Call>) -> Does {
    Does::Send(call.into_iter().collect())
}

fn workspace(row: &WorkspaceRow) -> MenuItem<Does> {
    MenuItem::choice(row.name.clone(), asked(row.choose())).marked(row.open).noted(row.count.clone())
}

fn host(row: &HostRow) -> MenuItem<Does> {
    MenuItem::choice(row.name.clone(), asked(row.choose())).marked(row.current)
}

/// The menu for what the rail last said.
pub fn items(rail: &Rail) -> Vec<MenuItem<Does>> {
    menu(rail)
        .into_iter()
        .map(|entry| match entry {
            Entry::Label(words) => MenuItem::heading(words),
            Entry::Project { id, name, current, tally } => {
                MenuItem::choice(name, Does::Go(vec![Call::SelectProject { project: id }])).marked(current).toned(tally.words, voice(tally.tone))
            }
            Entry::Rule => MenuItem::Rule,
            Entry::AddProject => MenuItem::choice("Add a project\u{2026}", Does::Send(vec![Call::OpenAddProject])).led("+"),
            Entry::Workspaces { open, rows } => MenuItem::fold("Switch workspace\u{2026}", open, rows.iter().map(workspace).collect()),
            Entry::Hosts { connected, rows } => {
                let none = rows.len() == 1;
                let mut listed: Vec<MenuItem<Does>> = rows.iter().map(host).collect();
                if none {
                    listed.push(MenuItem::heading(NO_HOSTS));
                }
                MenuItem::fold("Connect to a host\u{2026}", connected, listed)
            }
            Entry::Remove { project, name } => {
                MenuItem::choice(format!("Remove {name}\u{2026}"), Does::Send(vec![Call::RemoveProject { project }])).danger()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
