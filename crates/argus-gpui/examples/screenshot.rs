//! Can a GPUI view be rendered to a PNG without a person looking at a window?
//!
//! The plan gates every ported surface on a screenshot, so this answers it
//! for Windows. `HeadlessAppContext` cannot: it asks `gpui_platform` for a
//! `PlatformHeadlessRenderer`, and that returns `None` off macOS and Linux.
//! (`VisualTestAppContext` is macOS-only too.) What does exist is the real
//! `DirectXRenderer::render_to_image`, reached through `Window::render_to_image`
//! on a real window of the real platform: parked off-screen at (-10000,
//! -10000), never shown, drawn and read back without being presented.
//!
//! Running the example writes `target/argus-gpui-shots/sample.png` and prints
//! what it measured; the test below asserts the Ledger surfaces' pixels.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use argus_gpui::theme::Tokens;
use argus_gpui::window::Header;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    point, prelude::*, px, size, AnyWindowHandle, App, Bounds, Context, Render, Window, WindowBounds, WindowOptions,
};
use image::RgbaImage;

/// Logical size of the shot; the image is this times the display's scale.
const WIDTH: f32 = 480.0;
const HEIGHT: f32 = 200.0;
const SIDEBAR: f32 = 160.0;

/// A sidebar row and a stage header, built only from the Ledger palette and
/// the shell's own [`Header`], so the shot shows the real chrome.
struct Sample;

impl Render for Sample {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        h_flex()
            .size_full()
            .bg(tokens.bg_sunken)
            .text_color(tokens.fg)
            .child(
                v_flex()
                    .w(px(SIDEBAR))
                    .h_full()
                    .flex_shrink_0()
                    .bg(tokens.bg_chrome)
                    .border_r_1()
                    .border_color(tokens.line)
                    .p_2()
                    .child(
                        h_flex()
                            .h(px(28.0))
                            .px_2()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(tokens.bg_selected)
                            .text_color(tokens.fg)
                            .text_sm()
                            .child("gpui-migration"),
                    ),
            )
            .child(v_flex().flex_1().h_full().child(Header::new().title("argus")))
    }
}

/// A rendered frame and how long each phase took.
struct Shot {
    image: RgbaImage,
    /// Device pixels per logical pixel, and the window's logical size: the
    /// platform does not honour the requested size, so both are measured.
    scale: f32,
    logical: (f32, f32),
    first_ms: u128,
    again_ms: u128,
}

/// Opens the sample on the real Windows platform and reads its frame back.
///
/// The window is never shown and sits off-screen. `window.render_frame` builds
/// the scene that `Window::render_to_image` draws into DirectX's render target
/// and copies to the CPU; without it the capture is of a frame never laid out.
/// It runs inside `Application::run` because the real platform owns the
/// message loop, which `cx.quit()` ends once the frame is in hand.
fn capture() -> Result<Shot> {
    let out: Rc<RefCell<Option<Result<Shot>>>> = Rc::new(RefCell::new(None));
    let sink = out.clone();
    argus_gpui::window::application().run(move |cx| {
        argus_gpui::window::init(cx);
        *sink.borrow_mut() = Some(shoot(cx));
        cx.quit();
    });
    let shot = out.borrow_mut().take();
    shot.context("the application quit before the frame was captured")?
}

fn shoot(cx: &mut App) -> Result<Shot> {
    let started = Instant::now();
    let bounds = Bounds {
        origin: point(px(-10000.0), px(-10000.0)),
        size: size(px(WIDTH), px(HEIGHT)),
    };
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            show: false,
            ..Default::default()
        },
        |_, cx| cx.new(|_| Sample),
    )?;
    // Through the untyped handle: `WindowHandle::update` leases the root view,
    // which the frame being drawn needs to render.
    let handle: AnyWindowHandle = handle.into();
    let (image, scale, logical) = cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let viewport = window.viewport_size();
        let logical = (viewport.width.as_f32(), viewport.height.as_f32());
        window
            .render_to_image()
            .map(|image| (image, window.scale_factor(), logical))
    })??;
    let first_ms = started.elapsed().as_millis();

    // What a suite pays per scene once the device and glyph atlas are warm.
    let started = Instant::now();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_to_image()
    })??;
    let again_ms = started.elapsed().as_millis();
    Ok(Shot {
        image,
        scale,
        logical,
        first_ms,
        again_ms,
    })
}

/// `target/argus-gpui-shots/<name>.png`, under the workspace's target dir.
fn shot_path(name: &str) -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    target.join("argus-gpui-shots").join(format!("{name}.png"))
}

fn save(image: &RgbaImage, name: &str) -> Result<PathBuf> {
    let path = shot_path(name);
    std::fs::create_dir_all(path.parent().expect("shot_path has a parent"))?;
    image
        .save(&path)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

fn main() -> Result<()> {
    let shot = capture()?;
    let path = save(&shot.image, "sample")?;
    println!(
        "{}x{} px at scale {} (requested {WIDTH}x{HEIGHT} logical, got {:?}) -> {}",
        shot.image.width(),
        shot.image.height(),
        shot.scale,
        shot.logical,
        path.display()
    );
    println!(
        "first shot {} ms, second shot {} ms",
        shot.first_ms, shot.again_ms
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_gpui::theme::ledger;

    /// The pixel at logical `(x, y)`, scaled to the image's device pixels.
    fn at(shot: &Shot, x: f32, y: f32) -> [u8; 3] {
        let p = shot
            .image
            .get_pixel((x * shot.scale) as u32, (y * shot.scale) as u32);
        [p[0], p[1], p[2]]
    }

    fn rgb(colour: gpui_kit::Hsla) -> [u8; 3] {
        let c = colour.to_rgb();
        [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8)
    }

    // GPUI.md: "a headless render_to_image has to work before the port
    // starts". The Ledger surfaces are the contract a shot has to honour:
    // the stage is Ledger's `bg_sunken` and the chrome its `bg_chrome`, exactly.
    #[test]
    fn renders_the_ledger_surfaces_to_a_png() {
        let t = ledger::tokens();
        let shot = capture().expect("a real window renders off-screen on this machine");
        let path = save(&shot.image, "sample").unwrap();
        assert!(path.exists());

        let (w, h) = shot.logical;
        assert_eq!(
            (shot.image.width(), shot.image.height()),
            ((w * shot.scale).round() as u32, (h * shot.scale).round() as u32),
            "the image is the viewport in device pixels"
        );
        // The stage body, well clear of the header, the sidebar and any text.
        assert_eq!(at(&shot, w / 2.0, h / 2.0), rgb(t.bg_sunken), "stage background");
        // The sidebar's empty lower part.
        assert_eq!(at(&shot, 40.0, h - 40.0), rgb(t.bg_chrome), "sidebar chrome");
        // The header strip, between its title and the window controls.
        assert_eq!(at(&shot, w / 2.0, t.header_h / 2.0), rgb(t.bg_chrome), "header chrome");
        // The selected sidebar row, right of its label.
        assert_eq!(at(&shot, 140.0, 8.0 + 14.0), rgb(t.bg_selected), "selected row");
    }
}
