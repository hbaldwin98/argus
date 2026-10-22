//! The Feature stage's index in the whole window: what it draws of a snapshot, what each press and
//! key sends, and what the session's word changes. Rules cited are `gpui/parity/board.md`'s
//! (`BOARD-n`).
//!
//! The window is the shell over a caller that collects its calls (`keys::root::tests`), and the
//! session's index is sent as an event, so nothing here asks a real session.

use argus_desktop_core::connection::page::{AfterChoice, Agent, Connection, Event, FeatureRow, Features, Line, Move, Peek, Stage, Status};
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::AppContext as _;
use gpui_kit::{ElementId, TestAppContext};

use crate::keys::root::tests::{drain, hear, open, Opened};

mod board;
mod filter;
mod keys;
mod list;
mod menus;
mod states;

/// A row in `group`, with the moves Rust would offer it.
pub(crate) fn row(number: i64, slug: &str, title: &str, group: &'static str) -> FeatureRow {
    let here = |place: &str| group == place;
    let derived = |to: &'static str, label: &str, reason: &str| Move { to, label: label.into(), disabled: Some(if here(to) { "here now".into() } else { reason.into() }) };
    let mut moves = vec![
        derived("queued", "Queued", "follows the work"),
        derived("active", "Active", "when a task starts or an agent arrives"),
        derived("done", "Done", "when its 5 open tasks are done"),
    ];
    moves.push(if here("held") { Move { to: "unhold", label: "Lift the hold".into(), disabled: None } } else { Move { to: "held", label: "Held\u{2026}".into(), disabled: None } });
    moves.push(if here("archived") { Move { to: "reopen", label: "Reopen".into(), disabled: None } } else { Move { to: "archived", label: "Archived".into(), disabled: None } });
    FeatureRow {
        slug: slug.into(),
        number,
        title: title.into(),
        standing: group,
        group,
        tasks_done: 2,
        tasks_total: 7,
        decided: 0,
        open: 0,
        branch: Some(slug.into()),
        agent: None,
        after: Vec::new(),
        held: None,
        peeked: false,
        moves,
        footer: "written down \u{b7} 1d ago".into(),
        next: None,
    }
}

/// An active feature with an agent working on it and a question open, peeked.
pub(crate) fn trim() -> FeatureRow {
    FeatureRow {
        agent: Some(Agent { pane: 1, title: "claude \u{b7} trim context".into(), status: "working", needs_you: false }),
        open: 1,
        peeked: true,
        next: Some(Line { label: "2".into(), text: "Write the tests first".into(), aside: None }),
        ..row(41, "trim", "Trim context injection", "active")
    }
}

pub(crate) fn peek_of(row: &FeatureRow) -> Peek {
    Peek {
        slug: row.slug.clone(),
        number: row.number,
        title: row.title.clone(),
        standing: row.standing,
        branch: row.branch.clone(),
        brief: "A long brief.".into(),
        held: row.held.clone(),
        links: Vec::new(),
        needs_you: vec![Line { label: "D1".into(), text: "Drop it or refuse the start?".into(), aside: None }],
        in_flight: Vec::new(),
        next_up: Some(Line { label: "2".into(), text: "Write the tests first".into(), aside: None }),
        last_decided: None,
        pane: row.agent.as_ref().map(|agent| agent.pane),
        after: vec![AfterChoice { slug: "mouse".into(), number: 42, title: "Mouse selection".into(), waits: false }, AfterChoice { slug: "ssh".into(), number: 45, title: "Remote hosts".into(), waits: true }],
    }
}

/// The list of `argus` with a feature active, one queued, one held, one archived, the fold open.
pub(crate) fn list() -> Features {
    let held = FeatureRow { held: Some("until the bump lands".into()), ..row(43, "bump", "The ratatui bump", "held") };
    let rows = vec![trim(), row(42, "mouse", "Mouse selection", "queued"), held, row(44, "old", "Old work", "archived")];
    let peek = Some(peek_of(&rows[0]));
    Features { repository: "argus".into(), layout: "list", show_folded: true, loaded: true, active: 1, queued: 1, held: 1, archived: 1, folded: 2, rows, peek, ..Features::default() }
}

/// The same as a board with Archived folded away.
pub(crate) fn board() -> Features {
    let mut features = Features { layout: "board", show_folded: false, folded: 1, ..list() };
    features.rows.retain(|row| row.group != "archived");
    features
}

pub(crate) fn connected() -> Event {
    Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None })
}

pub(crate) fn heard(features: Features) -> Event {
    Event::Features(Box::new(features))
}

/// A connected window on the Feature stage that has heard `features`, its calls cleared.
pub(crate) fn on(cx: &mut TestAppContext, features: Features) -> Opened {
    let opened = open(cx);
    let rail = argus_desktop_core::connection::page::Rail { stage: Stage::Feature, ..crate::checkouts::stage::tests::rail() };
    hear(cx, &opened, vec![connected(), Event::Rail(Box::new(rail)), heard(features)]);
    drain(&opened);
    opened
}

pub(crate) fn sent(opened: &Opened) -> Vec<String> {
    drain(opened)
}

pub(crate) fn present(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.try_find(id).is_some()).unwrap()
}

pub(crate) fn label(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> Option<String> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.try_find(id).and_then(|found| found.label().map(str::to_owned))).unwrap()
}

pub(crate) fn selected(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> Option<bool> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.find(id).selected()).unwrap()
}

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

pub(crate) fn click(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, cx| window.click(id, cx)).unwrap();
    settle(cx, opened);
}

pub(crate) fn double_click(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, cx| window.double_click(id, cx)).unwrap();
    settle(cx, opened);
}

pub(crate) fn right_click(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, cx| window.right_click(id, cx)).unwrap();
    settle(cx, opened);
}

/// A feature opened from the index, as the open-feature surface would draw it.
pub(crate) fn open_view() -> argus_desktop_core::connection::page::FeatureView {
    use argus_desktop_core::connection::page::{FeatureView, Lens};
    FeatureView {
        slug: "trim".into(),
        number: 41,
        title: "Trim context injection".into(),
        standing: "active",
        branch: None,
        agent: None,
        brief: String::new(),
        links: Vec::new(),
        views: Vec::new(),
        reconciled: None,
        changes: None,
        reconcile: None,
        tasks_done: 0,
        tasks_total: 0,
        decided: 0,
        open: 0,
        lens: Lens::Work,
        loaded: true,
        rows: Vec::new(),
        task: None,
        decision: None,
        record: None,
        flows: None,
    }
}

/// `list()` with the cursor on `slug`.
pub(crate) fn peeking(mut features: Features, slug: &str) -> Features {
    for row in &mut features.rows {
        row.peeked = row.slug == slug;
    }
    features.peek = features.rows.iter().find(|row| row.slug == slug).map(peek_of);
    features
}
