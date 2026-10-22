//! What the editor's bar holds of a file's grammar: nothing, an offer waiting on a yes, an install
//! under way, or the sentence of how one failed (`ts:code/editor.ts` `grammarBar`, `notNow`).
//! The transitions are here, so a window only draws the slot and sends the person's answer.

use std::collections::HashSet;

use super::fetch::Offer;
use super::install::Found;
use super::say::{self, Ask};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Slot {
    #[default]
    None,
    /// A grammar to install if the person says yes.
    Offer(Offer),
    Installing(Offer),
    /// The grammar installed will not load, or one would not install.
    Said(String),
}

/// What the bar draws for a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// The question with Install and Not now beside it.
    Ask(Ask),
    Text(String),
}

impl Slot {
    /// What opening a file found, as the slot it leaves. A grammar that is already there leaves
    /// none; a pinned one Settings says to install is installing from the first draw.
    pub fn found(found: &Found) -> Slot {
        match found {
            Found::Colour(_) | Found::Nothing => Slot::None,
            Found::Failed(said) => Slot::Said(said.clone()),
            Found::Offer(offer) => Slot::Offer(offer.clone()),
            Found::Install(offer) => Slot::Installing(offer.clone()),
        }
    }

    /// The person said yes to the offer: the one to install, and the slot while it installs.
    pub fn install(&mut self) -> Option<Offer> {
        let Slot::Offer(offer) = std::mem::take(self) else { return None };
        *self = Slot::Installing(offer.clone());
        Some(offer)
    }

    /// The install is over: nothing is left to say if it worked, and why not if it did not.
    pub fn installed(offer: &Offer, result: &Result<(), String>) -> Slot {
        match result {
            Ok(()) => Slot::None,
            Err(message) => Slot::Said(say::could_not_install(offer, message)),
        }
    }

    /// The person said "Not now": the grammar is not offered again this window, and the slot
    /// empties.
    pub fn not_now(&mut self, declined: &mut HashSet<String>) {
        if let Slot::Offer(offer) = std::mem::take(self) {
            declined.insert(offer.name);
        }
    }

    /// What the bar shows, if anything: never while a clash is held, which has the bar's room.
    pub fn shown(&self, clashing: bool) -> Option<Shown> {
        if clashing {
            return None;
        }
        match self {
            Slot::None => None,
            Slot::Offer(offer) => Some(Shown::Ask(say::ask(offer))),
            Slot::Installing(offer) => Some(Shown::Text(say::installing(offer))),
            Slot::Said(text) => Some(Shown::Text(text.clone())),
        }
    }
}

/// Whether two files are of one kind for a grammar: the same extension, as the page reads it (the
/// text after the last dot of the name, case ignored, and the whole path when the name has no
/// dot, so two extensionless files in different folders are never one kind). A grammar installed
/// for one colours every open editor of the kind (EDITOR-GRAM-6).
pub fn same_kind(a: &str, b: &str) -> bool {
    fn ext(path: &str) -> String {
        let name = path.rsplit('/').next().unwrap_or(path);
        match name.rfind('.') {
            Some(dot) => name[dot + 1..].to_lowercase(),
            None => path.to_lowercase(),
        }
    }
    ext(a) == ext(b)
}

#[cfg(test)]
mod tests;
