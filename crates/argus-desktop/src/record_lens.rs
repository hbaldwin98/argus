//! The Record lens of an open feature: every decision in the order it was
//! made, as the tree it was made in, and a decision read as its record —
//! here, and wherever the Work lens's cursor is on one.
//!
//! The record's sections are the ones
//! `argus_protocol::board::decisions::adr` writes and reads back, so what a
//! reader sees is what an editor fills in and what Copy as ADR puts on the
//! clipboard.

use argus_protocol::board::outline::Outline;
use argus_protocol::{Decision, DecisionState, DecisionText, DecisionTreeRow};

use crate::page::{DecisionDetail, Line, RecordLens, RecordRow};

/// The decisions with `cursor` on one of them; a cursor on none, or on one
/// gone, lands on the first.
pub fn build(outline: &Outline, cursor: Option<i64>) -> RecordLens {
    let cursor = cursor
        .filter(|id| outline.decision(*id).is_some())
        .or_else(|| outline.decisions.first().map(|row| row.decision.id));
    RecordLens {
        rows: outline
            .decisions
            .iter()
            .map(|row| RecordRow {
                id: row.decision.id,
                label: row.label(),
                depth: row.depth,
                text: row.decision.chose.clone(),
                state: decision_state(row.decision),
                on: Some(row.decision.id) == cursor,
            })
            .collect(),
        decision: cursor.and_then(|id| outline.decision(id)).map(|row| decision_detail(row, outline)),
    }
}

/// How a decision stands as a reader is told: a withdrawn or superseded
/// one says so before its state does.
pub fn decision_state(decision: &Decision) -> &'static str {
    if decision.withdrawn() {
        "withdrawn"
    } else if decision.superseded() {
        "superseded"
    } else {
        decision.state.as_str()
    }
}

pub fn decision_detail(row: &DecisionTreeRow, outline: &Outline) -> DecisionDetail {
    let decision = row.decision;
    DecisionDetail {
        id: decision.id,
        label: row.label(),
        chose: decision.chose.clone(),
        state: decision_state(decision),
        withdrawn: decision.withdrawn(),
        raised_under: outline.raised_under(decision.id).map(|task| format!("task {}", task.label())),
        context: decision.context.clone(),
        detail: decision.detail.clone(),
        because: decision.because.clone(),
        over: decision.over.clone(),
        consequences: decision.consequences.iter().map(|c| format!("{} {}", c.sign(), c.text)).collect(),
        options: decision.options.clone(),
        carried_out_by: outline
            .carried_out_by(decision.id)
            .into_iter()
            .map(|row| Line { label: row.label(), text: row.task.title.clone(), aside: None })
            .collect(),
        document: DecisionText::from(decision).markdown(decision.open(), true).trim_end().to_string(),
        superseded_by: decision.superseded_by.and_then(|id| outline.decision(id)).map(|row| row.label()),
        by: if decision.session.is_some() { "by an agent" } else { "by you" },
        // A rejected answer is kept as a child of what it was weighed
        // against: the road not taken is half of what a record is for.
        considered: outline
            .decisions
            .iter()
            .filter(|child| child.decision.parent == Some(decision.id) && child.decision.state == DecisionState::Rejected)
            .map(|child| Line { label: child.label(), text: child.decision.chose.clone(), aside: child.decision.because.clone() })
            .collect(),
        adr: decision.adr(&row.label()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use std::collections::HashSet;

    fn record(board: &argus_protocol::DecisionBoard, cursor: Option<i64>) -> RecordLens {
        let tasks = fixtures::open_tasks();
        let scoped = board.scoped(Some("trim"));
        let outline = Outline::build(tasks.tree_rows(), scoped.tree_rows(), &HashSet::new());
        build(&outline, cursor)
    }

    /// D1 superseded by D3, which has a rejected answer under it.
    fn superseded() -> argus_protocol::DecisionBoard {
        let mut board = fixtures::open_board();
        let mut replacement = board.decisions[0].clone();
        replacement.id = 3;
        replacement.at = 30;
        replacement.chose = "Name it and link it".into();
        replacement.session = Some("s".into());
        let mut rejected = replacement.clone();
        rejected.id = 4;
        rejected.at = 40;
        rejected.parent = Some(3);
        rejected.state = DecisionState::Rejected;
        rejected.chose = "Print it whole".into();
        rejected.because = Some("too long".into());
        board.decisions[0].superseded_by = Some(3);
        board.decisions.extend([replacement, rejected]);
        board
    }

    #[test]
    fn every_decision_is_listed_in_the_order_made_with_the_first_under_the_cursor() {
        let lens = record(&fixtures::open_board(), None);
        let rows: Vec<_> = lens.rows.iter().map(|r| (r.label.as_str(), r.state, r.on)).collect();
        assert_eq!(rows, [("D1", "accepted", true), ("D2", "open", false)]);
        assert_eq!(lens.decision.unwrap().id, 1);
        assert_eq!(record(&fixtures::open_board(), Some(99)).decision.unwrap().id, 1, "a decision gone");
    }

    #[test]
    fn a_superseded_decision_names_what_replaced_it_and_stays_listed() {
        let board = superseded();
        let lens = record(&board, Some(1));
        let d1 = lens.decision.unwrap();
        assert_eq!((d1.state, d1.superseded_by.as_deref()), ("superseded", Some("D3")));
        assert!(lens.rows.iter().any(|r| r.id == 1), "the road not taken stays");
        assert_eq!(d1.by, "by you");
    }

    #[test]
    fn a_record_lists_the_answers_turned_down_under_it_and_copies_as_an_adr() {
        let board = superseded();
        let d3 = record(&board, Some(3)).decision.unwrap();
        assert_eq!(d3.by, "by an agent");
        assert_eq!(d3.considered.len(), 1);
        assert_eq!((d3.considered[0].text.as_str(), d3.considered[0].aside.as_deref()), ("Print it whole", Some("too long")));
        assert!(d3.adr.starts_with(&format!("# {} Name it and link it\n\nStatus: accepted", d3.label)), "{}", d3.adr);
    }

    #[test]
    fn an_open_decision_offers_its_options_and_its_record() {
        let decision = record(&fixtures::open_board(), Some(2)).decision.unwrap();
        assert_eq!((decision.label.as_str(), decision.state), ("D2", "open"));
        assert_eq!(decision.options.len(), 2);
        assert_eq!(decision.raised_under.as_deref(), Some("task 2"));
        assert!(decision.document.starts_with("How much history?\n\n## Context"));
        assert!(decision.document.contains("## Options\n- Every standing decision"));
        let made = record(&fixtures::open_board(), Some(1)).decision.unwrap();
        assert_eq!(made.consequences, ["- one more request"]);
        assert_eq!(made.carried_out_by.len(), 2, "what is under task 1");
    }

    #[test]
    fn a_withdrawn_decision_says_so() {
        let mut board = fixtures::open_board();
        board.decisions[0].withdrawn_at = Some(5);
        let decision = record(&board, Some(1)).decision.unwrap();
        assert_eq!((decision.state, decision.withdrawn), ("withdrawn", true));
    }
}
