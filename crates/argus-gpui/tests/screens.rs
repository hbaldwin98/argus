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

use argus_desktop_core::code::edit::EditEvent;
use argus_desktop_core::connection::page::Event;
use argus_desktop_core::screens::{scenes, NOW};
use argus_gpui::chrome::shell::Shell;
use argus_gpui::connection::caller::Caller;
use argus_gpui::connection::heard::Heard;
use argus_gpui::window;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{point, px, size, AnyWindowHandle, App, AppContext as _, Bounds, Entity, WindowBounds, WindowOptions};

/// The page's shots are 1440x900 CSS pixels at a device scale of 2.
const WIDTH: f32 = 1440.0;
const HEIGHT: f32 = 900.0;

const DEFAULT: &str = "settings,workspace-agent,workspace-shell,workspace-exited,workspace-split,workspace-grid,workspace-grid-four,workspace-console,workspace-empty,docks,float,review,review-comment,history,add-project,palette,shortcuts,switcher,checkouts,checkouts-files,checkouts-delete,code,code-edit,code-start,code-search,browsers,browsers-none,browser,browser-held,browser-tabless,browsers-tabless,agent-views,agent-view-page,saved-view,extension-stage,extension-dialog,features-list,features-board,capture,feature-work,feature-record,feature-flows,link,link-map,link-draft,link-typed,link-refused,reconcile,reconcile-ticked,reconcile-edited,reconcile-none,reconcile-asked,reconcile-carrying,reconcile-failed,palette-commands,palette-query,palette-features-found,palette-reading,palette-none,palette-moved,shortcuts-filtered,shortcuts-none,shortcuts-section";

/// The scenes the page finishes with a click on a control, and the control: the window is brought to the
/// same place by pressing it, so the menu or dialog is open in the picture as it is in the page's.
fn click_for(scene: &str) -> Option<&'static str> {
    match scene {
        "switcher" => Some("rail-projects"),
        _ => None,
    }
}

/// What a window does after a scene is brought to its screen: the page's palette and sheet keep what is
/// typed in them, so those states are made in the window, as a person makes them.
#[derive(Clone, Copy)]
enum Act {
    Type(&'static str),
    Press(&'static str),
    /// A press on the element with this id, to put the keys in a field.
    Click(&'static str),
}

/// The scenes that are a core scene and what is typed or pressed once it is open: every state of the
/// palette (SHELL-PALETTE-4..11) and the shortcuts sheet (SHELL-SHEET-2..9) the specs list that no core
/// scene reaches.
fn variant(name: &str) -> Option<(&'static str, &'static [Act])> {
    use Act::{Click, Press, Type};
    Some(match name {
        // A leading `>` limits the search to commands, with the whole list under the first rows (-5).
        "palette-commands" => ("palette", &[Type(">")]),
        // Searched, the groups are ranked by how well they match and the matched letters marked (-6, -7).
        "palette-query" => ("palette-features", &[Type("trim")]),
        // `#` limits it to the features of the board that has arrived (-5).
        "palette-features-found" => ("palette-features", &[Type("#")]),
        // Before the board has arrived `#` says so (-11).
        "palette-reading" => ("palette", &[Type("#")]),
        // Nothing matches (-11).
        "palette-none" => ("palette", &[Type("zzzzq")]),
        // The highlight moved down, and the row says what Enter does (-9, -10).
        "palette-moved" => ("palette", &[Press("down"), Press("down"), Press("down")]),
        // Searching the sheet narrows it to the sections with a match (SHEET-7).
        "shortcuts-filtered" => ("shortcuts", &[Click("sheet-find"), Type("palette")]),
        "shortcuts-none" => ("shortcuts", &[Click("sheet-find"), Type("zzzzq")]),
        // Down changes section (SHEET-9).
        "shortcuts-section" => ("shortcuts", &[Press("down"), Press("down"), Press("down")]),
        _ => return None,
    })
}

/// The scene `name` is made from: its own, or the core scene its variant starts from.
fn base_of(name: &str) -> &str {
    variant(name).map_or(name, |(base, _)| base)
}

/// Types and presses `acts` into the window, a frame apart.
async fn act(cx: &mut gpui_kit::AsyncApp, handle: AnyWindowHandle, acts: &[Act]) {
    use gpui_kit::{Keystroke, Modifiers};
    for act in acts {
        if let Act::Click(id) = act {
            cx.update(|cx| press(cx, handle, id));
            cx.background_executor().timer(Duration::from_millis(200)).await;
            continue;
        }
        let strokes: Vec<Keystroke> = match act {
            Act::Type(text) => text.chars().map(|ch| Keystroke { modifiers: Modifiers::none(), key: ch.to_string(), key_char: Some(ch.to_string()) }).collect(),
            Act::Press(key) => vec![Keystroke::parse(key).unwrap()],
            Act::Click(_) => Vec::new(),
        };
        for stroke in strokes {
            cx.update(|cx| {
                let _ = cx.update_window(handle, |_, window, cx| window.dispatch_keystroke(stroke, cx));
            });
            cx.background_executor().timer(Duration::from_millis(60)).await;
        }
    }
}

fn out_dir() -> PathBuf {
    std::env::var_os("ARGUS_GPUI_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/argus-gpui-shots"))
}

/// What the daemon says to the editor in `name`'s scene, once the window has made the editor: the scene sends
/// no edit events (the page's own harness delivers them by hand), so the file's text is the lines the page
/// was sent, answered as the daemon answers an open.
fn edits_in(name: &str, events: &[Event]) -> Vec<Event> {
    if name != "code-edit" {
        return Vec::new();
    }
    let Some(page) = events.iter().find_map(|event| match event {
        Event::Code(page) => Some(page),
        _ => None,
    }) else {
        return Vec::new();
    };
    let text: String = page.lines.iter().map(|row| row.parts.iter().map(|(text, _)| text.as_str()).collect::<String>() + "\n").collect();
    let path = page.path.clone().unwrap_or_default();
    vec![Event::Edit(EditEvent::Opened { checkout: page.checkout, path, version: 0, text })]
}

/// Everything a fresh page would hear in `name`'s scene, and the first frames apart: a frame for a cell
/// that has no screen yet is dropped, so they are heard once the window has drawn the cells the
/// rail names.
fn heard_in(name: &str) -> (Vec<Event>, Vec<Event>) {
    let scene = scenes().into_iter().find(|scene| scene.name == base_of(name)).unwrap_or_else(|| panic!("no scene {name}"));
    let mut session = (scene.build)();
    let mut events = session.everything(NOW);
    // The window asks the session for the palette's lists as it opens (`Call::Palette`), and the harness's calls go
    // nowhere, so the session is asked here: a scene that opens the palette shows what a real one lists.
    if base_of(name).starts_with("palette") {
        session.call(argus_desktop_core::connection::page::Call::Palette { open: true });
        events.extend(session.events(NOW));
    }
    events.extend(session.take_frames());
    let (frames, rest): (Vec<Event>, Vec<Event>) = events.into_iter().partition(|event| matches!(event, Event::Frame(_)));
    (rest, frames)
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
        // The page's shots are taken with a dark system, so its default theme draws as Mocha; this machine's
        // own light or dark must not decide which theme the photographs are of.
        cx.set_global(argus_gpui::settings::look::Forced(true));
        cx.spawn(async move |cx| {
            for name in names {
                // `scene+control` is the scene with that control pressed, for what only the window holds (a diagram expanded).
                let (scene, pressed) = name.split_once('+').map_or((name.as_str(), None), |(scene, id)| (scene, Some(id.to_owned())));
                let (events, frames) = heard_in(scene);
                let edits = edits_in(scene, &events);
                let result = cx.update(|cx| open(cx, events));
                let (handle, model) = match result {
                    Ok((handle, heard)) => {
                        // The window has made the cells the rail names, so their frames have a screen.
                        cx.update(|cx| {
                            heard.update(cx, |heard, cx| {
                                for frame in frames {
                                    heard.hear(frame);
                                }
                                cx.notify();
                                // What the session asked the window to open (the palette, the sheet) is said now that it can.
                                heard.announce(cx);
                            })
                        });
                        (handle, heard)
                    }
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
                // The editor asked for its file when it drew the page, so the daemon's answer has a file to land in.
                if !edits.is_empty() {
                    cx.update(|cx| {
                        model.update(cx, |heard, cx| {
                            for edit in edits {
                                heard.hear(edit);
                            }
                            heard.announce(cx);
                        })
                    });
                    cx.background_executor().timer(Duration::from_millis(500)).await;
                }
                if let Some((_, acts)) = variant(scene) {
                    act(cx, handle, acts).await;
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                }
                if let Some(id) = pressed.as_deref().or_else(|| click_for(scene)) {
                    cx.update(|cx| press(cx, handle, id));
                    cx.background_executor().timer(Duration::from_millis(500)).await;
                }
                let shot = cx.update(|cx| {
                    cx.update_window(handle, |_, window, cx| {
                        // A box measured as it is laid out asks to be drawn again at its width, and a
                        // view's own deferred layout asks again: enough frames for both to settle.
                        for _ in 0..6 {
                            window.render_frame(cx);
                        }
                        let scale = window.scale_factor();
                        let viewport = window.viewport_size();
                        window.render_to_image().map(|image| (image, scale, viewport))
                    })
                });
                match shot {
                    Ok(Ok((image, scale, viewport))) => {
                        let path = dir.join(format!("{}.png", name.replace('+', "-")));
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
fn open(cx: &mut App, events: Vec<Event>) -> anyhow::Result<(AnyWindowHandle, Entity<Heard>)> {
    // Windows are left open, and the overlay host is the app's: a dialog still up in an earlier scene's window
    // would keep this one's from opening, so each scene starts with a host that holds nothing.
    argus_gpui::kit::overlay::host::Overlays::install(cx);
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
    let model = heard.clone();
    let handle = gpui_kit::open_window(options, cx, move |window, cx| cx.new(|cx| Shell::new(heard, caller, window, cx)))?;
    Ok((handle.0, model))
}

/// Presses the control named `id`, as the page's scene presses it.
fn press(cx: &mut App, handle: AnyWindowHandle, id: &str) {
    use gpui_kit::test::TestEventExt as _;
    let _ = cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        if let Some(found) = window.try_find(id.to_owned()) {
            let at = found.bounds().center();
            window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
            window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
        }
        window.render_frame(cx);
    });
}

/// Every scene the harness photographs is a core scene or a variant of one, so a scene the window is
/// held to cannot name a screen core no longer builds. The test needs no window.
#[test]
fn every_default_scene_is_a_core_scene_or_a_variant_of_one() {
    let known: Vec<&str> = scenes().into_iter().map(|scene| scene.name).collect();
    let mut seen = std::collections::HashSet::new();
    for name in DEFAULT.split(',') {
        assert!(seen.insert(name), "{name} is listed twice");
        assert!(known.contains(&base_of(name)), "{name} is made from {}, which core has no scene for", base_of(name));
    }
    for name in ["palette-commands", "palette-reading", "palette-none", "palette-moved", "shortcuts-filtered", "shortcuts-none", "shortcuts-section", "reconcile-none", "reconcile-asked", "reconcile-carrying", "link-map"] {
        assert!(seen.contains(name), "{name} is a state the specs list and the harness no longer photographs");
    }
}
