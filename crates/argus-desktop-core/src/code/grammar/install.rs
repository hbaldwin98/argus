//! Finding a grammar when a file opens and installing it on request: the
//! installed one colours at once; otherwise a pinned grammar is installed
//! if Settings says not to ask, and every other is offered and waits for a
//! yes. Nothing is kept that did not load.
//!
//! The decision of `ts:code/editor.ts` `findGrammar` and the install of
//! `ts:code/grammar/load.ts`, without the bar they are shown in.

use std::collections::HashSet;
use std::sync::Arc;

use super::fetch::{download, offer_for, Offer};
use super::load::{load, Loaded, Loader};
use super::net::Http;
use super::say;

/// What opening a file found for its colours.
pub enum Found {
    /// Installed and loaded: colour every editor of the kind with it.
    Colour(Arc<Loaded>),
    /// Installed and will not load; the sentence for the bar.
    Failed(String),
    /// There is one to offer, and the person is asked.
    Offer(Offer),
    /// A pinned one and Settings says not to ask: install it now.
    Install(Offer),
    Nothing,
}

/// The file's grammar if it is installed; otherwise what to offer, or the
/// install itself where `auto` (Settings → "Install known grammars without
/// asking") covers a pinned one. A searched grammar is always asked about, and
/// one the person said "Not now" to this window session (`declined`, by name)
/// is not offered again.
pub fn find(path: &str, loader: &Loader, http: &dyn Http, auto: bool, declined: &HashSet<String>) -> Found {
    match loader.installed_for(path) {
        Ok(Some(loaded)) => return Found::Colour(loaded),
        Ok(None) => {}
        Err(error) => return Found::Failed(say::could_not_load(&error)),
    }
    match offer_for(path, http) {
        Some(offer) if !declined.contains(&offer.name) => {
            if offer.pinned && auto {
                Found::Install(offer)
            } else {
                Found::Offer(offer)
            }
        }
        _ => Found::Nothing,
    }
}

/// Downloads, loads and keeps what `offer` names: kept only once it has
/// loaded and its query compiled, so a grammar that will not run is never
/// installed. The error is the message to follow `say::could_not_install`.
pub fn install(offer: &Offer, loader: &Loader, http: &dyn Http) -> Result<Arc<Loaded>, String> {
    let grammar = download(offer, http)?;
    let loaded = load(&grammar)?;
    if let Err(error) = loader.store().keep(&grammar) {
        // It still colours this window; the next one asks again.
        tracing::warn!(grammar = %grammar.name, %error, "could not keep an installed grammar");
    }
    Ok(loader.adopt(loaded))
}

#[cfg(test)]
mod tests;
