//! The Views stage: the list of what agents showed, the open view drawn as
//! text, and what is left to the desktop.

use super::*;

use crate::app::View;
use argus_protocol::{AgentView, ServerMsg, ShownView};

fn press(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn shown(pane: u64, json: serde_json::Value) -> ShownView {
    let view: AgentView = serde_json::from_value(json).unwrap();
    ShownView {
        pane: PaneId(pane),
        at: argus_protocol::ago::now() as i64 - 600,
        subject: None,
        view,
    }
}

/// Every block the terminal draws, and one it leaves to the desktop.
fn a_ticket() -> serde_json::Value {
    serde_json::json!({
        "id": "ticket",
        "title": "ENG-88 Login fails on retry",
        "status": { "label": "In review", "tone": "active" },
        "blocks": [
            { "kind": "section", "heading": "Summary", "blocks": [
                { "kind": "markdown", "text": "# Problem\nA second login within a minute is refused.\n- the token is reused\n- the retry does not clear it\n\n> seen on staging\n1. reproduce\n2. fix" },
                { "kind": "callout", "tone": "warn", "text": "Blocks the release." }
            ]},
            { "kind": "fields", "fields": [
                { "label": "Assignee", "value": { "kind": "person", "name": "Hunter" } },
                { "label": "Estimate", "value": { "kind": "number", "number": 3, "unit": "pts" } },
                { "label": "Labels", "value": { "kind": "chips", "chips": [
                    { "label": "bug", "tone": "bad" }, { "label": "auth" } ] } },
                { "label": "Due", "value": { "kind": "date", "date": "2026-11-01" } },
                { "label": "Spent", "value": { "kind": "duration", "ms": 252000 } },
                { "label": "Code", "value": { "kind": "path", "path": "src/login.rs", "line": 42 } }
            ]},
            { "kind": "table", "total": 40,
              "columns": [ { "title": "Run" }, { "title": "Result" }, { "title": "Took", "numeric": true },
                           { "title": "Branch", "optional": true } ],
              "sorted_by": 0,
              "rows": [
                [ { "kind": "text", "text": "#1201" }, { "kind": "chip", "chip": { "label": "pass", "tone": "good" } },
                  { "kind": "duration", "ms": 61000 }, { "kind": "text", "text": "main" } ],
                [ { "kind": "text", "text": "#1200" }, { "kind": "chip", "chip": { "label": "fail", "tone": "bad" } },
                  { "kind": "duration", "ms": 4000 }, { "kind": "text", "text": "feat/retry-the-login" } ]
              ] },
            { "kind": "stats", "stats": [
                { "label": "Coverage", "value": "81%", "delta": "+2", "tone": "good", "trend": [1, 3, 2, 5, 8] },
                { "label": "p95", "value": "412ms", "tone": "warn" } ] },
            { "kind": "progress", "label": "Migration", "value": 412, "max": 1000, "note": "412 / 1,000" },
            { "kind": "checklist", "items": [
                { "label": "Reproduce", "done": true },
                { "label": "Write the fix", "done": false, "note": "waiting on review" } ] },
            { "kind": "list", "items": [
                { "who": "Sam", "at": "2026-10-14T09:30:00Z", "body": "Seen it twice now." } ] },
            { "kind": "code", "lang": "rust", "text": "fn retry() {\n    clear();\n}" },
            { "kind": "log", "lines": [
                { "at": "2026-10-14T09:30:01Z", "level": "error", "text": "token reused" },
                { "level": "info", "text": "retrying" } ] },
            { "kind": "divider" },
            { "kind": "chart", "x": ["a", "b"], "series": [] },
            { "kind": "hologram", "depth": 3 },
            { "kind": "fields", "fields": [
                { "label": "Future", "value": { "kind": "hologram" } } ] }
        ]
    })
}

fn app_with_views() -> App {
    let mut app = app_with_tree();
    app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(
        100,
        serde_json::json!({ "id": "run", "title": "Nightly run", "blocks": [
            { "kind": "callout", "tone": "good", "text": "All green." } ] }),
    ))));
    app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(100, a_ticket()))));
    app.open_view(View::AgentViews);
    app
}

#[test]
fn the_stage_says_when_no_agent_has_shown_a_view() {
    let mut app = app_with_tree();
    press(&mut app, '4');
    let out = lines(&draw_at(&mut app, 110, 24)).join("\n");
    assert!(out.contains("No agent has shown a view yet"), "{out}");
}

#[test]
fn the_rail_stays_beside_the_views() {
    let mut app = app_with_views();
    let out = lines(&draw_at(&mut app, 120, 40));
    assert!(app.layout.rail.outer.width > 0, "the rail is drawn");
    assert!(out.iter().any(|l| l.contains("orion")), "its rows are on screen");
    assert!(out.iter().any(|l| l.contains("ENG-88 Login fails on retry")));
}

#[test]
fn every_block_the_terminal_draws_reads_as_text() {
    let mut app = app_with_views();
    // Tall enough that nothing is scrolled away.
    let out = lines(&draw_at(&mut app, 130, 100)).join("\n");
    for expected in [
        "Summary",
        "Problem",
        "• the token is reused",
        "│ seen on staging",
        "1. reproduce",
        "▌ Blocks the release.",
        "Assignee",
        "Hunter",
        "3 pts",
        "[bug] [auth]",
        "2026-11-01",
        "4m 12s",
        "src/login.rs:42",
        "Run ▾",
        "Result",
        "[pass]",
        "1m 1s",
        "2 of 40 rows",
        "Coverage",
        "81%",
        "+2",
        "Migration",
        "41%",
        "412 / 1,000",
        "[x] Reproduce",
        "[ ] Write the fix",
        "waiting on review",
        "Sam · 2026-10-14 09:30",
        "Seen it twice now.",
        "rust",
        "│     clear();",
        "ERROR token reused",
        "chart — open in the desktop",
        "needs a newer Argus to show",
        "(needs a newer Argus)",
    ] {
        assert!(out.contains(expected), "missing {expected:?}:\n{out}");
    }
}

#[test]
fn a_table_drops_its_optional_column_before_it_squeezes_the_rest() {
    let mut app = app_with_views();
    let wide = lines(&draw_at(&mut app, 130, 100)).join("\n");
    assert!(wide.contains("feat/retry-the-login"));

    let narrow = lines(&draw_at(&mut app, 70, 100)).join("\n");
    assert!(!narrow.contains("feat/retry-the-login"), "{narrow}");
    assert!(narrow.contains("[fail]"), "{narrow}");
}

#[test]
fn a_view_taller_than_the_stage_scrolls() {
    let mut app = app_with_views();
    let top = lines(&draw_at(&mut app, 110, 24)).join("\n");
    assert!(top.contains("Summary"));
    assert!(!top.contains("chart — open in the desktop"));

    press(&mut app, 'G');
    let bottom = lines(&draw_at(&mut app, 110, 24)).join("\n");
    assert!(bottom.contains("chart — open in the desktop"), "{bottom}");
    assert!(!bottom.contains("Problem"));
    assert!(app.agent_views.scroll < usize::MAX / 4, "the frame clamps what was asked");

    press(&mut app, 'g');
    let again = lines(&draw_at(&mut app, 110, 24)).join("\n");
    assert!(again.contains("Summary"));
}

#[test]
fn n_opens_the_other_view_and_starts_it_at_the_top() {
    let mut app = app_with_views();
    press(&mut app, 'G');
    draw_at(&mut app, 110, 24);
    press(&mut app, 'n');
    let out = lines(&draw_at(&mut app, 110, 24)).join("\n");
    assert!(out.contains("All green."), "{out}");
    assert_eq!(app.agent_views.scroll, 0);
}

#[test]
fn a_click_on_a_listed_view_opens_it() {
    let mut app = app_with_views();
    draw_at(&mut app, 110, 24);
    let rows = app.layout.agent_views.inner;
    let click = |x, y| crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    app.on_mouse(click(rows.x + 3, rows.y + 1));
    assert_eq!(app.agent_views.opened().unwrap().view.id, "run");
}

#[test]
fn a_stage_with_no_room_still_draws() {
    let mut app = app_with_views();
    for (w, h) in [(30, 6), (24, 3), (10, 2), (1, 1)] {
        draw_at(&mut app, w, h);
    }
}

#[test]
#[ignore = "prints a frame for eyeballing; asserts nothing"]
fn dump_agent_views() {
    let mut app = app_with_views();
    let w = std::env::var("DUMP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
    let h = std::env::var("DUMP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    for line in lines(&draw_at(&mut app, w, h)) {
        println!("|{line}");
    }
}
