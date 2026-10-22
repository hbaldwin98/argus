//! How a spike example opens its frameless window and what its header strip holds.
//!
//! Every example calls [`open`] with its own root view; the shell puts the
//! header (drag region and window controls) above it, so no example repeats
//! window chrome. There is no system title bar and no status bar: the
//! header strip is the stage's, as in the Tauri window.

use anyhow::Result;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::{
    div, prelude::*, px, size, AnyView, AnyWindowHandle, App, Application, Bounds, Context, Entity,
    Hsla, MouseButton, Pixels, Render, SharedString, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowControlArea, WindowOptions,
};

use crate::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::{self, Ledger};
use crate::tokens::{fs, size, sp, ty};

/// `--header-h`.
pub const HEADER_HEIGHT: Pixels = px(size::HEADER_H);
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

/// The strip across the top of the stage: a drag region that grows, holding
/// the breadcrumb over the title as the page's `.head-stack` does, and the
/// three window controls.
#[derive(IntoElement)]
pub struct Header {
    title: SharedString,
    crumb: Option<(SharedString, SharedString)>,
}

impl Header {
    pub fn new() -> Self {
        Self {
            title: SharedString::from("argus"),
            crumb: None,
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    /// The repository and branch the title's pane lives in, written small above it.
    pub fn crumb(mut self, crumb: Option<(String, String)>) -> Self {
        self.crumb = crumb.map(|(repository, branch)| (repository.into(), branch.into()));
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
        let mono = cx.theme().mono_font_family.clone();
        let maximize = if window.is_maximized() { Glyph::WinRestore } else { Glyph::WinMax };
        // `.strip`: on the surface colour, a hairline under it.
        h_flex()
            .id("header")
            .h(px(size::HEADER_H))
            .flex_shrink_0()
            .items_stretch()
            .bg(ledger.raised)
            .border_b_1()
            .border_color(ledger.line)
            .child(
                v_flex()
                    .id("drag")
                    .flex_1()
                    .min_w_0()
                    .justify_center()
                    .gap(px(sp::S2))
                    .pl(px(sp::S20))
                    .pr(px(sp::S16))
                    // Windows answers a hit test over this area with HTCAPTION,
                    // which gives drag, double-click to maximize and Win11's
                    // snap layouts for free; elsewhere the move starts here.
                    .window_control_area(WindowControlArea::Drag)
                    .when(!cfg!(windows), |this| {
                        this.on_mouse_down(MouseButton::Left, |_, window, _| {
                            window.start_window_move()
                        })
                    })
                    .when_some(self.crumb, |this, (repository, branch)| {
                        this.child(
                            h_flex()
                                .h(px(sp::S16))
                                .gap(px(sp::S4))
                                .items_center()
                                .text_size(px(fs::XS))
                                .text_color(ledger.faint)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(repository)
                                .child("/")
                                .child(div().font_family(mono).child(branch)),
                        )
                    })
                    .child(
                        div()
                            .typed(ty::HEADING)
                            .text_color(ledger.foreground)
                            .truncate()
                            .child(self.title),
                    ),
            )
            .child(control("minimize", Glyph::WinMin, Control::Minimize, ledger))
            .child(control("maximize", maximize, Control::Maximize, ledger))
            .child(control("close", Glyph::WinClose, Control::Close, ledger))
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

    /// `.winbtn:hover` lifts to `--fill`; `.winbtn.shut:hover` is Windows' red.
    fn hover(self, ledger: Ledger) -> (Hsla, Hsla) {
        match self {
            Control::Close => theme::window_close_hover(),
            _ => (ledger.fill(), ledger.foreground),
        }
    }
}

/// `.winbtn`: 46 wide and as tall as the strip, so the corner is the button's to the edge.
fn control(id: &'static str, glyph: Glyph, which: Control, ledger: Ledger) -> impl IntoElement {
    let (hover_bg, hover_fg) = which.hover(ledger);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(size::WINDOW_CONTROL_W))
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
        .child(glyph.at(10.0))
}

/// Runs `stop` and quits once the last window has closed, so what was kept
/// running for the window (the daemon loop, the frame tick) ends with it
/// rather than updating a window that is gone. `stop` is run before the quit
/// and only once, whichever way the window closed: the header's control, the
/// system's close, or the window being removed from code.
pub fn quit_when_closed(cx: &mut App, stop: impl FnMut() + 'static) -> Subscription {
    quit_when_closed_with(cx, stop, |cx| cx.quit())
}

/// Answers a close of the only window by quitting the app with the window
/// still open. Closing it first makes gpui-pre-windows touch the destroyed
/// handle as the app winds down ("window not found", "Invalid window
/// handle"); a quit tears the window down in the order a timed quit does,
/// which logs nothing. The system's close, Alt+F4 and the header's control
/// all ask here first.
pub fn quit_instead_of_closing(window: &mut Window, cx: &App) {
    window.on_window_should_close(cx, |_, cx| {
        if cx.windows().len() > 1 {
            return true;
        }
        cx.quit();
        false
    });
}

/// [`quit_when_closed`] with the quit given: the test platform's quit does
/// nothing and runs no quit observer, so a test can only see it asked for.
fn quit_when_closed_with(cx: &mut App, mut stop: impl FnMut() + 'static, mut quit: impl FnMut(&mut App) + 'static) -> Subscription {
    cx.on_window_closed(move |cx, _| {
        if cx.windows().is_empty() {
            stop();
            quit(cx);
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use gpui_kit::TestAppContext;

    fn open_empty(cx: &mut TestAppContext) -> AnyWindowHandle {
        cx.update(|cx| {
            open(cx, |_, cx| cx.new(|_| Nothing)).expect("open test window").0
        })
    }

    struct Nothing;

    impl Render for Nothing {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn close(cx: &mut TestAppContext, handle: AnyWindowHandle) {
        cx.update_window(handle, |_, window, _| window.remove_window()).unwrap();
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn closing_the_last_window_stops_what_ran_for_it(cx: &mut TestAppContext) {
        cx.update(init);
        let stops = Rc::new(Cell::new(0));
        let counted = stops.clone();
        let quits = Rc::new(Cell::new(0));
        let quit = quits.clone();
        let _watching = cx.update(|cx| {
            quit_when_closed_with(cx, move || counted.set(counted.get() + 1), move |_| quit.set(quit.get() + 1))
        });
        let first = open_empty(cx);
        let second = open_empty(cx);

        close(cx, first);
        assert_eq!(stops.get(), 0, "stopped while a window was still open");
        assert_eq!(quits.get(), 0, "quit while a window was still open");
        close(cx, second);
        assert_eq!(stops.get(), 1, "the last window closed and nothing stopped");
        assert_eq!(quits.get(), 1, "the last window closed and the app kept running");
    }
}
