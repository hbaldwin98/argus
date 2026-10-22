//! What the command palette lists: a row for each command, extension command and view, pane,
//! feature and checkout it can go to, each with the mark for its kind, its detail, what it says at
//! its end, and what choosing it asks of the session (the page's `keys/palette.ts`, `everything`).
//!
//! The search over them is `keys/rank`; a window draws the rows it keeps. A row's `key` is the same
//! across a rebuild, so a highlight can stay on it while the lists change under it.

use super::chords::{sentence, KeyChord};
use super::keymap::{Action, Command, Place};
use super::rank::{Candidate, Group};
use crate::connection::page::{Call, Palette};
use crate::extensions::keys::ExtCommandPage;
use crate::extensions::views::ExtTab;

/// The mark beside a row's text: what kind of thing it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Workspace,
    List,
    Branch,
    Code,
    Console,
    Search,
    Keyboard,
    Gear,
    Plus,
    Terminal,
    Puzzle,
    /// A command with no mark of its own.
    Chevron,
}

/// What a row says at its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aside {
    None,
    /// A command's keys, with the title of the place they work in when it is not Anywhere.
    Keys { place: Option<&'static str>, chords: Vec<KeyChord> },
    /// The standing of a pane or a feature, in its word.
    Word(&'static str),
    /// A checkout's git summary, shown only when it is not clean.
    Git(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Result {
    pub key: String,
    pub group: Group,
    /// What the search matches and the row shows.
    pub text: String,
    pub detail: Option<String>,
    pub mark: Mark,
    pub aside: Aside,
    pub does: Does,
}

/// What choosing a row asks of the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Does {
    Command(Action),
    ExtensionView { extension: String, view: String },
    ExtensionCommand { extension: String, command: String },
    Select(u64),
    GoToFeature(String),
    GoToCheckout(u64),
}

impl Does {
    /// The call that does it. `picked` is the checkout and pane the sidebar holds, which an
    /// extension's command is run on, as a key of its own is.
    pub fn call(&self, picked: Option<(u64, Option<u64>)>) -> Call {
        match self {
            Does::Command(action) => Call::Command { action: *action },
            Does::ExtensionView { extension, view } => Call::OpenExtensionView { extension: extension.clone(), view: view.clone() },
            Does::ExtensionCommand { extension, command } => Call::ExtensionCommand {
                extension: extension.clone(),
                command: command.clone(),
                checkout: picked.map(|(checkout, _)| checkout),
                pane: picked.and_then(|(_, pane)| pane),
            },
            Does::Select(pane) => Call::Select { pane: *pane },
            Does::GoToFeature(slug) => Call::GoToFeature { slug: slug.clone() },
            Does::GoToCheckout(checkout) => Call::GoToCheckout { checkout: *checkout },
        }
    }

    /// Choosing it gives the pane the keys: a pane that was gone to is typed in.
    pub fn gives_the_pane_the_keys(&self) -> bool {
        matches!(self, Does::Select(_))
    }
}

impl Result {
    pub fn candidate(&self) -> Candidate {
        Candidate { group: self.group, text: self.text.clone() }
    }
}

fn mark_of(action: Action) -> Mark {
    match action {
        Action::Workspace => Mark::Workspace,
        Action::Feature | Action::NewFeature => Mark::List,
        Action::Checkouts | Action::Branches => Mark::Branch,
        Action::Code | Action::OpenCode => Mark::Code,
        Action::Console | Action::ToggleConsole => Mark::Console,
        Action::Palette | Action::Search => Mark::Search,
        Action::Shortcuts => Mark::Keyboard,
        Action::Settings => Mark::Gear,
        Action::Compose | Action::StartAgent => Mark::Plus,
        Action::StartShell => Mark::Terminal,
        _ => Mark::Chevron,
    }
}

fn command(command: &Command) -> Result {
    let place = command.place.filter(|place| *place != Place::Anywhere).map(Place::title);
    Result {
        key: format!("command {:?}", command.action),
        group: Group::Commands,
        text: sentence(command.label),
        detail: None,
        mark: mark_of(command.action),
        aside: if command.chords.is_empty() {
            Aside::None
        } else {
            Aside::Keys { place, chords: command.chords.iter().map(KeyChord::from).collect() }
        },
        does: Does::Command(command.action),
    }
}

/// The results for every extension view and command.
fn extension(views: &[ExtTab], commands: &[ExtCommandPage]) -> Vec<Result> {
    let views = views.iter().map(|view| Result {
        key: format!("extension go {} {}", view.extension, view.view),
        group: Group::Commands,
        text: format!("Go to {}", view.title),
        detail: Some(view.extension.clone()),
        mark: Mark::Puzzle,
        aside: Aside::None,
        does: Does::ExtensionView { extension: view.extension.clone(), view: view.view.clone() },
    });
    let commands = commands.iter().map(|command| Result {
        key: format!("extension {} {}", command.extension, command.command),
        group: Group::Commands,
        text: command.title.clone(),
        detail: Some(command.extension.clone()),
        mark: Mark::Puzzle,
        aside: if command.chords.is_empty() {
            Aside::None
        } else {
            Aside::Keys { place: None, chords: command.chords.iter().map(KeyChord::from).collect() }
        },
        does: Does::ExtensionCommand { extension: command.extension.clone(), command: command.command.clone() },
    });
    views.chain(commands).collect()
}

/// Every row, the commands first, then the extensions', panes, features and checkouts, each in its
/// own order. `palette` is what the session says while the palette is open.
pub fn results(commands: &[Command], ext_views: &[ExtTab], ext_commands: &[ExtCommandPage], palette: Option<&Palette>) -> Vec<Result> {
    let mut rows: Vec<Result> = commands.iter().map(command).collect();
    rows.extend(extension(ext_views, ext_commands));
    let Some(palette) = palette else { return rows };
    rows.extend(palette.panes.iter().map(|pane| Result {
        key: format!("pane {}", pane.id),
        group: Group::Panes,
        text: pane.title.clone(),
        detail: Some(pane.place.clone()),
        mark: Mark::Terminal,
        aside: Aside::Word(pane.status),
        does: Does::Select(pane.id),
    }));
    rows.extend(palette.features.iter().map(|feature| Result {
        key: format!("feature {}", feature.slug),
        group: Group::Features,
        text: format!("#{} {}", feature.number, feature.title),
        detail: None,
        mark: Mark::List,
        aside: Aside::Word(feature.standing),
        does: Does::GoToFeature(feature.slug.clone()),
    }));
    rows.extend(palette.checkouts.iter().map(|checkout| Result {
        key: format!("checkout {}", checkout.id),
        group: Group::Checkouts,
        text: format!("{} / {}", checkout.repository, checkout.name),
        detail: Some(checkout.branch.clone()).filter(|branch| !branch.is_empty()),
        mark: Mark::Branch,
        aside: match &checkout.git {
            Some(git) if !checkout.clean => Aside::Git(git.clone()),
            _ => Aside::None,
        },
        does: Does::GoToCheckout(checkout.id),
    }));
    rows
}

/// What the empty list says: the features have not come yet under `#`, else nothing matched.
pub fn nothing(scoped_to_features: bool, palette: Option<&Palette>) -> &'static str {
    if scoped_to_features && !palette.is_some_and(|palette| palette.features_loaded) {
        "Reading the features\u{2026}"
    } else {
        "No match."
    }
}

/// Where the highlight goes when the rows are rebuilt: the row that has `held`'s key if there is
/// one, else the old place clamped to the last row (SHELL-PALETTE-8).
pub fn highlight(rows: &[&str], held: Option<&str>, at: usize) -> usize {
    held.and_then(|key| rows.iter().position(|row| *row == key)).unwrap_or_else(|| at.min(rows.len().saturating_sub(1)))
}

#[cfg(test)]
mod tests;
