use argus_protocol::{PaneKind, PaneStatus, ReviewBase};

use super::*;
use crate::connection::page::DiffSide;
use crate::fixtures::{checkout, pane};

fn anchor(path: &str, new: Option<u32>, old: Option<u32>) -> ReviewAnchor {
    ReviewAnchor {
        base: ReviewBase::Unstaged,
        commit: None,
        path: path.into(),
        old_path: None,
        old_start: old,
        old_end: old,
        new_start: new,
        new_end: new,
        text: Vec::new(),
    }
}

fn draft(id: u64, anchor: ReviewAnchor) -> ReviewDraft {
    ReviewDraft { id, anchor, body: format!("draft {id}") }
}

fn agents(ids: &[u64]) -> CheckoutInfo {
    let panes = ids.iter().map(|&id| pane(id, PaneKind::Agent, &format!("agent {id}"), PaneStatus::Working)).collect();
    checkout(1, "main", true, panes)
}

fn submitted(msg: Result<ClientMsg, String>) -> PaneId {
    match msg {
        Ok(ClientMsg::Draft { action: DraftAction::Submit { recipient, .. }, .. }) => recipient,
        other => panic!("not a send: {other:?}"),
    }
}

fn row(row: usize, old: Option<u32>, new: Option<u32>) -> DiffRow {
    let side = DiffSide { old, new, marker: " ", kind: "context", parts: Vec::new() };
    DiffRow { row, kind: "line", text: String::new(), hidden: None, step: None, left: Some(side), right: None, cursor: false, picked: false }
}

#[test]
fn a_review_is_sent_to_the_agent_named_else_the_one_lit_else_the_first() {
    let co = agents(&[7, 8, 9]);
    let mut d = Drafts::default();
    d.receive(1, vec![draft(1, anchor("a.rs", Some(1), None))]);
    let send = |to: Option<u64>, lit: Option<u64>| {
        let mut d = Drafts { held: d.held.clone(), ..Drafts::default() };
        submitted(d.gesture(DraftGesture::Submit { summary: String::new(), to }, Some(&co), lit.map(PaneId), 5))
    };
    assert_eq!(send(Some(9), Some(8)), PaneId(9));
    assert_eq!(send(None, Some(8)), PaneId(8));
    assert_eq!(send(None, None), PaneId(7));
    assert_eq!(send(Some(42), None), PaneId(7), "one not running is not sent to");
}

#[test]
fn an_empty_review_or_one_still_going_is_not_sent() {
    let co = agents(&[7]);
    let mut d = Drafts::default();
    let submit = || DraftGesture::Submit { summary: String::new(), to: None };
    assert!(d.gesture(submit(), Some(&co), None, 1).is_err());
    d.receive(1, vec![draft(1, anchor("a.rs", Some(1), None))]);
    d.gesture(submit(), Some(&co), None, 2).unwrap();
    assert!(d.page(Some(&co), None, None, &[]).sending);
    assert!(d.gesture(submit(), Some(&co), None, 3).is_err());
    d.receive(2, Vec::new());
    assert!(!d.page(Some(&co), None, None, &[]).sending, "the answer to the send lets it go");
    assert!(d.gesture(submit(), Some(&agents(&[])), None, 4).is_err(), "nothing left to send");
}

#[test]
fn a_draft_hangs_under_its_last_line_only_on_its_own_file_and_side() {
    let co = agents(&[7]);
    let mut d = Drafts::default();
    d.receive(1, vec![draft(1, anchor("a.rs", Some(3), None)), draft(2, anchor("a.rs", None, Some(5))), draft(3, anchor("b.rs", Some(3), None))]);
    let rows = [row(10, Some(3), Some(3)), row(11, Some(5), None), row(12, Some(6), Some(4))];
    let page = d.page(Some(&co), None, Some(("a.rs", ReviewBase::Unstaged)), &rows);
    assert_eq!(page.drafts.iter().map(|v| v.row).collect::<Vec<_>>(), [Some(10), Some(11), None]);
    assert_eq!(page.drafts[0].at, "a.rs:3");
    let staged = d.page(Some(&co), None, Some(("a.rs", ReviewBase::Staged)), &rows);
    assert!(staged.drafts.iter().all(|v| v.row.is_none()), "written on the other side");
}

#[test]
fn a_draft_needs_words_and_lines() {
    assert!(Drafts::add(Some(anchor("a.rs", Some(1), None)), CheckoutId(1), "  ", 1).is_err());
    assert!(Drafts::add(None, CheckoutId(1), "why", 1).is_err());
    let Ok(ClientMsg::Draft { action: DraftAction::Add { body, .. }, .. }) = Drafts::add(Some(anchor("a.rs", Some(1), None)), CheckoutId(1), " why? ", 1) else {
        panic!("not an add");
    };
    assert_eq!(body, "why?");
}

#[test]
fn a_refused_change_lets_go_and_another_is_not_taken() {
    let co = agents(&[7]);
    let mut d = Drafts::default();
    d.gesture(DraftGesture::Delete { id: 1 }, Some(&co), None, 4).unwrap();
    assert!(!d.refused(5));
    assert!(d.refused(4));
    assert!(!d.refused(4), "taken once");
}
