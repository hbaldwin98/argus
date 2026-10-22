//! What a saved view's column says about itself: when it was built and at which commit, what the
//! Details fold lists, and whether the panel draws a head of its own.
//!
//! The page writes these in `v/saved.ts` beside the elements; here they are functions of the
//! page's data and a clock, so the window draws what they return and the rules are tested once
//! (CHECKOUTS-CODE-VIEWS-SAVED-4, -6, -8).

use crate::layout::docks::{DocksPage, Panel};
use crate::system::clock::Clock;
use crate::views::saved::PageSaved;
use crate::views::values::{ago, local_time};

/// A commit as the meta row cuts it.
pub fn commit_short(commit: &str) -> String {
    commit.chars().take(7).collect()
}

/// A commit as Details cuts it.
pub fn commit_long(commit: &str) -> String {
    commit.chars().take(12).collect()
}

/// The moment a view was built, in the form the clock reads: Unix seconds as RFC 3339.
fn moment(built_at: i64) -> String {
    jiff::Timestamp::from_second(built_at).map(|at| at.to_string()).unwrap_or_default()
}

/// The meta row's `built <ago>`, and the exact local date-time that is its tooltip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    pub said: String,
    pub exact: String,
}

/// `built 4 min ago at 6ce099a`, with ` · asked to build it again` while a refresh is pending.
pub fn built(clock: &Clock, view: &PageSaved) -> Built {
    let at = moment(view.built_at);
    let mut said = format!("built {}", ago(clock, &at));
    if let Some(commit) = view.built_commit.as_deref().filter(|commit| !commit.is_empty()) {
        said.push_str(&format!(" at {}", commit_short(commit)));
    }
    if view.refreshing {
        said.push_str(" \u{b7} asked to build it again");
    }
    Built { said, exact: local_time(clock, &at).unwrap_or_default() }
}

/// What a source is, for the Details list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub kind: String,
    pub text: String,
    /// A `url` source is a link, opened in the system browser.
    pub link: bool,
}

/// What the Details fold lists, in its order; a row with nothing to say is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    pub scope: String,
    pub built: String,
    pub features: Option<String>,
    pub recipe: Option<String>,
    pub sources: Vec<Source>,
}

pub fn details(clock: &Clock, view: &PageSaved) -> Details {
    let at = moment(view.built_at);
    let mut built = local_time(clock, &at).unwrap_or_default();
    if let Some(commit) = view.built_commit.as_deref().filter(|commit| !commit.is_empty()) {
        built.push_str(&format!(" \u{b7} commit {}", commit_long(commit)));
    }
    Details {
        scope: view.scope.label.clone(),
        built,
        features: (!view.features.is_empty()).then(|| view.features.iter().map(|f| format!("#{}", f.number)).collect::<Vec<_>>().join(", ")),
        recipe: view.recipe.clone().filter(|recipe| !recipe.is_empty()),
        sources: view.sources.iter().map(|s| Source { kind: s.kind.to_owned(), text: s.text.clone(), link: s.kind == "url" }).collect(),
    }
}

/// Whether the panel has a head of its own, which carries the stale chip and the column count. A
/// lone view in a dock with a tab strip is named, closed and moved by its tab, so a second row saying
/// the same would be the view's name twice over (SAVED-4). A floating panel's head is its frame's
/// title bar, so it has one.
pub fn headed(columns: usize, panel: &Panel, docks: &DocksPage) -> bool {
    let tabbed = [&docks.left, &docks.right, &docks.bottom].iter().any(|dock| dock.tabs.len() > 1 && dock.tabs.iter().any(|tab| &tab.panel == panel));
    !(columns == 1 && tabbed)
}

/// Whether the panel is a dialog over the stage.
pub fn floating(panel: &Panel, docks: &DocksPage) -> bool {
    docks.floating.iter().any(|float| &float.panel == panel)
}

#[cfg(test)]
mod tests;
