//! The decision board: what an agent chose, and what it chose against.
//!
//! A log of decisions answers "what happened". A tree answers the
//! question people actually come back with, which is "why is it like
//! this" — and that answer is mostly the road not taken. So a decision
//! names what it was chosen *over* as well as what was chosen, and hangs
//! off the decision that constrained it.
//!
//! A decision that turns out to be wrong is superseded by a new one, which
//! points back at it; the old node stays on the board, drawn dimmed.
//! Editing it away would leave a board that agrees with itself and explains
//! nothing. What a person may correct is a decision's prose, the way they
//! correct a brief, and what anyone may do to a question is answer it.
//!
//! A decision is not always made yet. One still `Open` is a question the
//! work is waiting on, with the answers being weighed; one `Rejected` is an
//! answer that was turned down, kept because it is the road not taken.

use serde::{Deserialize, Serialize};

use crate::features::Feature;
use crate::ids::{ProjectId, RepositoryId};
use crate::tracker_links::TrackerMap;

/// Past this a field has stopped being a decision and started being a
/// design document, which belongs in the checkout it describes.
pub const MAX_DECISION_BYTES: usize = 1024;
/// A question with more answers than this has not been thought about yet.
pub const MAX_DECISION_OPTIONS: usize = 9;
pub const MAX_DECISION_CONSEQUENCES: usize = 12;

/// Whether a decision stands, is still being asked, or was turned down.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionState {
    /// Made, and what the work is built on.
    #[default]
    Accepted,
    /// A question nobody has answered. Its `chose` is the question, and it
    /// holds the tasks beside it until somebody answers.
    Open,
    /// An answer that was considered and turned down.
    Rejected,
}

/// Read by name so a state a later build adds reads as accepted instead of
/// failing the whole board, for the reason `FeatureState` gives.
impl<'de> Deserialize<'de> for DecisionState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<DecisionState, D::Error> {
        let name = String::deserialize(d)?;
        Ok(DecisionState::parse(&name).unwrap_or_default())
    }
}

impl DecisionState {
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionState::Accepted => "accepted",
            DecisionState::Open => "open",
            DecisionState::Rejected => "rejected",
        }
    }

    pub fn parse(text: &str) -> Option<DecisionState> {
        match text.trim().to_ascii_lowercase().as_str() {
            "accepted" => Some(DecisionState::Accepted),
            "open" => Some(DecisionState::Open),
            "rejected" => Some(DecisionState::Rejected),
            _ => None,
        }
    }

    /// How a decision typed on one line stands, by its first character:
    /// the note grammar's `?` asks a question and `!` records one made, and
    /// `~` is an answer that was turned down. Both clients read a typed
    /// decision this way, so a line means the same in either.
    pub fn typed(line: &str) -> (DecisionState, &str) {
        let line = line.trim();
        let (state, rest) = match line.chars().next() {
            Some('?') => (DecisionState::Open, &line[1..]),
            Some('~') => (DecisionState::Rejected, &line[1..]),
            Some('!') => (DecisionState::Accepted, &line[1..]),
            _ => (DecisionState::Accepted, line),
        };
        (state, rest.trim())
    }
}

impl std::fmt::Display for DecisionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing a decision costs or buys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consequence {
    /// Whether it is something gained rather than something paid.
    pub gain: bool,
    pub text: String,
}

impl Consequence {
    /// Reads a line as it is written: a leading `-` is a cost, a leading
    /// `+` or nothing a gain. `None` for a line that says nothing.
    pub fn parse(line: &str) -> Option<Consequence> {
        let line = line.trim();
        let (gain, text) = match line.chars().next()? {
            '+' => (true, &line[1..]),
            '-' => (false, &line[1..]),
            '−' => (false, &line['−'.len_utf8()..]),
            _ => (true, line),
        };
        let text = text.trim();
        (!text.is_empty()).then(|| Consequence {
            gain,
            text: text.to_string(),
        })
    }

    pub fn sign(&self) -> char {
        if self.gain { '+' } else { '-' }
    }
}

/// One recorded decision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// Durable for the life of the board, unlike a pane or project id:
    /// this is what an agent passes back to hang the next decision off.
    pub id: i64,
    /// The decision this one descends from, or `None` at the root.
    pub parent: Option<i64>,
    pub at: i64,
    /// The agent session that recorded it. A board with no attribution is
    /// a board nobody can ask a follow-up question about.
    pub session: Option<String>,
    /// The checkout it was decided in, as a path. Two agents on two
    /// branches make different decisions, and which branch is often the
    /// whole explanation.
    pub checkout: Option<String>,
    /// The feature it was decided under, by slug. `None` for a decision
    /// recorded before features existed: those stay on the board unfiled
    /// rather than being dragged under a feature nobody chose.
    #[serde(default)]
    pub feature: Option<String>,
    /// What was chosen. The one thing a decision cannot be recorded
    /// without.
    pub chose: String,
    /// What it was chosen over.
    pub over: Option<String>,
    /// What forced it.
    pub because: Option<String>,
    /// The decision that replaced this one. Set on the old node when a new
    /// one supersedes it, so a reader walking down the tree sees that this
    /// branch was abandoned without having to hunt for the reversal.
    pub superseded_by: Option<i64>,
    /// When the decision was withdrawn, if it has been: taken back as a
    /// mistake rather than replaced by a better one. Kept, not deleted, so
    /// the history still says it was once decided; agents are not shown it.
    #[serde(default)]
    pub withdrawn_at: Option<i64>,
    #[serde(default)]
    pub state: DecisionState,
    /// The task it was raised under. That one link is what binds the task
    /// and everything beneath it to the decision, and what an open one
    /// holds up.
    #[serde(default)]
    pub task: Option<i64>,
    /// What was true that made a decision necessary.
    #[serde(default)]
    pub context: Option<String>,
    /// The decision in full, where `chose` is only its name.
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub consequences: Vec<Consequence>,
    /// The answers an open decision is choosing between.
    #[serde(default)]
    pub options: Vec<String>,
}

impl Decision {
    pub fn superseded(&self) -> bool {
        self.superseded_by.is_some()
    }

    pub fn withdrawn(&self) -> bool {
        self.withdrawn_at.is_some()
    }

    /// Whether it is a question still waiting on somebody.
    pub fn open(&self) -> bool {
        self.state == DecisionState::Open && !self.withdrawn() && !self.superseded()
    }

    /// Whether the work is built on it: accepted, and neither replaced nor
    /// taken back.
    pub fn stands(&self) -> bool {
        self.state == DecisionState::Accepted && !self.withdrawn() && !self.superseded()
    }
}

/// A change to one decision already on the board. A decision is otherwise
/// append-only; this is how one recorded in error is taken back, and how
/// that is undone in turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionChange {
    Withdraw { id: i64 },
    Restore { id: i64 },
    /// Answers an open decision. `answer` becomes its `detail`, the options
    /// not taken become what it was chosen over, and it is accepted.
    Resolve {
        id: i64,
        answer: String,
        because: Option<String>,
    },
    /// Replaces a decision's prose, keeping its place, state and history.
    /// A person correcting what a decision says; one that turned out wrong
    /// is still superseded.
    Rewrite { id: i64, text: DecisionText },
}

/// Everything a decision says, apart from where it hangs and how it stands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionText {
    pub chose: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub because: Option<String>,
    #[serde(default)]
    pub over: Option<String>,
    #[serde(default)]
    pub consequences: Vec<Consequence>,
    #[serde(default)]
    pub options: Vec<String>,
}

impl DecisionText {
    /// The same trimming and bounds a new decision gets, since a rewrite
    /// must not be the way past them.
    pub fn checked(self) -> Result<DecisionText, &'static str> {
        let tidy = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let text = DecisionText {
            chose: self.chose.trim().to_string(),
            context: tidy(self.context),
            detail: tidy(self.detail),
            because: tidy(self.because),
            over: tidy(self.over),
            consequences: self
                .consequences
                .into_iter()
                .filter_map(|c| {
                    let text = c.text.trim().to_string();
                    (!text.is_empty()).then_some(Consequence { text, ..c })
                })
                .collect(),
            options: self
                .options
                .into_iter()
                .map(|o| o.trim().to_string())
                .filter(|o| !o.is_empty())
                .collect(),
        };
        if text.chose.is_empty() {
            return Err("a decision has to say what was chosen");
        }
        let longest = [
            Some(&text.chose),
            text.context.as_ref(),
            text.detail.as_ref(),
            text.because.as_ref(),
            text.over.as_ref(),
        ]
        .into_iter()
        .flatten()
        .chain(text.options.iter())
        .map(String::len)
        .chain(text.consequences.iter().map(|c| c.text.len()))
        .max()
        .unwrap_or(0);
        if longest > MAX_DECISION_BYTES {
            return Err("a decision is a sentence or two per field, not a document");
        }
        if text.options.len() > MAX_DECISION_OPTIONS {
            return Err("a decision with that many options is not a question yet");
        }
        if text.consequences.len() > MAX_DECISION_CONSEQUENCES {
            return Err("a decision lists the consequences that matter, not all of them");
        }
        Ok(text)
    }
}

impl From<&Decision> for DecisionText {
    fn from(d: &Decision) -> DecisionText {
        DecisionText {
            chose: d.chose.clone(),
            context: d.context.clone(),
            detail: d.detail.clone(),
            because: d.because.clone(),
            over: d.over.clone(),
            consequences: d.consequences.clone(),
            options: d.options.clone(),
        }
    }
}

/// A decision as it is asked for, before the store gives it an identity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionWrite {
    pub chose: String,
    pub over: Option<String>,
    pub because: Option<String>,
    /// Hangs this decision under an existing one.
    pub under: Option<i64>,
    /// Replaces an existing decision. The new node takes the old one's
    /// place in the tree — its parent, not its children — and the old one
    /// is marked rather than removed.
    pub supersedes: Option<i64>,
    /// Accepted unless said otherwise: `Open` asks a question, `Rejected`
    /// records an answer that was turned down.
    #[serde(default)]
    pub state: DecisionState,
    /// The task it is raised under, which must be on the same feature.
    #[serde(default)]
    pub task: Option<i64>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub consequences: Vec<Consequence>,
    /// Only an open decision has options; a made one has what it was
    /// chosen over.
    #[serde(default)]
    pub options: Vec<String>,
}

impl DecisionWrite {
    /// Trims every field, drops the ones left empty, and refuses a write
    /// with nothing chosen or a field long enough to be a document.
    ///
    /// Validation lives here rather than in the daemon because both sides
    /// of the pane API have to agree on what a decision is: the helper
    /// tells the agent what was wrong with it, and the daemon is what
    /// cannot be talked past.
    pub fn checked(self) -> Result<DecisionWrite, &'static str> {
        let text = DecisionText {
            chose: self.chose,
            context: self.context,
            detail: self.detail,
            because: self.because,
            over: self.over,
            consequences: self.consequences,
            options: self.options,
        }
        .checked()?;
        if self.under.is_some() && self.supersedes.is_some() {
            return Err("a decision either hangs under one or replaces one, not both");
        }
        if self.task.is_some_and(|id| id <= 0) {
            return Err("a task id must be positive");
        }
        if self.state != DecisionState::Open && !text.options.is_empty() {
            return Err("only an open decision has options; say what a made one was chosen over");
        }
        if self.state == DecisionState::Open && self.supersedes.is_some() {
            return Err("a question cannot replace a decision; supersede it with the answer");
        }
        Ok(DecisionWrite {
            chose: text.chose,
            over: text.over,
            because: text.because,
            under: self.under,
            supersedes: self.supersedes,
            state: self.state,
            task: self.task,
            context: text.context,
            detail: text.detail,
            consequences: text.consequences,
            options: text.options,
        })
    }
}

/// One project's whole board, which is how it is read: a decision means
/// nothing without the ones above it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionBoard {
    pub project: Option<ProjectId>,
    pub name: String,
    /// The artifact scope's features, oldest first. A board is read one feature
    /// at a time, so the client needs the list to offer the choice.
    #[serde(default)]
    pub features: Vec<Feature>,
    /// Oldest first, so a parent is always already known by the time its
    /// children are read.
    pub decisions: Vec<Decision>,
    /// How the artifact scope's work maps to the team's trackers.
    #[serde(default)]
    pub tracker_map: TrackerMap,
    /// The repositories in the tree whose board this is, so a client keeps
    /// only its own: every board is pushed to every client, and the
    /// repositories of a project share its name. Empty from a daemon that
    /// does not say, which leaves the name to go by.
    #[serde(default)]
    pub repositories: Vec<RepositoryId>,
}

/// Whether a board, a task list or a diagram list pushed to every client is
/// the one of `repository`, in the project named `project_name`. Said once
/// so every client keeps the same pushes: before boards named their
/// repositories, a write to one repository's board replaced the board on
/// screen for another repository of the same project.
pub fn is_for(
    pushed_project: &str,
    pushed_repositories: &[RepositoryId],
    project_name: &str,
    repository: RepositoryId,
) -> bool {
    pushed_project == project_name
        && (pushed_repositories.is_empty() || pushed_repositories.contains(&repository))
}

impl DecisionBoard {
    /// Whether this is the board of `repository` in the project named
    /// `project_name`.
    pub fn is_for(&self, project_name: &str, repository: RepositoryId) -> bool {
        is_for(&self.name, &self.repositories, project_name, repository)
    }
}

/// One row of a decision tree, including the topology a renderer needs to
/// draw the branches that lead to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionTreeRow<'a> {
    pub depth: usize,
    pub decision: &'a Decision,
    /// Whether each non-root ancestor has a sibling below this row.
    pub ancestor_continuations: Vec<bool>,
    pub has_next_sibling: bool,
    pub has_children: bool,
    /// Its one-based place among its siblings at each depth, which is what
    /// a reader calls it: `[2, 1]` is `D2.1`.
    pub number: Vec<usize>,
}

impl DecisionTreeRow<'_> {
    /// What a reader calls this decision. Positional, so it is for reading
    /// a board and never for writing to one: `id` is what a write names.
    pub fn label(&self) -> String {
        format!("D{}", outline_number(&self.number))
    }
}

/// A place in an outline as it is said aloud: `2.1`.
pub fn outline_number(number: &[usize]) -> String {
    number
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

impl DecisionBoard {
    /// The same board holding only one feature's decisions — or, for
    /// `None`, only the unfiled ones.
    ///
    /// Filtering here rather than in the daemon because the client is
    /// pushed one board per project and switches scope without a round
    /// trip, and because a parent outside the scope is left to
    /// [`DecisionBoard::tree_rows`], which already draws the child of a
    /// parent it cannot see as a root.
    pub fn scoped(&self, feature: Option<&str>) -> DecisionBoard {
        DecisionBoard {
            project: self.project,
            name: self.name.clone(),
            features: self.features.clone(),
            decisions: self
                .decisions
                .iter()
                .filter(|d| d.feature.as_deref() == feature)
                .cloned()
                .collect(),
            tracker_map: self.tracker_map.clone(),
            repositories: self.repositories.clone(),
        }
    }

    /// How many decisions are filed under one feature, or under none.
    pub fn count_for(&self, feature: Option<&str>) -> usize {
        self.decisions
            .iter()
            .filter(|d| d.feature.as_deref() == feature)
            .count()
    }

    /// One feature's decisions that stand, and its questions still open.
    pub fn standing_for(&self, feature: Option<&str>) -> (usize, usize) {
        let filed = self
            .decisions
            .iter()
            .filter(|d| d.feature.as_deref() == feature);
        filed.fold((0, 0), |(decided, open), d| {
            (
                decided + usize::from(d.stands()),
                open + usize::from(d.open()),
            )
        })
    }

    /// The board flattened depth-first with each decision's depth, which
    /// is the order it is drawn in.
    ///
    /// Orphans — a decision whose parent is not on this board — are
    /// treated as roots rather than dropped. A board that silently loses
    /// records is worse than one that draws a node in the wrong place.
    pub fn tree(&self) -> Vec<(usize, &Decision)> {
        self.tree_rows()
            .into_iter()
            .map(|row| (row.depth, row.decision))
            .collect()
    }

    /// The board flattened depth-first with enough sibling information to
    /// draw branch guides between its rows.
    pub fn tree_rows(&self) -> Vec<DecisionTreeRow<'_>> {
        let known: std::collections::HashSet<i64> = self.decisions.iter().map(|d| d.id).collect();
        let mut children: std::collections::HashMap<Option<i64>, Vec<&Decision>> =
            std::collections::HashMap::new();
        for d in &self.decisions {
            let parent = d.parent.filter(|p| known.contains(p));
            children.entry(parent).or_default().push(d);
        }
        let mut out = Vec::new();
        walk(&children, None, &mut Vec::new(), &mut Vec::new(), &mut out);
        out
    }
}

fn walk<'a>(
    children: &std::collections::HashMap<Option<i64>, Vec<&'a Decision>>,
    parent: Option<i64>,
    number: &mut Vec<usize>,
    ancestor_continuations: &mut Vec<bool>,
    out: &mut Vec<DecisionTreeRow<'a>>,
) {
    let Some(here) = children.get(&parent) else {
        return;
    };
    let depth = number.len();
    for (index, decision) in here.iter().enumerate() {
        let has_next_sibling = index + 1 < here.len();
        number.push(index + 1);
        out.push(DecisionTreeRow {
            depth,
            decision,
            ancestor_continuations: ancestor_continuations.clone(),
            has_next_sibling,
            has_children: children.contains_key(&Some(decision.id)),
            number: number.clone(),
        });
        if depth > 0 {
            ancestor_continuations.push(has_next_sibling);
        }
        walk(
            children,
            Some(decision.id),
            number,
            ancestor_continuations,
            out,
        );
        if depth > 0 {
            ancestor_continuations.pop();
        }
        number.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_decision_says_how_it_stands_with_its_first_character() {
        assert_eq!(DecisionState::typed("? which route"), (DecisionState::Open, "which route"));
        assert_eq!(DecisionState::typed("~ read the file"), (DecisionState::Rejected, "read the file"));
        assert_eq!(DecisionState::typed("! one contract"), (DecisionState::Accepted, "one contract"));
        assert_eq!(DecisionState::typed(" one contract "), (DecisionState::Accepted, "one contract"));
    }

    #[test]
    fn a_push_is_for_the_repository_it_names_and_an_unnamed_one_goes_by_project() {
        let named = DecisionBoard {
            name: "src".into(),
            repositories: vec![RepositoryId(2)],
            ..DecisionBoard::default()
        };
        assert!(named.is_for("src", RepositoryId(2)));
        assert!(!named.is_for("src", RepositoryId(3)), "another repository of the project");
        assert!(!named.is_for("work", RepositoryId(2)), "another project");

        let from_an_older_daemon = DecisionBoard { name: "src".into(), ..DecisionBoard::default() };
        assert!(from_an_older_daemon.is_for("src", RepositoryId(3)));
        assert!(!from_an_older_daemon.is_for("work", RepositoryId(3)));
    }

    fn decision(id: i64, parent: Option<i64>, chose: &str) -> Decision {
        Decision {
            id,
            parent,
            at: 0,
            session: None,
            checkout: None,
            feature: None,
            chose: chose.to_string(),
            over: None,
            because: None,
            superseded_by: None,
            withdrawn_at: None,
            ..Decision::default()
        }
    }

    fn board(decisions: Vec<Decision>) -> DecisionBoard {
        DecisionBoard {
            project: None,
            name: "argus".into(),
            features: Vec::new(),
            decisions,
            ..DecisionBoard::default()
        }
    }

    #[test]
    fn a_decision_is_drawn_under_the_one_that_constrained_it() {
        let board = board(vec![
            decision(1, None, "sqlite"),
            decision(2, Some(1), "wal mode"),
            decision(3, Some(2), "one connection"),
            decision(4, None, "ratatui"),
        ]);
        assert_eq!(
            board
                .tree()
                .iter()
                .map(|(depth, d)| (*depth, d.chose.as_str()))
                .collect::<Vec<_>>(),
            [
                (0, "sqlite"),
                (1, "wal mode"),
                (2, "one connection"),
                (0, "ratatui"),
            ]
        );
    }

    #[test]
    fn a_decision_whose_parent_is_not_here_is_still_drawn() {
        let board = board(vec![decision(2, Some(99), "wal mode")]);
        assert_eq!(board.tree().len(), 1, "an orphan is a root, not a loss");
        assert_eq!(board.tree()[0].0, 0);
    }

    #[test]
    fn tree_rows_carry_the_branch_lines_a_renderer_must_continue() {
        let board = board(vec![
            decision(1, None, "root"),
            decision(2, Some(1), "first child"),
            decision(3, Some(2), "grandchild"),
            decision(4, Some(1), "last child"),
        ]);

        let rows = board.tree_rows();
        assert!(rows[0].has_children);
        assert!(rows[1].has_next_sibling);
        assert_eq!(rows[2].ancestor_continuations, [true]);
        assert!(!rows[3].has_next_sibling);
    }

    #[test]
    fn a_board_is_read_one_feature_at_a_time() {
        let mut decisions = vec![
            decision(1, None, "sqlite"),
            decision(2, Some(1), "wal mode"),
            decision(3, None, "one reader thread"),
        ];
        decisions[0].feature = Some("notes".into());
        decisions[1].feature = Some("notes".into());
        decisions[2].feature = Some("pty".into());
        let board = board(decisions);

        assert_eq!(
            board
                .scoped(Some("notes"))
                .tree()
                .iter()
                .map(|(depth, d)| (*depth, d.chose.as_str()))
                .collect::<Vec<_>>(),
            [(0, "sqlite"), (1, "wal mode")]
        );
        assert_eq!(board.count_for(Some("pty")), 1);
        assert_eq!(
            board.count_for(None),
            0,
            "a decision filed under a feature is not also unfiled"
        );
    }

    #[test]
    fn a_child_whose_parent_is_under_another_feature_is_still_drawn() {
        let mut decisions = vec![
            decision(1, None, "sqlite"),
            decision(2, Some(1), "wal mode"),
        ];
        decisions[0].feature = Some("notes".into());
        decisions[1].feature = Some("pty".into());
        let scoped = board(decisions).scoped(Some("pty"));
        assert_eq!(scoped.tree().len(), 1, "an orphan is a root, not a loss");
        assert_eq!(scoped.tree()[0].0, 0);
    }

    #[test]
    fn a_decision_has_to_say_what_was_chosen() {
        let write = DecisionWrite {
            chose: "   ".into(),
            ..Default::default()
        };
        assert!(write.checked().is_err());
    }

    #[test]
    fn an_empty_field_is_dropped_rather_than_recorded_blank() {
        let write = DecisionWrite {
            chose: "  sqlite  ".into(),
            over: Some("   ".into()),
            because: Some(" the schema needs migrations ".into()),
            ..Default::default()
        }
        .checked()
        .unwrap();
        assert_eq!(write.chose, "sqlite");
        assert_eq!(write.over, None);
        assert_eq!(
            write.because.as_deref(),
            Some("the schema needs migrations")
        );
    }

    #[test]
    fn a_decision_either_descends_from_one_or_replaces_one() {
        let write = DecisionWrite {
            chose: "postgres".into(),
            under: Some(1),
            supersedes: Some(2),
            ..Default::default()
        };
        assert!(write.checked().is_err());
    }

    #[test]
    fn a_field_long_enough_to_be_a_document_is_refused() {
        let write = DecisionWrite {
            chose: "x".repeat(MAX_DECISION_BYTES + 1),
            ..Default::default()
        };
        assert!(write.checked().is_err());
    }
}
