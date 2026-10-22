//! Photographs the GPUI window in the scenes the Tauri page is photographed in,
//! so the two can be set side by side.
//!
//! Each scene of `argus-desktop-core`'s `screens` (the same session the page's
//! `write_screens` replays) is brought to its screen, its events are heard by
//! a real window on the real platform, parked off-screen and never shown, and
//! `Window::render_to_image` reads the frame back (the spike's
//! `examples/screenshot.rs` found that this works through DirectX). Run it by
//! name, it writes nothing otherwise:
//!
//!   ARGUS_GPUI_SHOTS=<dir> cargo test -p argus-gpui --test screens -- --ignored --nocapture
//!
//! `ARGUS_GPUI_SCENES=a,b` limits the scenes. A scene the page finishes with a
//! click (`then`) is photographed as heard, before the click.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use argus_desktop_core::connection::page::Event;
use argus_desktop_core::screens::{scenes, NOW};
use argus_gpui::chrome::shell::Shell;
use argus_gpui::connection::caller::Caller;
use argus_gpui::connection::heard::Heard;
use argus_gpui::window;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{point, px, size, AnyWindowHandle, App, AppContext as _, Bounds, WindowBounds, WindowOptions};

/// The page's shots are 1440x900 CSS pixels at a device scale of 2.
const WIDTH: f32 = 1440.0;
const HEIGHT: f32 = 900.0;

const DEFAULT: &str = "workspace-agent,workspace-shell,workspace-split,workspace-console,docks,review,review-comment,history";

fn out_dir() -> PathBuf {
    std::env::var_os("ARGUS_GPUI_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/argus-gpui-shots"))
}

/// Everything a fresh page would hear in `name`'s scene, with the first frames.
fn heard_in(name: &str) -> Vec<Event> {
    let scene = scenes().into_iter().find(|scene| scene.name == name).unwrap_or_else(|| panic!("no scene {name}"));
    let mut session = (scene.build)();
    let mut events = session.everything(NOW);
    events.extend(session.take_frames());
    events
}

#[test]
#[ignore = "opens a real window off-screen and writes PNGs; run it by name"]
fn write_gpui_screens() {
    let wanted = std::env::var("ARGUS_GPUI_SCENES").unwrap_or_else(|_| DEFAULT.to_owned());
    let names: Vec<String> = wanted.split(',').filter(|name| !name.is_empty()).map(str::to_owned).collect();
    let dir = out_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let failures: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = failures.clone();
    window::application().run(move |cx: &mut App| {
        window::init(cx);
        cx.spawn(async move |cx| {
            for name in names {
                let events = heard_in(&name);
                let result = cx.update(|cx| open(cx, events));
                let handle = match result {
                    Ok(handle) => handle,
                    Err(error) => {
                        sink.borrow_mut().push(format!("{name}: {error}"));
                        continue;
                    }
                };
                // The platform does not honour the opening size, so ask again, and give
                // the resize its message before the frame is read.
                cx.update(|cx| {
                    let _ = cx.update_window(handle, |_, window, _| window.resize(size(px(WIDTH), px(HEIGHT))));
                });
                // Lets the observers lay the events out and the fonts settle.
                cx.background_executor().timer(Duration::from_millis(500)).await;
                let shot = cx.update(|cx| {
                    cx.update_window(handle, |_, window, cx| {
                        window.render_frame(cx);
                        window.render_frame(cx);
                        let scale = window.scale_factor();
                        let viewport = window.viewport_size();
                        window.render_to_image().map(|image| (image, scale, viewport))
                    })
                });
                match shot {
                    Ok(Ok((image, scale, viewport))) => {
                        let path = dir.join(format!("{name}.png"));
                        // The page is shot at CSS 1440x900 and a device scale of 2; this
                        // machine's display scale is whatever it is, so bring the frame to
                        // the same pixels and the same logical size.
                        let image = image::imageops::resize(&image, (WIDTH * 2.0) as u32, (HEIGHT * 2.0) as u32, image::imageops::FilterType::CatmullRom);
                        image.save(&path).unwrap();
                        println!("{name}: {}x{} px, scale {scale}, viewport {:?} -> {}", image.width(), image.height(), viewport, path.display());
                    }
                    other => sink.borrow_mut().push(format!("{name}: {other:?}")),
                }
                // Left open: the app quits when its last window closes.
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    });
    assert!(failures.borrow().is_empty(), "scenes that did not render: {:?}", failures.borrow());
}

/// A window parked off-screen at the page's size, hearing `events`.
fn open(cx: &mut App, events: Vec<Event>) -> anyhow::Result<AnyWindowHandle> {
    let heard = cx.new(|_| Heard::new());
    heard.update(cx, |heard, _| {
        for event in events {
            heard.hear(event);
        }
    });
    let bounds = Bounds { origin: point(px(-10000.0), px(-10000.0)), size: size(px(WIDTH), px(HEIGHT)) };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: false,
        show: false,
        ..Default::default()
    };
    // A call goes nowhere: the scene is already where it should be.
    let caller = Caller::new(|_| {});
    let handle = gpui_kit::open_window(options, cx, move |window, cx| cx.new(|cx| Shell::new(heard, caller, window, cx)))?;
    Ok(handle.0)
}
