//! Features: the scope a decision belongs to.
//!
//! A project-wide decision board answers "what has this project ever
//! decided", which is not the question an agent picking up a feature has.
//! What it needs is the handful of choices made while building the thing
//! it is about to touch. So a decision is filed under a *feature*, and a
//! feature is a document: a title, prose the human or an agent can add to,
//! and the checkout and branch it originated in.
//!
//! The document half is what makes this more than a label. An agent
//! arriving on a branch reads what the feature is for and then the
//! decisions taken under it, which together are the context a transcript
//! would have held.

use serde::{Deserialize, Serialize};

use crate::decisions::Decision;
use crate::ids::ProjectId;

/// Past this a feature document has stopped being a brief and started
/// being a design document, which belongs in the checkout it describes.
pub const MAX_FEATURE_BODY_BYTES: usize = 8192;
pub const MAX_FEATURE_TITLE_BYTES: usize = 200;

/// One feature of a project, and the document that says what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feature {
    /// Stable within a project, and what a decision row carries. Derived
    /// from the title once, at creation: a title someone later rewords
    /// must not orphan the decisions filed under it.
    pub slug: String,
    pub title: String,
    /// The brief. Empty until someone writes one — a feature with no
    /// document is still a scope worth having.
    pub body: String,
    /// Where the work started. Not where it must stay: a feature outlives
    /// the worktree it was cut in, and the branch is often what a reader
    /// needs to find the code.
    pub origin_checkout: Option<String>,
    pub origin_branch: Option<String>,
    pub at: i64,
    /// Who opened it — an agent session, or `None` when the human did.
    pub session: Option<String>,
}

/// A feature as it is asked for, before the store gives it a slug.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureWrite {
    pub title: String,
    pub body: Option<String>,
}

impl FeatureWrite {
    pub fn checked(self) -> Result<FeatureWrite, &'static str> {
        let title = self.title.trim().to_string();
        let body = self.body.map(|b| b.trim().to_string()).filter(|b| !b.is_empty());
        if title.is_empty() {
            return Err("a feature has to have a title");
        }
        if title.len() > MAX_FEATURE_TITLE_BYTES {
            return Err("a feature title is a short noun phrase, not a paragraph");
        }
        if body.as_ref().is_some_and(|b| b.len() > MAX_FEATURE_BODY_BYTES) {
            return Err("a feature document is a brief, not a design document");
        }
        Ok(FeatureWrite { title, body })
    }
}

/// What an agent asks a feature endpoint to do.
///
/// One message rather than four endpoints because all four are the same
/// thing from the daemon's side: a change to which feature this checkout is
/// working on, or to that feature's document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureAction {
    /// Opens a feature and makes it this checkout's.
    Open(FeatureWrite),
    /// Points this checkout at an existing feature.
    Select { slug: String },
    /// Appends a paragraph to the current feature's document.
    Append { text: String },
}

/// A feature with the decisions filed under it, which is how an agent
/// reads one: the brief and the reasoning are one answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureBoard {
    pub project: Option<ProjectId>,
    pub project_name: String,
    /// Every feature of the project, oldest first, so an agent that is on
    /// the wrong one can see what else there is.
    pub features: Vec<Feature>,
    /// The feature this checkout is working on, if it has one.
    pub current: Option<String>,
    /// The current feature's decisions. Empty when there is no current
    /// feature — a board is only meaningful inside one.
    pub decisions: Vec<Decision>,
    /// Decisions recorded before features existed, or under a feature since
    /// gone. Reported so nothing is silently invisible, never mixed in.
    pub unfiled: usize,
}

/// A title turned into a key: lowercase, words joined by dashes, bounded.
///
/// Kept here rather than in the store because the helper prints slugs back
/// to agents and both sides have to agree on what one looks like.
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        if slug.len() >= MAX_SLUG_BYTES {
            break;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "feature".to_string()
    } else {
        slug
    }
}

pub const MAX_SLUG_BYTES: usize = 48;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_becomes_a_key_a_human_can_read() {
        assert_eq!(slugify("Scope decisions to a feature"), "scope-decisions-to-a-feature");
        assert_eq!(slugify("  PTY deadlock (again!)  "), "pty-deadlock-again");
    }

    #[test]
    fn a_title_with_nothing_usable_in_it_still_gets_a_key() {
        assert_eq!(slugify("!!!"), "feature");
    }

    #[test]
    fn a_slug_is_bounded_however_long_the_title_is() {
        assert!(slugify(&"word ".repeat(100)).len() <= MAX_SLUG_BYTES);
    }

    #[test]
    fn a_feature_has_to_have_a_title() {
        assert!(FeatureWrite { title: "  ".into(), body: None }.checked().is_err());
    }

    #[test]
    fn an_empty_document_is_dropped_rather_than_stored_blank() {
        let write = FeatureWrite { title: " decisions ".into(), body: Some("  ".into()) }
            .checked()
            .unwrap();
        assert_eq!(write.title, "decisions");
        assert_eq!(write.body, None);
    }
}
