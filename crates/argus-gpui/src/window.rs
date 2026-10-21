//! How a spike example opens its frameless window and what its header strip holds.
//!
//! Every example calls [`open`] with its own root view; the shell puts the
//! header (drag region and window controls) above it, so no example repeats
//! window chrome. There is no system title bar and no status bar: the
//! header strip is the stage's, as in the Tauri window.

use anyhow::Result;
use gpui_kit::component::{h_flex, v_flex, Icon, IconName, Sizable as _};
use gpui_kit::{
    div, prelude::*, px, size, AnyView, AnyWindowHandle, App, Application, Bounds, Context, Entity,
    Hsla, MouseButton, Pixels, Render, SharedString, TitlebarOptions, Window, WindowBounds,
    WindowControlArea, WindowOptions,
};

use crate::theme::{self, Ledger};

pub const HEADER_HEIGHT: Pixels = px(34.0);
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
    theme::install(cx);
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
        let ledger = *Ledger::global(cx);
        v_flex()
            .size_full()
            .bg(ledger.background)
            .text_color(ledger.foreground)
            .child(Header::new())
            .child(div().flex_1().min_h_0().child(self.stage.clone()))
    }
}

/// The strip across the top: a drag region that grows, and the three window controls.
#[derive(IntoElement)]
pub struct Header {
    title: SharedString,
}

impl Header {
    pub fn new() -> Self {
        Self {
            title: SharedString::from("argus"),
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }
}

impl Default for Header {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for Header {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let maximize_icon = if window.is_maximized() {
            IconName::WindowRestore
        } else {
            IconName::WindowMaximize
        };
        h_flex()
            .id("header")
            .h(HEADER_HEIGHT)
            .flex_shrink_0()
            .items_center()
            .bg(ledger.chrome)
            .border_b_1()
            .border_color(ledger.line)
            .child(
                h_flex()
                    .id("drag")
                    .flex_1()
                    .h_full()
                    .px_3()
                    .items_center()
                    .text_color(ledger.faint)
                    .text_xs()
                    // Windows answers a hit test over this area with HTCAPTION,
                    // which gives drag, double-click to maximize and Win11's
                    // snap layouts for free; elsewhere the move starts here.
                    .window_control_area(WindowControlArea::Drag)
                    .when(!cfg!(windows), |this| {
                        this.on_mouse_down(MouseButton::Left, |_, window, _| {
                            window.start_window_move()
                        })
                    })
                    .child(self.title),
            )
            .child(control("minimize", IconName::WindowMinimize, Control::Minimize, ledger))
            .child(control("maximize", maximize_icon, Control::Maximize, ledger))
            .child(control("close", IconName::WindowClose, Control::Close, ledger))
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Control {
    Minimize,
    Maximize,
    Close,
}

impl Control {
    fn area(self) -> WindowControlArea {
        match self {
            Control::Minimize => WindowControlArea::Min,
            Control::Maximize => WindowControlArea::Max,
            Control::Close => WindowControlArea::Close,
        }
    }

    /// Close warns in red; the others only lift to the selected grey.
    fn hover(self, ledger: Ledger) -> (Hsla, Hsla) {
        match self {
            Control::Close => (ledger.danger, ledger.background),
            _ => (ledger.selected, ledger.foreground),
        }
    }
}

fn control(id: &'static str, icon: IconName, which: Control, ledger: Ledger) -> impl IntoElement {
    let (hover_bg, hover_fg) = which.hover(ledger);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(HEADER_HEIGHT + px(10.0))
        .h_full()
        .flex_shrink_0()
        .text_color(ledger.muted)
        .hover(|style| style.bg(hover_bg).text_color(hover_fg))
        // Windows draws and handles the click through the hit test, so the
        // press never reaches an `on_click` there; other platforms need one.
        .window_control_area(which.area())
        .when(!cfg!(windows), |this| {
            this.on_click(move |_, window, _| match which {
                Control::Minimize => window.minimize_window(),
                Control::Maximize => window.zoom_window(),
                Control::Close => window.remove_window(),
            })
        })
        .child(Icon::new(icon).small())
}
