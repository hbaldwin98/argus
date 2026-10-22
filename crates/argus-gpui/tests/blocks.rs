//! Photographs the block renderer: the agent view and the saved view the page's own screens show
//! (core's `agent-views` and `saved-view` scenes), and a gallery of every simple block, so they can
//! be set beside the page's `agent-views.png`, `agent-view-page.png` and `saved-view.png`. The
//! surface that hosts a view in a panel is a later item; this draws the blocks in a window of their
//! own, parked off-screen and never shown. Run it by name, it writes nothing otherwise:
//!
//!   ARGUS_GPUI_SHOTS=<dir> cargo test -p argus-gpui --test blocks -- --ignored --nocapture
//!
//! `ARGUS_GPUI_SCENES=a,b` limits the shots.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use argus_desktop_core::connection::page::{Event, FlowLine, PageDiagram, PageParticipant};
use argus_desktop_core::screens::{scenes, NOW};
use argus_desktop_core::views::saved::SavedViewsPage;
use argus_gpui::connection::caller::Caller;
use argus_gpui::style::Typed as _;
use argus_gpui::theme::scale::rem;
use argus_gpui::theme::Tokens;
use argus_gpui::views::blocks::view::Blocks;
use argus_gpui::views::ctx::ViewCtx;
use argus_gpui::views::memory::Memory;
use argus_gpui::window;
use argus_protocol::Block;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{div, point, prelude::*, px, size, App, Bounds, Entity, WindowBounds, WindowOptions};
use serde_json::json;

struct Shot {
    name: &'static str,
    width: f32,
    height: f32,
    blocks: Vec<Block>,
    ctx: ViewCtx,
    saved: Option<Rc<SavedViewsPage>>,
    /// Elements pressed once it is drawn, each after the pointer is put over its drawing: the corner
    /// button that opens a figure larger, then the larger view's own buttons.
    presses: &'static [&'static str],
}

/// The panel the blocks are drawn in: the surface colour with the page's 16px around them.
struct Frame {
    blocks: Entity<Blocks>,
    width: f32,
}

impl Render for Frame {
    fn render(&mut self, _: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        div().size_full().bg(tokens.bg_surface).text_color(tokens.fg).typed(tokens.type_body).p(rem(tokens.sp_16)).child(div().w(px(self.width - 32.0)).child(self.blocks.clone()))
    }
}

/// What a fresh page would hear in the scene `name`.
fn events_of(name: &str) -> Vec<Event> {
    let scene = scenes().into_iter().find(|scene| scene.name == name).unwrap_or_else(|| panic!("no scene {name}"));
    (scene.build)().everything(NOW)
}

fn agent_view(title: &str) -> (Vec<Block>, ViewCtx) {
    let page = events_of("agent-views")
        .into_iter()
        .find_map(|event| if let Event::Views(page) = event { Some(page) } else { None })
        .expect("the scene shows views");
    let shown = page.views.iter().find(|view| view.view.title.starts_with(title)).unwrap_or_else(|| panic!("no view {title}"));
    let ctx = ViewCtx { diagrams: Rc::new(shown.diagrams.clone()), can_press: true, ..ViewCtx::agent(shown.pane, &shown.view.id) };
    (shown.view.blocks.clone(), ctx)
}

fn saved_page() -> Rc<SavedViewsPage> {
    let page = events_of("saved-view").into_iter().find_map(|event| if let Event::SavedViews(page) = event { Some(page) } else { None }).expect("the scene saves views");
    Rc::new(*page)
}

fn gallery() -> Vec<Block> {
    serde_json::from_value(json!([
        {"kind": "section", "heading": "Callouts", "blocks": [
            {"kind": "callout", "tone": "warn", "text": "Decision #3 is still open: how much history should a brief carry?"},
            {"kind": "callout", "tone": "bad", "text": "Two checks failed.\nThe second line stays a second line."},
            {"kind": "callout", "tone": "good", "text": "Every check passed."},
            {"kind": "callout", "tone": "info", "text": "Built from main at 6ce099a."}
        ]},
        {"kind": "fields", "fields": [
            {"label": "Status", "value": {"kind": "chip", "chip": {"label": "in review", "tone": "active"}}},
            {"label": "Owner", "value": {"kind": "person", "name": "Ada Lovelace"}},
            {"label": "Progress", "value": {"kind": "progress", "value": 3, "max": 4, "tone": "good"}}
        ]},
        {"kind": "card", "title": "Deploy 0.34", "tone": "warn", "chip": {"label": "needs you", "tone": "warn"}, "blocks": [
            {"kind": "markdown", "text": "Ready to cut. **Two** blockers are still open."},
            {"kind": "row", "align": "spread", "items": [
                {"kind": "value", "value": {"kind": "ticket", "key": "ARG-142", "title": "Rank recipes", "status": {"label": "open", "tone": "info"}}},
                {"kind": "action", "action": {"id": "ship", "label": "Ship", "tone": "good", "confirm": "Ship 0.34?"}}
            ]}
        ], "actions": [
            {"id": "a", "label": "Good", "tone": "good"}, {"id": "b", "label": "Bad", "tone": "bad"}, {"id": "c", "label": "Warn", "tone": "warn"},
            {"id": "d", "label": "Info", "tone": "info"}, {"id": "e", "label": "Plain"}
        ]},
        {"kind": "grid", "min": "sm", "items": [
            {"kind": "card", "title": "One", "blocks": [{"kind": "markdown", "text": "first"}]},
            {"kind": "card", "title": "Two", "blocks": [{"kind": "markdown", "text": "second"}]},
            {"kind": "card", "title": "Three", "blocks": [{"kind": "markdown", "text": "third"}]}
        ]},
        {"kind": "details", "summary": "More detail", "open": true, "chip": {"label": "3", "tone": "neutral"}, "blocks": [
            {"kind": "code", "lang": "rust", "text": "fn main() {\n    println!(\"hello\");\n}"}
        ]},
        {"kind": "tabs", "tabs": [
            {"title": "Comments", "blocks": [{"kind": "list", "items": [
                {"who": "Ada Lovelace", "at": "2026-10-19T13:52:00Z", "title": "Looks right", "body": "Ship it, with **one** note."},
                {"title": "Merged by CI", "at": "2026-10-19T14:20:00Z"}
            ]}]},
            {"title": "Other", "blocks": [{"kind": "markdown", "text": "the other tab"}]}
        ]},
        {"kind": "stats", "stats": [
            {"label": "Caught", "value": "41", "tone": "good", "delta": "+3"},
            {"label": "Survived", "value": "3", "tone": "bad"},
            {"label": "Unviable", "value": "6", "delta": "same"}
        ]},
        {"kind": "progress", "label": "Scenes photographed", "value": 34, "max": 36, "tone": "active", "note": "float and docks left"},
        {"kind": "timeline", "events": [
            {"at": "Oct 12", "title": "0.33.5 released", "tone": "good"},
            {"at": "Oct 19", "title": "0.33.6 released", "tone": "good", "body": "Stops capping an agent's memory"},
            {"at": "Oct 22", "title": "Trim context lands", "tone": "active"}
        ]},
        {"kind": "timeline", "events": [
            {"at": "Oct 12", "title": "Build started", "lane": "ci", "tone": "info"},
            {"at": "Oct 12", "title": "Reviewed", "lane": "people", "tone": "good"},
            {"at": "Oct 13", "title": "Merged", "tone": "active"}
        ]},
        {"kind": "tree", "nodes": [
            {"label": "crates", "open": true, "children": [
                {"label": "argus-gpui", "open": true, "value": {"kind": "number", "number": 1204}, "children": [{"label": "views", "chip": {"label": "new", "tone": "info"}}]},
                {"label": "argusd", "children": [{"label": "store"}]}
            ]}
        ]},
        {"kind": "checklist", "items": [
            {"label": "Add a route", "done": true},
            {"label": "Wire it up", "done": false, "note": "waits on decision #3"}
        ]},
        {"kind": "log", "lines": [
            {"level": "debug", "text": "reading the board"},
            {"level": "info", "at": "2026-10-19T14:20:00Z", "text": "41 caught, 6 unviable in 3m 12s"},
            {"level": "warn", "text": "one run was slow"},
            {"level": "error", "text": "MISSED faults.rs:88: replace FaultPlan::due -> bool with true"}
        ]},
        {"kind": "board", "columns": [
            {"title": "Todo", "chip": {"label": "2", "tone": "neutral"}, "cards": [
                {"title": "Rank recipes", "body": "Sort by **cost**", "chip": {"label": "#41", "tone": "active"}},
                {"title": "Cache the brief", "value": {"kind": "number", "number": 3, "unit": "days"}}
            ]},
            {"title": "Doing", "cards": [{"title": "Trim context"}]},
            {"title": "Done", "cards": []},
            {"title": "Later", "cards": [{"title": "Wider than the view"}]}
        ]},
        {"kind": "matrix", "rows": ["Windows", "Linux"], "columns": ["build", "test"], "cells": [
            [{"tone": "good", "weight": 1}, {"tone": "warn", "weight": 0.6, "label": "1 flaky"}],
            [{"tone": "good", "weight": 0.3}, {"tone": "neutral", "weight": 0.2, "label": "n/a"}]
        ]},
        {"kind": "columns", "columns": [
            [{"kind": "callout", "tone": "info", "text": "left column"}],
            [{"kind": "callout", "tone": "good", "text": "right column"}]
        ]},
        {"kind": "divider"},
        {"kind": "embed", "view": "release-0-34"},
        {"kind": "embed", "view": "not-saved-yet"},
        {"kind": "table", "columns": [], "rows": []},
        {"kind": "chart", "x": [], "series": []},
        {"kind": "graph", "nodes": [], "edges": []}
    ]))
    .expect("the gallery is blocks")
}

/// The blocks that are drawn rather than laid out: a table with every part of its bar, charts of each
/// shape, a sequence diagram and a graph in both directions.
fn drawn() -> Vec<Vec<Block>> {
    let owners = ["Ada Lovelace", "Grace Hopper", "Linus T"];
    let runs: Vec<serde_json::Value> = (1..=12)
        .map(|n| {
            let (tone, word) = if n % 5 == 0 { ("bad", "failed") } else { ("good", "passed") };
            json!([
                {"kind": "text", "text": format!("run {n}")},
                {"kind": "number", "number": 90 * n * n, "unit": "ms"},
                {"kind": "chip", "chip": {"label": word, "tone": tone}},
                {"kind": "person", "name": owners[n % 3]},
                {"kind": "text", "text": format!("A note on run {n} that is a good deal longer than a cell should be")}
            ])
        })
        .collect();
    let keys: Vec<String> = (1..=12).map(|n| format!("r{n}")).collect();
    let details: Vec<serde_json::Value> = (1..=12).map(|n| if n < 3 { json!([{"kind": "markdown", "text": format!("What happened in **run {n}**.")}]) } else { json!(null) }).collect();
    let table = json!({"kind": "table", "sorted_by": 1,
        "columns": [{"title": "Run"}, {"title": "Took", "numeric": true}, {"title": "Result"}, {"title": "Owner"}, {"title": "Note", "optional": true}],
        "rows": runs, "total": 212, "row_keys": keys, "select": "many", "details": details,
        "actions": [{"id": "rerun", "label": "Re-run", "tone": "good"}, {"id": "drop", "label": "Drop", "tone": "bad", "confirm": "Drop these runs?"}]});
    let words = ["start", "plan", "edit", "test", "review", "commit"];
    let charts = json!([
        {"kind": "chart", "shape": "bar", "unit": "tokens", "x": words, "series": [
            {"name": "injected", "values": [10600, 10400, 11200, 10500, 10300, 10900]},
            {"name": "brief", "values": [3100, 2900, 3200, 3000, 2900, 3100], "tone": "good"}]},
        {"kind": "chart", "shape": "line", "unit": "ms", "x": words, "series": [
            {"name": "p50", "values": [120, 140, null, 90, 100, 80], "tone": "active"},
            {"name": "p95", "values": [300, 340, 280, 260, null, 210], "tone": "active"},
            {"name": "p99", "values": [-40, 20, 60, 120, 90, 70], "tone": "active"}]},
        {"kind": "chart", "shape": "area", "x": words, "series": [{"name": "queue", "values": [2, 5, 3, 8, 6, 9], "tone": "warn"}]},
        {"kind": "chart", "shape": "spark", "x": [], "series": [{"name": "trend", "values": [3, 4, 3, 6, 8, 7, 9], "tone": "good"}]},
        {"kind": "stats", "stats": [{"label": "Prompt context", "value": "3.1k", "tone": "good", "delta": "-71%", "trend": [9, 9, 8, 6, 4, 3]}, {"label": "Brief", "value": "148", "trend": [1, 2, 2, 3, 5]}]}
    ]);
    let graph = json!([
        {"kind": "graph", "direction": "down", "nodes": [
            {"id": "edit", "label": "edit", "tone": "active"}, {"id": "test", "label": "run the tests"}, {"id": "fix", "label": "fix it", "tone": "bad"},
            {"id": "ship", "label": "ship", "tone": "good", "opens": "release-0-34"}, {"id": "loop", "label": "retry", "tone": "warn"}],
         "edges": [{"from": "edit", "to": "test"}, {"from": "test", "to": "fix", "label": "red"}, {"from": "fix", "to": "test", "label": "re-run"},
            {"from": "test", "to": "ship", "label": "green"}, {"from": "edit", "to": "ship"}, {"from": "loop", "to": "loop", "label": "again"}, {"from": "ship", "to": "ghost"}]},
        {"kind": "graph", "direction": "right", "nodes": [{"id": "a", "label": "hook"}, {"id": "b", "label": "argusd"}, {"id": "c", "label": "a very long label that is clipped at twenty-eight characters"}],
         "edges": [{"from": "a", "to": "b", "label": "view show"}, {"from": "b", "to": "c"}]}
    ]);
    let diagram = json!([{"kind": "diagram", "source": DIAGRAM}, {"kind": "diagram", "source": "not a sequence"}]);
    // An agent's diff, and a form with a field of every kind.
    let patch_form = json!([
        {"kind": "diff", "path": "crates/argus-gpui/src/views/blocks/form.rs", "patch": PATCH},
        {"kind": "form", "id": "deploy", "submit": {"id": "go", "label": "Deploy", "tone": "good"}, "fields": [
            {"name": "branch", "label": "Branch", "input": {"kind": "text", "default": "main", "placeholder": "a branch"}},
            {"name": "count", "label": "Replicas", "input": {"kind": "number", "default": 3, "min": 1, "max": 9}},
            {"name": "env", "label": "Environment", "input": {"kind": "choice", "options": ["dev", "staging", "prod"], "default": "staging"}},
            {"name": "when", "label": "Not before", "input": {"kind": "date"}},
            {"name": "loud", "label": "Tell the channel", "input": {"kind": "toggle", "default": true}},
            {"name": "note", "label": "Note", "input": {"kind": "text", "multiline": true, "placeholder": "Why now?"}}
        ]}
    ]);
    [json!([table]), charts, graph, diagram, patch_form].into_iter().map(|v| serde_json::from_value(v).expect("drawn blocks")).collect()
}

const PATCH: &str = "--- a/src/form.rs
+++ b/src/form.rs
@@ -10,4 +10,5 @@ pub fn form(id: &str) {
     let key = format!(\"{id}-form\");
-    let old = 1;
+    let new = 2;
+    let more = 3;
     keep();
     keep();
@@ -40,3 +41,3 @@ fn tail() {
     one();
-    two();
+    three();
     four();
";

const DIAGRAM: &str = "sequenceDiagram
    actor person as Person
    participant hook as argus-hook
    participant argusd
    person->>hook: argus-hook view show
    hook->>argusd: ViewShow with the whole view
    alt the daemon is up
        argusd->>argusd: check the view
        argusd-->>hook: shown
    else it is not
        hook-->>person: nothing happens
    end";

/// Each diagram block's source as Rust reads it, as the page is told.
fn read(blocks: &[Block]) -> Vec<PageDiagram> {
    use argus_protocol::board::diagrams::flow::{self, FlowRow};
    blocks
        .iter()
        .filter_map(|block| if let Block::Diagram { source } = block { Some(source) } else { None })
        .map(|source| PageDiagram {
            source: source.clone(),
            participants: flow::participants(source).into_iter().map(|p| PageParticipant { id: p.id, label: p.label, actor: p.actor }).collect(),
            rows: flow::rows(source)
                .into_iter()
                .map(|row| match row {
                    FlowRow::Step(s) => FlowLine::Step { number: s.number, from: s.from, to: s.to, reply: s.reply, text: s.text },
                    FlowRow::Branch { kind, when } => FlowLine::Branch { word: kind, when },
                })
                .collect(),
        })
        .collect()
}

fn out_dir() -> PathBuf {
    std::env::var_os("ARGUS_GPUI_SHOTS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/argus-gpui-shots"))
}

fn shots() -> Vec<Shot> {
    let (trim, trim_ctx) = agent_view("Trim context");
    let saved = saved_page();
    let release = saved.views.iter().find(|view| view.id == "release-0-34").expect("the release view");
    let release_ctx = ViewCtx::saved(release, None, vec![release.id.clone()], 0, "col0", true);
    let mut shots = vec![
        Shot { name: "blocks-agent-panel", width: 440.0, height: 1300.0, blocks: trim.clone(), ctx: trim_ctx.clone(), saved: None, presses: &[] },
        Shot { name: "blocks-agent-page", width: 1150.0, height: 900.0, blocks: trim, ctx: trim_ctx, saved: None, presses: &[] },
        Shot { name: "blocks-saved", width: 440.0, height: 900.0, blocks: release.view.blocks.clone(), ctx: release_ctx, saved: Some(saved.clone()), presses: &[] },
    ];
    let all = gallery();
    let ctx = ViewCtx { can_press: true, ..ViewCtx::agent(1, "gallery") };
    for (name, from, to) in [("blocks-gallery-1", 0, 7), ("blocks-gallery-2", 7, 14), ("blocks-gallery-3", 14, all.len())] {
        shots.push(Shot { name, width: 760.0, height: 1300.0, blocks: all[from..to.min(all.len())].to_vec(), ctx: ctx.clone(), saved: Some(saved.clone()), presses: &[] });
    }
    for (k, blocks) in drawn().into_iter().enumerate() {
        let name = ["blocks-table", "blocks-charts", "blocks-graphs", "blocks-diagrams", "blocks-patch-form"][k];
        let ctx = ViewCtx { diagrams: Rc::new(read(&blocks)), can_press: true, ..ViewCtx::agent(1, "drawn") };
        shots.push(Shot { name, width: 760.0, height: 900.0, blocks, ctx, saved: Some(saved.clone()), presses: &[] });
    }
    // A drawing opened larger: the diagram with its source beside it, and the graph.
    let (diagrams, graphs) = (drawn()[3].clone(), drawn()[2].clone());
    let larger = |name: &'static str, blocks: Vec<Block>, presses: &'static [&'static str]| {
        let ctx = ViewCtx { diagrams: Rc::new(read(&blocks)), can_press: true, ..ViewCtx::agent(1, "drawn") };
        Shot { name, width: 1100.0, height: 760.0, blocks, ctx, saved: Some(saved.clone()), presses }
    };
    shots.push(larger("blocks-figure-diagram", diagrams, &["1/drawn.0-diagram-open", "figure-source"]));
    shots.push(larger("blocks-figure-graph", graphs, &["1/drawn.0-graph-open"]));
    shots
}

#[test]
#[ignore = "opens a real window off-screen and writes PNGs; run it by name"]
fn write_view_blocks() {
    let wanted: Option<Vec<String>> = std::env::var("ARGUS_GPUI_SCENES").ok().map(|names| names.split(',').map(str::to_owned).collect());
    let dir = out_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let shots: Vec<Shot> = shots().into_iter().filter(|shot| wanted.as_ref().is_none_or(|names| names.iter().any(|name| name == shot.name))).collect();
    window::application().run(move |cx: &mut App| {
        window::init(cx);
        cx.set_global(argus_gpui::settings::look::Forced(true));
        cx.spawn(async move |cx| {
            for shot in shots {
                let Shot { name, width, height, blocks, ctx, saved, presses } = shot;
                let handle = cx
                    .update(|cx| {
                        let bounds = Bounds { origin: point(px(-10000.0), px(-10000.0)), size: size(px(width), px(height)) };
                        let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None, focus: false, show: false, ..Default::default() };
                        gpui_kit::open_window(options, cx, |_, cx| {
                            let view = cx.new(|cx| {
                                let mut view = Blocks::new(Memory::shared(), Caller::new(|_| {}));
                                view.show(Rc::new(blocks), ctx, saved, cx);
                                view
                            });
                            cx.new(|_| Frame { blocks: view, width })
                        })
                        .map(|(handle, _)| handle)
                    })
                    .expect("opens");
                cx.update(|cx| {
                    let _ = cx.update_window(handle, |_, window, _| window.resize(size(px(width), px(height))));
                });
                for id in presses {
                    // The corner button shows while the pointer is over the drawing.
                    let drawing = id.replace("-open", "-inline");
                    cx.update(|cx| {
                        let _ = cx.update_window(handle, |_, window, cx| {
                            window.render_frame(cx);
                            if window.try_find(drawing.clone()).is_some() {
                                window.hover(drawing, cx);
                            }
                            window.render_frame(cx);
                            window.click(id.to_string(), cx);
                        });
                    });
                    cx.background_executor().timer(Duration::from_millis(400)).await;
                }
                cx.background_executor().timer(Duration::from_millis(500)).await;
                // A stage measured at the window's last size places its drawing a frame later.
                for _ in 0..4 {
                    cx.update(|cx| {
                        let _ = cx.update_window(handle, |_, window, cx| window.render_frame(cx));
                    });
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                }
                let image = cx.update(|cx| {
                    cx.update_window(handle, |_, window, cx| {
                        for _ in 0..3 {
                            window.render_frame(cx);
                        }
                        window.render_to_image()
                    })
                });
                match image {
                    Ok(Ok(image)) => {
                        // The platform may not give the height asked for: keep the frame's own proportions.
                        let tall = (width * 2.0 * image.height() as f32 / image.width() as f32) as u32;
                        let image = image::imageops::resize(&image, (width * 2.0) as u32, tall, image::imageops::FilterType::CatmullRom);
                        let path = dir.join(format!("{name}.png"));
                        image.save(&path).unwrap();
                        println!("{name} -> {}", path.display());
                    }
                    other => panic!("{name} did not render: {other:?}"),
                }
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    });
}
