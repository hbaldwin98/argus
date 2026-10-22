//! Photographs the shared renderers of an agent's views: every kind of value and a Markdown with
//! each of its constructs, so they can be set beside the page's own (`agent-view-page.png` for
//! the values in a table, the page's `.md` styles for the Markdown).
//!
//! The block renderer that draws whole views is a later item; this is the part of it that exists,
//! in a window of its own, parked off-screen and never shown. Run it by name, it writes nothing
//! otherwise:
//!
//!   ARGUS_GPUI_SHOTS=<dir> cargo test -p argus-gpui --test views -- --ignored --nocapture

use std::path::PathBuf;
use std::time::Duration;

use argus_desktop_core::system::clock::Clock;
use argus_gpui::connection::caller::Caller;
use argus_gpui::style::Typed as _;
use argus_gpui::theme::scale::rem;
use argus_gpui::theme::Tokens;
use argus_gpui::views::blocks::markdown::Markdown;
use argus_gpui::views::ctx::{Draw, ViewCtx};
use argus_gpui::views::values::value;
use argus_gpui::window;
use argus_protocol::{Chip, Tone, Value};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{div, point, prelude::*, px, size, App, Bounds, Entity, SharedString, WindowBounds, WindowOptions};

const WIDTH: f32 = 1100.0;
const HEIGHT: f32 = 820.0;

const MARKDOWN: &str = "# A heading\n\n#### A smaller one\n\nA paragraph with **bold**, _emphasis_, ~~struck~~ text, `inline code`, a [link](https://example.com/docs) and a bare https://example.com/bare address. A second line\nbreaks hard.\n\n- first item\n  - a nested one\n- second item with **marks**\n\n3. third\n4. fourth\n\n> A quote that runs\n> over two lines.\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n---\n\n| Name | Count | Note |\n|------|-------|------|\n| `alpha` | 12 | the **first** |\n| beta | 3 | a [link](https://example.com) |\n\n<script>alert(1)</script> is only text.\n";

fn chip(label: &str, tone: Tone) -> Chip {
    Chip { label: label.into(), tone }
}

fn values() -> Vec<(&'static str, Value)> {
    vec![
        ("text", Value::Text { text: "A plain text value".into() }),
        ("number", Value::Number { number: 1234567.5, unit: Some("tokens".into()) }),
        ("bool", Value::Bool { value: true }),
        ("bool", Value::Bool { value: false }),
        ("date", Value::Date { date: "2026-10-05".into() }),
        ("date_time", Value::DateTime { at: "2026-10-19T13:52:00Z".into() }),
        ("person", Value::Person { name: "Ada Lovelace".into() }),
        ("people", Value::People { names: ["Ada Lovelace", "Grace Hopper", "Alan Turing", "Edsger Dijkstra", "Barbara Liskov"].map(String::from).to_vec() }),
        ("chip", Value::Chip { chip: chip("in progress", Tone::Active) }),
        ("chips", Value::Chips { chips: vec![chip("good", Tone::Good), chip("warn", Tone::Warn), chip("bad", Tone::Bad), chip("info", Tone::Info), chip("neutral", Tone::Neutral)] }),
        ("link", Value::Link { label: "the docs".into(), url: "https://example.com/docs".into() }),
        (
            "ticket",
            Value::Ticket { key: "ARG-142".into(), title: Some("Rank recipes".into()), url: Some("https://jira.example/ARG-142".into()), status: Some(chip("open", Tone::Info)) },
        ),
        ("progress", Value::Progress { value: 3.0, max: 4.0, tone: Tone::Good }),
        ("duration", Value::Duration { ms: 252_000 }),
        ("code", Value::Code { text: "cargo test -p argus".into() }),
        ("path", Value::Path { path: "crates/argusd/src/hook/brief.rs".into(), line: Some(31) }),
        ("path outside", Value::Path { path: "/etc/passwd".into(), line: None }),
        ("commit", Value::Commit { sha: "aa10a2010c567a48999479fed257411c0a8b2b67".into(), title: Some("List saved views".into()) }),
        ("feature", Value::Feature { number: 41 }),
        ("pane", Value::Pane { pane: 9, label: Some("claude \u{b7} trim context".into()) }),
        ("view", Value::View { id: "release".into(), label: Some("Release 0.34".into()) }),
        ("empty", Value::Empty),
    ]
}

struct Gallery {
    markdown: Entity<Markdown>,
    caller: Caller,
}

impl Render for Gallery {
    fn render(&mut self, _: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let clock = Clock::local();
        let view = ViewCtx::agent(7, "gallery");
        let draw = Draw { tokens: &tokens, clock: &clock, caller: &self.caller, saved: &[], view: Some(&view) };
        let rows = values().into_iter().enumerate().map(|(k, (kind, v))| {
            h_flex()
                .gap(rem(tokens.sp_12))
                .items_center()
                .child(div().w(rem(132.0)).flex_shrink_0().text_color(tokens.fg_muted).typed(tokens.type_small).child(SharedString::from(kind)))
                .child(div().min_w_0().child(value(&v, &format!("v{k}"), &draw)))
        });
        h_flex()
            .size_full()
            .items_start()
            .gap(rem(tokens.sp_24))
            .p(rem(tokens.sp_24))
            .bg(tokens.bg_surface)
            .text_color(tokens.fg)
            .typed(tokens.type_body)
            .child(v_flex().w(rem(520.0)).flex_shrink_0().gap(rem(tokens.sp_8)).children(rows))
            .child(div().flex_1().min_w_0().child(self.markdown.clone()))
    }
}

fn out_dir() -> PathBuf {
    std::env::var_os("ARGUS_GPUI_SHOTS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/argus-gpui-shots"))
}

#[test]
#[ignore = "opens a real window off-screen and writes a PNG; run it by name"]
fn write_view_renderers() {
    let dir = out_dir();
    std::fs::create_dir_all(&dir).unwrap();
    window::application().run(move |cx: &mut App| {
        window::init(cx);
        cx.set_global(argus_gpui::settings::look::Forced(true));
        cx.spawn(async move |cx| {
            let handle = cx
                .update(|cx| {
                    let bounds = Bounds { origin: point(px(-10000.0), px(-10000.0)), size: size(px(WIDTH), px(HEIGHT)) };
                    let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None, focus: false, show: false, ..Default::default() };
                    gpui_kit::open_window(options, cx, |_, cx| {
                        let caller = Caller::new(|_| {});
                        let markdown = cx.new(|cx| Markdown::new("md", MARKDOWN, caller.clone(), cx));
                        cx.new(|_| Gallery { markdown, caller })
                    })
                    .map(|(handle, _)| handle)
                })
                .expect("opens");
            cx.update(|cx| {
                let _ = cx.update_window(handle, |_, window, _| window.resize(size(px(WIDTH), px(HEIGHT))));
            });
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let shot = cx.update(|cx| {
                cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    window.render_frame(cx);
                    window.render_to_image()
                })
            });
            match shot {
                Ok(Ok(image)) => {
                    let image = image::imageops::resize(&image, (WIDTH * 2.0) as u32, (HEIGHT * 2.0) as u32, image::imageops::FilterType::CatmullRom);
                    let path = dir.join("views-renderers.png");
                    image.save(&path).unwrap();
                    println!("views-renderers -> {}", path.display());
                }
                other => panic!("did not render: {other:?}"),
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    });
}
