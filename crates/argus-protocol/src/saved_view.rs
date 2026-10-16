//! What a saved view is: an agent's view kept in the store, with the scope
//! it belongs to and the metadata an agent refreshes it from.
//!
//! A shown view (`agent_view`) lives and dies with its agent; a saved one is
//! a record of its own. An agent saves the same id again to refresh it in
//! place, and keeps nothing older: there are no versions. What it was built
//! from (`recipe`, `sources`, `built_commit`) is kept so that any agent, not
//! only the one that built it, can rebuild it. The daemon fills in the rest
//! (`built_at`, `built_commit`, `stale`); what an agent may write is
//! [`SavedViewWrite`], and what a client is sent is [`SavedView`].
//!
//! Scope is how a client finds a view, never a permission: a person sees
//! every saved view, and the scope only says which ones to offer first.

use serde::{Deserialize, Serialize};

use crate::agent_view::AgentView;
use crate::checkout_path::is_checkout_relative;
use crate::ids::RepositoryId;
use crate::view_check::{is_web_url, MAX_VIEW_ID_BYTES};

/// How a recipe is kept: the steps to build the view again, not a document.
pub const MAX_RECIPE_BYTES: usize = 4096;
pub const MAX_VIEW_SOURCES: usize = 32;
/// Features one view is attached to, besides the one it may be scoped to.
pub const MAX_ATTACHED_FEATURES: usize = 16;
/// Saved views in the store. A save past it is refused, with a reason, so a
/// loop saving a view per item cannot grow the store without bound.
pub const MAX_SAVED_VIEWS: usize = 500;
/// How many views deep an embed is followed. A view embedding another
/// embedding another is as deep as a reader follows; a client draws a line
/// saying so past it, which also ends a cycle across views.
pub const MAX_EMBED_DEPTH: usize = 3;
/// One source's text: a path, a URL or a note.
pub const MAX_SOURCE_BYTES: usize = 2048;
/// The longest body a save may arrive in: a view at its bound, and room for
/// a recipe and every source at theirs, with JSON's quoting on top, and the
/// keys, kinds, scope and features around them.
pub const MAX_SAVED_VIEW_WRITE_BYTES: usize = crate::view_check::MAX_VIEW_BYTES
    + 2 * (MAX_RECIPE_BYTES + MAX_VIEW_SOURCES * MAX_SOURCE_BYTES)
    + 4096;

/// A feature, named the way a client names one: the repository it is on and
/// its number there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureRef {
    pub repository: RepositoryId,
    pub number: i64,
}

/// Where a saved view belongs, as a client is told it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewScope {
    /// Not tied to any repository.
    Global,
    Repository { repository: RepositoryId },
    /// One feature of a repository. A feature that is removed leaves its
    /// views behind, scoped to the repository.
    Feature { repository: RepositoryId, number: i64 },
}

/// What a view was built from, for the agent that builds it again. Nothing
/// here is read by Argus: a `Url` is not fetched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewSource {
    /// A file of the checkout the view was saved from, relative to its root.
    File { path: String },
    Commit { sha: String },
    /// An http or https page the agent read.
    Url { url: String },
    Note { text: String },
    /// A kind a newer Argus knows; kept, and said to need one.
    #[serde(other)]
    Unknown,
}

/// A saved view as the daemon sends it to a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedView {
    /// The agent's stable name for it, unique across the store. The body's
    /// own `id` is this.
    pub id: String,
    pub scope: ViewScope,
    /// The features it is attached to, besides a feature scope.
    #[serde(default)]
    pub features: Vec<FeatureRef>,
    /// How to build it again, in the agent's words.
    #[serde(default)]
    pub recipe: Option<String>,
    #[serde(default)]
    pub sources: Vec<ViewSource>,
    /// When the last save landed, in Unix seconds by the daemon's clock.
    pub built_at: i64,
    /// The commit HEAD was on in the saving pane's checkout, when it was
    /// known.
    #[serde(default)]
    pub built_commit: Option<String>,
    /// Someone said it is out of date, or the checkout it was built in has
    /// moved on since. A save clears it.
    #[serde(default)]
    pub stale: bool,
    pub view: AgentView,
}

/// Where an agent puts a view, relative to the checkout it saves from: the
/// daemon names the repository and the feature's repository itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WriteScope {
    Global,
    /// This pane's repository.
    #[default]
    Repository,
    /// A feature of this pane's repository, by number.
    Feature { number: i64 },
}

/// What an agent sends to save a view: the view and what it was built from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedViewWrite {
    /// The saved view's id; the view's own when left out.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub scope: WriteScope,
    /// Feature numbers of this pane's repository to attach it to. A save
    /// replaces this repository's attachments and leaves others alone.
    #[serde(default)]
    pub features: Vec<i64>,
    #[serde(default)]
    pub recipe: Option<String>,
    #[serde(default)]
    pub sources: Vec<ViewSource>,
    pub view: AgentView,
}

impl SavedViewWrite {
    /// The write if it keeps the limits, with the id settled: the view's
    /// own id is the saved view's. A refusal names the part at fault.
    pub fn checked(mut self) -> Result<SavedViewWrite, String> {
        if let Some(id) = self.id.take() {
            self.view.id = id;
        }
        self.view = self.view.checked()?;
        // Same rules as the view's own id, which `checked` has just held
        // it to, but the saved id is also a link target and a path segment
        // clients show, so it must read back exactly as written.
        if self.view.id != self.view.id.trim() || self.view.id.len() > MAX_VIEW_ID_BYTES {
            return Err("id: no space around it".into());
        }
        if let Some(recipe) = &self.recipe {
            if recipe.len() > MAX_RECIPE_BYTES {
                return Err(format!("recipe: at most {MAX_RECIPE_BYTES} bytes"));
            }
        }
        if self.sources.len() > MAX_VIEW_SOURCES {
            return Err(format!("sources: at most {MAX_VIEW_SOURCES}"));
        }
        for (i, source) in self.sources.iter().enumerate() {
            source_ok(source).map_err(|why| format!("sources[{i}]: {why}"))?;
        }
        if self.features.len() > MAX_ATTACHED_FEATURES {
            return Err(format!("features: at most {MAX_ATTACHED_FEATURES}"));
        }
        if self.features.iter().any(|n| *n < 1) {
            return Err("features: feature numbers start at 1".into());
        }
        if let WriteScope::Feature { number } = self.scope {
            if number < 1 {
                return Err("scope: feature numbers start at 1".into());
            }
        }
        self.features.sort_unstable();
        self.features.dedup();
        Ok(self)
    }
}

fn source_ok(source: &ViewSource) -> Result<(), String> {
    let text = match source {
        ViewSource::File { path } if !is_checkout_relative(path) => {
            return Err("a file is inside the checkout: relative, with no `..`".into());
        }
        ViewSource::File { path } => path,
        ViewSource::Commit { sha } => {
            if !(7..=64).contains(&sha.len()) || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("a commit is its hex id, at least 7 characters".into());
            }
            sha
        }
        ViewSource::Url { url } if !is_web_url(url) => return Err("a url is an http or https URL".into()),
        ViewSource::Url { url } => url,
        ViewSource::Note { text } => text,
        ViewSource::Unknown => return Err("no source kind by that name".into()),
    };
    if text.len() > MAX_SOURCE_BYTES {
        return Err(format!("at most {MAX_SOURCE_BYTES} bytes"));
    }
    Ok(())
}

/// One row of an agent's list of saved views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedViewEntry {
    pub id: String,
    pub title: String,
    pub scope: WriteScope,
    pub stale: bool,
    pub built_at: i64,
}

/// A saved view as the agent that rebuilds it reads it: everything
/// [`SavedViewWrite`] takes, so the same JSON, changed, can be saved again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedViewRecord {
    pub id: String,
    pub scope: WriteScope,
    pub features: Vec<i64>,
    pub recipe: Option<String>,
    pub sources: Vec<ViewSource>,
    pub built_at: i64,
    pub built_commit: Option<String>,
    pub stale: bool,
    pub view: AgentView,
}

/// What an agent sends to the pane API's `saved-view` endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SavedViewChange {
    Save { view: Box<SavedViewWrite> },
    /// Answered with a [`SavedViewEntry`] list.
    List,
    /// Answered with a [`SavedViewRecord`].
    Get { id: String },
    Delete { id: String },
    /// Say the view is out of date, as a person's click does.
    Stale { id: String },
}

/// What is typed to the agent that last saved a view when the person asks
/// for it to be refreshed, on one line with no control character in it: a
/// newline would send the request early.
pub fn refresh_request(view: &AgentView, recipe: Option<&str>, sources: &[ViewSource]) -> String {
    use crate::view_press::plain;
    let mut text = format!(
        "Refresh the saved view `{id}` (\"{title}\")",
        id = plain(&view.id),
        title = plain(&view.title),
    );
    if let Some(recipe) = recipe.filter(|r| !r.trim().is_empty()) {
        text.push_str(&format!(". Recipe: {}", plain(recipe.trim())));
    }
    let sources: Vec<String> = sources
        .iter()
        .filter_map(|source| match source {
            ViewSource::File { path } => Some(format!("file {}", plain(path))),
            ViewSource::Commit { sha } => Some(format!("commit {}", plain(sha))),
            ViewSource::Url { url } => Some(format!("url {}", plain(url))),
            ViewSource::Note { text } => Some(format!("note {}", plain(text))),
            ViewSource::Unknown => None,
        })
        .collect();
    if !sources.is_empty() {
        text.push_str(&format!(". Sources: {}", sources.join("; ")));
    }
    text.push_str(&format!(
        ". Read them again with your own tools, then save it anew with \
         `argus-hook view save --stdin`, same id `{}`; `argus-hook view get {}` prints it as it is.",
        plain(&view.id),
        plain(&view.id),
    ));
    text
}

#[cfg(test)]
mod tests;
