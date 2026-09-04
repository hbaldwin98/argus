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

/// Which column of the board a feature sits in.
///
/// The states are the life of a piece of work and not a taxonomy: it is
/// proposed, someone is on it, it is stuck, it is offered for review, it
/// is accepted. `Submitted` and `Done` are separate on purpose — the agent
/// that did the work can reach the first and never the second, which is
/// the whole reason a human is in the loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeatureState {
    /// Written down, nobody on it. Where `feature open` leaves one.
    #[default]
    Proposed,
    Active,
    Blocked,
    /// Offered for review, with whatever evidence the worker gave.
    Submitted,
    Done,
}

impl FeatureState {
    pub const ALL: [FeatureState; 5] = [
        FeatureState::Proposed,
        FeatureState::Active,
        FeatureState::Blocked,
        FeatureState::Submitted,
        FeatureState::Done,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FeatureState::Proposed => "proposed",
            FeatureState::Active => "active",
            FeatureState::Blocked => "blocked",
            FeatureState::Submitted => "submitted",
            FeatureState::Done => "done",
        }
    }

    pub fn parse(text: &str) -> Option<FeatureState> {
        FeatureState::ALL
            .into_iter()
            .find(|s| s.as_str() == text.trim().to_ascii_lowercase())
    }

    /// Whether an agent may make this move itself. It may pick work up,
    /// say it is stuck, and offer what it has; it may not accept its own
    /// work, and it may not take back the acceptance.
    pub fn agent_may_enter(self) -> bool {
        !matches!(self, FeatureState::Done)
    }
}

impl std::fmt::Display for FeatureState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
    /// Which column it is in. Defaulted on the wire so a board written
    /// before the states existed reads as proposed rather than failing.
    #[serde(default)]
    pub state: FeatureState,
    /// The harness session that picked it up, not a pane id: a claim has
    /// to outlive the restart that hands out fresh ids.
    #[serde(default)]
    pub claimed_by: Option<String>,
    #[serde(default)]
    pub claimed_at: Option<i64>,
    /// Why it is stuck, set when it enters `Blocked` and cleared when it
    /// leaves. The history of blockers is in the events, not here.
    #[serde(default)]
    pub blocker: Option<String>,
    /// What was offered on submission. One field rather than a list: a
    /// resubmission replaces what it supersedes.
    #[serde(default)]
    pub evidence: Option<String>,
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
    /// Moves the current feature to another column. `detail` is the
    /// blocker when the target is `Blocked` and the evidence when it is
    /// `Submitted`; elsewhere it is a note on the move.
    Move {
        state: FeatureState,
        detail: Option<String>,
    },
}

/// One move between columns, in the order they happened.
///
/// Kept because the column a feature is in cannot say who put it there.
/// A human looking at three features submitted this morning wants to know
/// which agent submitted each and what it claimed, and the current state
/// has thrown all of that away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureEvent {
    pub id: i64,
    pub at: i64,
    pub slug: String,
    pub state: FeatureState,
    /// `agent` or `human` — which side made the move. Held rather than
    /// inferred from `session`, since a human write carries no session
    /// and so would be indistinguishable from a lost one.
    pub actor: String,
    pub session: Option<String>,
    pub detail: Option<String>,
}

/// One move, as the store is asked to make it.
///
/// Grouped rather than passed as six arguments because they are one thing:
/// who moved it where, when, and what they said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureMove {
    pub state: FeatureState,
    /// The blocker when blocking, the evidence when submitting, a note
    /// otherwise.
    pub detail: Option<String>,
    pub actor: Actor,
    pub session: Option<String>,
    pub at: i64,
}

/// Which side asked for a move, and so what it is allowed to ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    Human,
    Agent,
}

impl Actor {
    pub fn as_str(self) -> &'static str {
        match self {
            Actor::Human => "human",
            Actor::Agent => "agent",
        }
    }
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

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn a_state_survives_the_round_trip_through_its_name() {
        for state in FeatureState::ALL {
            assert_eq!(FeatureState::parse(state.as_str()), Some(state));
        }
        assert_eq!(FeatureState::parse(" Active "), Some(FeatureState::Active));
        assert_eq!(FeatureState::parse("shipped"), None);
    }

    #[test]
    fn an_agent_can_offer_work_but_not_accept_it() {
        assert!(FeatureState::Submitted.agent_may_enter());
        assert!(FeatureState::Blocked.agent_may_enter());
        assert!(
            !FeatureState::Done.agent_may_enter(),
            "the review column is where work stops, not passes through"
        );
    }

    /// The shape `Feature` had before the board states existed. Kept here
    /// rather than deleted with the old code: it is the only way to say
    /// what an older peer actually puts on the wire.
    #[derive(Serialize)]
    struct FeatureBeforeStates {
        slug: String,
        title: String,
        body: String,
        origin_checkout: Option<String>,
        origin_branch: Option<String>,
        at: i64,
        session: Option<String>,
    }

    #[test]
    fn a_feature_written_before_the_states_reads_as_proposed() {
        let old = FeatureBeforeStates {
            slug: "pty".into(),
            title: "The pty".into(),
            body: String::new(),
            origin_checkout: None,
            origin_branch: None,
            at: 1,
            session: None,
        };
        let bytes = rmp_serde::to_vec_named(&old).unwrap();
        let feature: Feature = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(feature.state, FeatureState::Proposed);
        assert_eq!(feature.claimed_by, None);
    }
}
