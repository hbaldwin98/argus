//! The recent-sessions dialog in a window, and the fixtures the reader's tests share: a listing of
//! sessions as core sends it, and a transcript read whole.

use argus_desktop_core::agents::sessions_dialog::{SessionPreview, SessionRowView, SessionsPage};
use argus_desktop_core::connection::page::{Connection, Event, ReaderPage, Status, TranscriptLine, TurnJump};
use gpui_kit::TestAppContext;

use crate::keys::root::tests::{hear, Opened};

mod keys;
mod list;
mod head;
mod menu;

pub(crate) fn row(n: usize, title: &str, live: bool, on: bool) -> SessionRowView {
    SessionRowView { row: n, title: title.into(), harness: "claude".into(), checkout: "main".into(), live, turns: Some(3), ago: "1h".into(), removed: false, on }
}

pub(crate) fn preview(offer: &'static str) -> SessionPreview {
    SessionPreview {
        title: "Trim the context".into(),
        harness: "claude".into(),
        checkout: "main".into(),
        session_id: "2b7f1c9e-41d3-4c0a".into(),
        started: Some("2026-10-19T14:02:11Z".into()),
        loading: false,
        notice: None,
        entries: Vec::new(),
        earlier: 0,
        offer,
        command: Some("claude --resume 2b7f1c9e".into()),
        why: None,
        readable: true,
    }
}

pub(crate) fn line(kind: &'static str, text: &str) -> TranscriptLine {
    TranscriptLine { kind, text: text.into(), tool: if kind == "tool" { "Read".into() } else { String::new() }, failed: false, at: Some("2026-10-19T14:02:11Z".into()) }
}

/// One live session and two ended ones, the first live one under the cursor.
pub(crate) fn sessions() -> SessionsPage {
    SessionsPage {
        open: true,
        repository: "argus".into(),
        harnesses: vec!["claude".into(), "codex".into()],
        checkouts: vec!["main".into()],
        rows: vec![row(0, "Trim the context", true, true)],
        ended_folded: true,
        ended_count: 2,
        ended_newest: "20m".into(),
        preview: Some(preview("go")),
        ..SessionsPage::default()
    }
}

/// The page with the reader open on the session under the cursor: two of the person's turns among other lines.
pub(crate) fn reading() -> SessionsPage {
    let mut preview = preview("go");
    preview.entries = vec![line("user", "First ask"), line("agent", "On it"), line("tool", "crates/a.rs"), line("user", "Second ask")];
    let jumps = vec![
        TurnJump { entry: 0, text: "First ask".into(), at: Some("2026-10-19T14:02:11Z".into()), on: false },
        TurnJump { entry: 3, text: "Second ask".into(), at: None, on: false },
    ];
    let reader = ReaderPage { session: preview, repository: "argus".into(), ended: None, turns: Some(2), jumps, find: String::new(), matches: Vec::new(), found: None, target: None, moves: 0 };
    SessionsPage { reader: Some(reader), preview: None, ..sessions() }
}

pub(crate) fn connected() -> Event {
    Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None })
}

/// The session says the dialog is `page`, the daemon being there.
pub(crate) fn hear_page(cx: &mut TestAppContext, opened: &Opened, page: SessionsPage) {
    hear(cx, opened, vec![connected(), Event::Sessions(Box::new(page))]);
}

/// The calls of the dialog so far.
pub(crate) fn said(opened: &Opened) -> Vec<String> {
    crate::keys::root::tests::asked(opened).into_iter().filter(|call| call.contains("Sessions") || call.contains("Transcript")).collect()
}
