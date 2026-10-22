//! How the main window opens: the application with its assets, the frameless window's options, and
//! `open`, which puts a stage's view under the header.

pub mod quit;

use anyhow::Result;
use gpui_kit::component::v_flex;
use gpui_kit::{
    div, prelude::*, px, size, AnyView, AnyWindowHandle, App, Application, Bounds, Context, Entity, Pixels, Render,
    SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions,
};

use crate::chrome::header::Header;
use crate::kit::fonts;
use crate::theme::{self, Tokens};

/// Below this the sidebar, stage and docks no longer fit side by side.
pub const MIN_SIZE: gpui_kit::Size<Pixels> = size(px(900.0), px(560.0));

/// The application with gpui-kit's bundled icons, which the window controls draw.
pub fn application() -> Application {
    gpui_kit::application().with_assets(gpui_kit::assets::Assets)
}

/// Everything an example does first, in the order it must happen: gpui-kit's
/// own init, then the Ledger theme over it.
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    fonts::install(cx);
    theme::kit::install(cx, theme::ledger::tokens());
}

/// A frameless window: `appears_transparent` is what makes gpui-pre-windows
/// hide the non-client area, so the header's control areas are all that
/// moves and closes it.
pub fn options(cx: &App) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("Argus")),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        // The header moves the window itself on macOS, rather than AppKit.
        app_owns_titlebar_drag: true,
        window_min_size: Some(MIN_SIZE),
        ..Default::default()
    }
}

/// Opens the main window with `build`'s view as the stage under the header.
///
/// Returns the stage's entity so a test or example can drive it.
pub fn open<V: Render>(
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> Result<(AnyWindowHandle, Entity<V>)> {
    let options = options(cx);
    let mut stage_out = None;
    let (handle, _shell) = gpui_kit::open_window(options, cx, |window, cx| {
        let stage = build(window, cx);
        stage_out = Some(stage.clone());
        cx.new(|_| Shell {
            stage: stage.into(),
        })
    })?;
    Ok((
        handle,
        stage_out.expect("open_window ran its build closure"),
    ))
}

/// The header strip over the stage, filling the window.
struct Shell {
    stage: AnyView,
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        v_flex()
            .size_full()
            .bg(tokens.bg_sunken)
            .text_color(tokens.fg)
            .child(Header::new())
            .child(div().flex_1().min_h_0().child(self.stage.clone()))
    }
}
