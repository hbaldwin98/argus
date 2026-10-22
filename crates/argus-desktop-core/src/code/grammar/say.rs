//! What the editor's bar says about a grammar: the offer to install one, and
//! what became of installing it. The words are the page's
//! (`ts:code/editor.ts:328-355,408-420`); a window draws them in its own bar.

use super::fetch::Offer;

/// A size as the offer tells it: whole kilobytes rounded up, or megabytes to
/// a decimal from 1 MiB.
pub fn size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

/// The offer as two runs of the bar's text: the question, and where the
/// grammar comes from, which the page sets in the mono font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub question: String,
    pub from: String,
}

pub fn ask(offer: &Offer) -> Ask {
    let bytes = size(offer.bytes());
    let from = if offer.pinned {
        format!("{} from npm, {bytes}", offer.source)
    } else {
        let by = offer.publisher.as_deref().map(|p| format!(" by {p}")).unwrap_or_default();
        format!("{} from npm{by}, {bytes}; found by searching, not checked by Argus", offer.source)
    };
    Ask { question: format!("No colours for {} yet. Install its tree-sitter grammar? ", offer.label), from }
}

pub fn installing(offer: &Offer) -> String {
    format!("Installing the {} grammar…", offer.label)
}

pub fn could_not_install(offer: &Offer, message: &str) -> String {
    format!("Could not install {}: {message}", offer.source)
}

pub fn could_not_load(error: &str) -> String {
    format!("Could not load the grammar: {error}")
}

#[cfg(test)]
mod tests;
