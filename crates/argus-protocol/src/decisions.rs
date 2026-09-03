//! The decision board: what an agent chose, and what it chose against.
//!
//! A log of decisions answers "what happened". A tree answers the
//! question people actually come back with, which is "why is it like
//! this" — and that answer is mostly the road not taken. So a decision
//! names what it was chosen *over* as well as what was chosen, and hangs
//! off the decision that constrained it.
//!
//! Nothing here is ever edited. A decision that turns out to be wrong is
//! superseded by a new one, which points back at it; the old node stays on
//! the board, drawn dimmed. Editing it away would leave a board that
//! agrees with itself and explains nothing.

use serde::{Deserialize, Serialize};

use crate::features::Feature;
use crate::ids::ProjectId;

/// Past this a field has stopped being a decision and started being a
/// design document, which belongs in the checkout it describes.
pub const MAX_DECISION_BYTES: usize = 1024;

/// One recorded decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl Decision {
    pub fn superseded(&self) -> bool {
        self.superseded_by.is_some()
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
        let tidy = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let write = DecisionWrite {
            chose: self.chose.trim().to_string(),
            over: tidy(self.over),
            because: tidy(self.because),
            ..self
        };
        if write.chose.is_empty() {
            return Err("a decision has to say what was chosen");
        }
        let longest = [
            Some(&write.chose),
            write.over.as_ref(),
            write.because.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(|s| s.len())
        .max()
        .unwrap_or(0);
        if longest > MAX_DECISION_BYTES {
            return Err("a decision is a sentence or two per field, not a document");
        }
        if write.under.is_some() && write.supersedes.is_some() {
            return Err("a decision either hangs under one or replaces one, not both");
        }
        Ok(write)
    }
}

/// One project's whole board, which is how it is read: a decision means
/// nothing without the ones above it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionBoard {
    pub project: Option<ProjectId>,
    pub name: String,
    /// The project's features, oldest first. A board is read one feature
    /// at a time, so the client needs the list to offer the choice.
    #[serde(default)]
    pub features: Vec<Feature>,
    /// Oldest first, so a parent is always already known by the time its
    /// children are read.
    pub decisions: Vec<Decision>,
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
        }
    }

    /// How many decisions are filed under one feature, or under none.
    pub fn count_for(&self, feature: Option<&str>) -> usize {
        self.decisions
            .iter()
            .filter(|d| d.feature.as_deref() == feature)
            .count()
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
        walk(&children, None, 0, &mut Vec::new(), &mut out);
        out
    }
}

fn walk<'a>(
    children: &std::collections::HashMap<Option<i64>, Vec<&'a Decision>>,
    parent: Option<i64>,
    depth: usize,
    ancestor_continuations: &mut Vec<bool>,
    out: &mut Vec<DecisionTreeRow<'a>>,
) {
    let Some(here) = children.get(&parent) else {
        return;
    };
    for (index, decision) in here.iter().enumerate() {
        let has_next_sibling = index + 1 < here.len();
        out.push(DecisionTreeRow {
            depth,
            decision,
            ancestor_continuations: ancestor_continuations.clone(),
            has_next_sibling,
            has_children: children.contains_key(&Some(decision.id)),
        });
        if depth > 0 {
            ancestor_continuations.push(has_next_sibling);
        }
        walk(
            children,
            Some(decision.id),
            depth + 1,
            ancestor_continuations,
            out,
        );
        if depth > 0 {
            ancestor_continuations.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    fn board(decisions: Vec<Decision>) -> DecisionBoard {
        DecisionBoard {
            project: None,
            name: "argus".into(),
            features: Vec::new(),
            decisions,
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
        let mut decisions = vec![decision(1, None, "sqlite"), decision(2, Some(1), "wal mode")];
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
        assert_eq!(write.because.as_deref(), Some("the schema needs migrations"));
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
