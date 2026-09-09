//! Features: the scope a decision belongs to.
//!
//! A broad decision board answers "what has this project ever
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
use crate::tasks::TaskCounts;
use crate::ids::ProjectId;

/// Past this a feature document has stopped being a brief and started
/// being a design document, which belongs in the checkout it describes.
pub const MAX_FEATURE_BODY_BYTES: usize = 8192;
pub const MAX_FEATURE_TITLE_BYTES: usize = 200;

/// Whether a feature is still being worked on, or accepted.
///
/// Two states, not the five columns this used to be. The others —
/// proposed, active, blocked, submitted — were the one thing in Argus a
/// person had to maintain by hand: nothing observed them, and a column
/// meaning "somebody last dragged this here" is stale the moment the work
/// moves on. What they were reaching for is read off the panes on the
/// feature's checkouts and the state of its tasks instead, which cannot
/// go stale because nobody maintains it.
///
/// `Done` stays a stored state, and stays the human's: accepting work is
/// the one judgement no observation can stand in for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FeatureState {
    #[default]
    Open,
    Done,
}

/// Read by name rather than by derive, so a state this build does not know
/// reads as open instead of failing the whole board.
///
/// `#[serde(default)]` on the field does not cover this: it fills in a
/// *missing* field and has nothing to say about a present one holding
/// `proposed`. A peer built before the columns were collapsed sends
/// exactly that, and without this the answer it is reading is not one
/// unknown feature but no features at all — which is what a reader sees
/// as "invalid daemon response". The same tolerance the store's read has,
/// for the same reason, and the protocol has no version negotiation yet
/// to arrange it any other way (TARGET.md, "Protocol and safety").
impl<'de> Deserialize<'de> for FeatureState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<FeatureState, D::Error> {
        let name = String::deserialize(d)?;
        Ok(FeatureState::parse(&name).unwrap_or_default())
    }
}

impl FeatureState {
    pub const ALL: [FeatureState; 2] = [FeatureState::Open, FeatureState::Done];

    pub fn as_str(self) -> &'static str {
        match self {
            FeatureState::Open => "open",
            FeatureState::Done => "done",
        }
    }

    /// Parses a stored or typed state, including the four the board used
    /// to have. `feature_event` keeps the name a move was recorded under,
    /// so those names outlive the columns and still have to read as
    /// something rather than fail the whole board.
    pub fn parse(text: &str) -> Option<FeatureState> {
        match text.trim().to_ascii_lowercase().as_str() {
            "done" => Some(FeatureState::Done),
            "open" | "proposed" | "active" | "blocked" | "submitted" => Some(FeatureState::Open),
            _ => None,
        }
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
    /// Open, or accepted. Defaulted on the wire so a feature written
    /// before the state existed reads as open rather than failing.
    #[serde(default)]
    pub state: FeatureState,
    /// Every checkout currently pointed at this feature, plus the one it
    /// was cut in. Carried so a reader can connect a feature to the panes
    /// running on it: without it the feature list is an island, describing
    /// work with no way to see whether anything is happening to it.
    #[serde(default)]
    pub checkouts: Vec<String>,
    /// How its tasks stand. On the feature rather than fetched per row
    /// because a list wants every feature's counts at once, and a round
    /// trip per row is not a list.
    #[serde(default)]
    pub tasks: TaskCounts,
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

// There is no agent-side move. The only state left is `Done`, which is
// acceptance, and the agent that did the work is the one party that
// cannot accept it — so the whole action would be a refusal.

/// One state change, in the order they happened.
///
/// Kept because the state a feature is in cannot say who put it there. A
/// human looking at three features accepted this morning wants to know who
/// accepted each and what they said, and the current state has thrown all
/// of that away. Rows written under the five old column names are still
/// here and still read: [`FeatureState::parse`] maps them onto `open`.
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
    /// Whatever the mover said about it.
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
        assert_eq!(FeatureState::parse(" Done "), Some(FeatureState::Done));
        assert_eq!(FeatureState::parse("shipped"), None);
    }

    #[test]
    fn the_columns_the_board_used_to_have_still_read_as_open() {
        for old in ["proposed", "active", "blocked", "submitted"] {
            assert_eq!(
                FeatureState::parse(old),
                Some(FeatureState::Open),
                "{old} is still written on every feature_event recorded before the cut"
            );
        }
    }

    /// A peer that predates the collapse, which for a while is every
    /// daemon anyone has running.
    #[test]
    fn a_state_from_before_the_collapse_arrives_as_open_rather_than_failing() {
        for old in ["proposed", "active", "blocked", "submitted"] {
            let bytes = rmp_serde::to_vec_named(&old).unwrap();
            assert_eq!(
                rmp_serde::from_slice::<FeatureState>(&bytes).unwrap(),
                FeatureState::Open,
                "{old} must not take the whole board down with it"
            );
        }
        // And a state only a newer Argus knows, for the same reason: one
        // unreadable row is a better answer than no features at all.
        let bytes = rmp_serde::to_vec_named(&"archived").unwrap();
        assert_eq!(
            rmp_serde::from_slice::<FeatureState>(&bytes).unwrap(),
            FeatureState::Open
        );
    }

    /// The whole reason the tolerance is on the state and not the field:
    /// a feature is what actually travels, and one bad state used to mean
    /// no features at all.
    #[test]
    fn a_feature_from_an_older_peer_still_arrives() {
        #[derive(Serialize)]
        struct FeatureWithOldColumns {
            slug: String,
            title: String,
            body: String,
            origin_checkout: Option<String>,
            origin_branch: Option<String>,
            at: i64,
            session: Option<String>,
            state: String,
            claimed_by: Option<String>,
            blocker: Option<String>,
        }
        let old = FeatureWithOldColumns {
            slug: "pty".into(),
            title: "The pty".into(),
            body: String::new(),
            origin_checkout: None,
            origin_branch: None,
            at: 1,
            session: None,
            state: "submitted".into(),
            claimed_by: Some("sess-1".into()),
            blocker: None,
        };
        let bytes = rmp_serde::to_vec_named(&old).unwrap();
        let feature: Feature = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(feature.slug, "pty");
        assert_eq!(feature.state, FeatureState::Open);
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
    fn a_feature_written_before_the_states_reads_as_open() {
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
        assert_eq!(feature.state, FeatureState::Open);
        assert_eq!(feature.checkouts, Vec::<String>::new());
        assert_eq!(feature.tasks, TaskCounts::default());
    }
}
