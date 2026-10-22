//! Can a live GPUI window switch between Opaque, Transparent, Blurred and Mica,
//! fade its Ledger surfaces to an alpha, colour its DWM border and keep Windows
//! 11's snap-layout flyout on a custom maximize button?
//!
//! The Tauri window gets all of this from CSS alpha plus `window-vibrancy`;
//! here the surfaces carry the alpha themselves and the platform window only
//! supplies what shows through. Set `BACKDROP_TOUR_MS=700` to step through the
//! five appearances unattended and quit, printing each switch.

use std::time::Duration;

use argus_gpui::theme::Tokens;
use argus_gpui::window;
use gpui_kit::component::button::Button;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{h_flex, v_flex, Icon, IconName, Selectable as _, Sizable as _};
use gpui_kit::{
    div, prelude::*, px, App, Context, Entity, Hsla, MouseButton, SharedString, Subscription,
    Window, WindowBackgroundAppearance, WindowControlArea,
};

/// The five effects the desktop's Settings offers, in the order it lists them.
const APPEARANCES: [(&str, WindowBackgroundAppearance); 5] = [
    ("Opaque", WindowBackgroundAppearance::Opaque),
    ("Transparent", WindowBackgroundAppearance::Transparent),
    ("Blurred", WindowBackgroundAppearance::Blurred),
    ("Mica", WindowBackgroundAppearance::MicaBackdrop),
    ("Mica Alt", WindowBackgroundAppearance::MicaAltBackdrop),
];

const MIN_PERCENT: f32 = 40.0;
const MAX_PERCENT: f32 = 100.0;
const DEFAULT_PERCENT: f32 = 80.0;

/// The alpha a surface is filled with. An opaque window has nothing behind
/// it to show, and a fill below 1.0 there would reveal whatever the swap
/// chain holds, so the slider only means something for the other four.
fn fill_alpha(appearance: WindowBackgroundAppearance, percent: f32) -> f32 {
    match appearance {
        WindowBackgroundAppearance::Opaque => 1.0,
        _ => percent.clamp(MIN_PERCENT, MAX_PERCENT) / 100.0,
    }
}

/// A Ledger colour at the stage's alpha; the hairlines stay solid so the
/// layout reads the same through any backdrop.
fn surface(color: Hsla, alpha: f32) -> Hsla {
    color.alpha(alpha)
}

/// `0xRRGGBB` as the `0x00BBGGRR` COLORREF the DWM border attribute takes.
#[cfg(windows)]
fn colorref(rgb: u32) -> u32 {
    ((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff)
}

fn name_of(appearance: WindowBackgroundAppearance) -> &'static str {
    APPEARANCES
        .iter()
        .find(|(_, candidate)| *candidate == appearance)
        .map(|(name, _)| *name)
        .unwrap_or("?")
}

/// The DWM border colour, set on the raw HWND.
///
/// gpui-kit does not re-export `raw-window-handle`, so `Window` cannot hand
/// over its HWND without a dependency this crate lacks; the thread's active
/// window is the same window here (one window, one UI thread) and works once
/// the window has been activated. Both functions are declared by hand rather
/// than adding `windows-sys`, which argus-desktop needs for much more.
#[cfg(windows)]
mod dwm {
    use std::ffi::c_void;

    const DWMWA_BORDER_COLOR: u32 = 34;

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(hwnd: *mut c_void, attr: u32, value: *const c_void, size: u32)
            -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetActiveWindow() -> *mut c_void;
    }

    /// `None` while no window of this thread is active yet; else the HRESULT
    /// (0 is success; Windows 10 answers an error and draws no coloured border).
    pub fn border(rgb: u32) -> Option<i32> {
        let colorref = super::colorref(rgb);
        unsafe {
            let hwnd = GetActiveWindow();
            if hwnd.is_null() {
                return None;
            }
            Some(DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &colorref as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            ))
        }
    }
}

#[cfg(not(windows))]
mod dwm {
    pub fn border(_rgb: u32) -> Option<i32> {
        Some(0)
    }
}

const LINE_RGB: u32 = argus_gpui::theme::themes::ledger::THEME.palette.surface0.rgb;

struct Stage {
    appearance: WindowBackgroundAppearance,
    percent: f32,
    opacity: Entity<SliderState>,
    border_set: bool,
    _subscription: Subscription,
}

impl Stage {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(MIN_PERCENT)
                .max(MAX_PERCENT)
                .step(1.0)
                .default_value(DEFAULT_PERCENT)
        });
        let subscription = cx.subscribe_in(&opacity, window, |this, _, event: &SliderEvent, _, cx| {
            if let SliderEvent::Change(value) = event {
                this.percent = value.end();
                cx.notify();
            }
        });
        let stage = Self {
            appearance: WindowBackgroundAppearance::Transparent,
            percent: DEFAULT_PERCENT,
            opacity,
            border_set: false,
            _subscription: subscription,
        };
        if let Some(ms) = std::env::var("BACKDROP_TOUR_MS")
            .ok()
            .and_then(|ms| ms.parse::<u64>().ok())
        {
            cx.spawn_in(window, async move |this, cx| {
                for (_, appearance) in APPEARANCES {
                    cx.background_executor().timer(Duration::from_millis(ms)).await;
                    this.update_in(cx, |stage, window, cx| stage.set(appearance, window, cx))
                        .ok();
                }
                cx.background_executor().timer(Duration::from_millis(ms)).await;
                this.update_in(cx, |_, _, cx| cx.quit()).ok();
            })
            .detach();
        }
        stage
    }

    fn set(&mut self, appearance: WindowBackgroundAppearance, window: &mut Window, cx: &mut Context<Self>) {
        window.set_background_appearance(appearance);
        self.appearance = appearance;
        println!("appearance: {}", name_of(appearance));
        self.apply_border();
        cx.notify();
    }

    /// Retried from `render` until the window is active: the first frame can
    /// draw before Windows has activated it.
    fn apply_border(&mut self) {
        if self.border_set {
            return;
        }
        if let Some(result) = dwm::border(LINE_RGB) {
            self.border_set = true;
            println!("dwm border #{LINE_RGB:06x}: HRESULT {result:#x}");
        }
    }
}

fn control(
    id: &'static str,
    icon: IconName,
    area: WindowControlArea,
    tokens: Tokens,
) -> impl IntoElement {
    let (hover_bg, hover_fg) = match area {
        WindowControlArea::Close => (tokens.danger, tokens.bg_sunken),
        _ => (tokens.bg_selected, tokens.fg),
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(tokens.header_h + 10.0))
        .h_full()
        .flex_shrink_0()
        .text_color(tokens.fg_muted)
        .hover(|style| style.bg(hover_bg).text_color(hover_fg))
        // Windows hit-tests this area as HTMAXBUTTON, HTMINBUTTON or HTCLOSE
        // and performs the click itself, which is what raises the Win11 snap
        // flyout over the maximize one; no `on_click` is wired on Windows.
        .window_control_area(area)
        .when(!cfg!(windows), move |this| {
            this.on_click(move |_, window, _| match area {
                WindowControlArea::Min => window.minimize_window(),
                WindowControlArea::Max => window.zoom_window(),
                _ => window.remove_window(),
            })
        })
        .child(Icon::new(icon).small())
}

impl Render for Stage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_border();
        let tokens = *Tokens::global(cx);
        let alpha = fill_alpha(self.appearance, self.percent);
        let maximize = if window.is_maximized() {
            IconName::WindowRestore
        } else {
            IconName::WindowMaximize
        };

        let header = h_flex()
            .id("header")
            .h(px(tokens.header_h))
            .flex_shrink_0()
            .items_center()
            .bg(surface(tokens.bg_chrome, alpha))
            .border_b_1()
            .border_color(tokens.line)
            .child(
                h_flex()
                    .id("drag")
                    .flex_1()
                    .h_full()
                    .px_3()
                    .items_center()
                    .text_color(tokens.fg_faint)
                    .text_xs()
                    .window_control_area(WindowControlArea::Drag)
                    .when(!cfg!(windows), |this| {
                        this.on_mouse_down(MouseButton::Left, |_, window, _| {
                            window.start_window_move()
                        })
                    })
                    .child(SharedString::from(format!(
                        "backdrop spike  {}  {:.0}%",
                        name_of(self.appearance),
                        self.percent
                    ))),
            )
            .child(control("minimize", IconName::WindowMinimize, WindowControlArea::Min, tokens))
            .child(control("maximize", maximize, WindowControlArea::Max, tokens))
            .child(control("close", IconName::WindowClose, WindowControlArea::Close, tokens));

        let buttons = h_flex().gap_2().children(APPEARANCES.iter().map(|&(name, appearance)| {
            Button::new(name)
                .small()
                .label(name)
                .selected(self.appearance == appearance)
                .on_click(cx.listener(move |this, _, window, cx| this.set(appearance, window, cx)))
        }));

        // Raised, selected and the page background laid on one another show
        // whether the alpha stacks as it does in the webview.
        let swatches = h_flex().gap_3().children(
            [
                ("background", tokens.bg_sunken),
                ("raised", tokens.bg_surface),
                ("selected", tokens.bg_selected),
            ]
            .map(|(name, color)| {
                div()
                    .w(px(150.0))
                    .h(px(70.0))
                    .p_2()
                    .bg(surface(color, alpha))
                    .border_1()
                    .border_color(tokens.line_strong)
                    .text_xs()
                    .text_color(tokens.fg_muted)
                    .child(name)
            }),
        );

        let note = |text: &'static str| div().text_xs().text_color(tokens.fg_faint).child(text);

        v_flex()
            .size_full()
            .bg(surface(tokens.bg_sunken, alpha))
            .text_color(tokens.fg)
            .child(header)
            .child(
                v_flex()
                    .p_4()
                    .gap_4()
                    .child(buttons)
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                div()
                                    .w(px(150.0))
                                    .text_sm()
                                    .child(format!("Opacity {:.0}%", self.percent)),
                            )
                            .child(Slider::new(&self.opacity).w(px(280.0))),
                    )
                    .child(swatches)
                    .child(
                        v_flex()
                            .gap_1()
                            .child(note("Mica and Mica Alt need Windows 11 22621+; the fills must be below 100% to show it."))
                            .child(note("Blurred is SetWindowCompositionAttribute accent state 4 (acrylic) on Windows."))
                            .child(note("Hover the maximize button for the snap-layout flyout.")),
                    ),
            )
    }
}

fn main() {
    window::application().run(|cx: &mut App| {
        window::init(cx);
        let mut options = window::options(cx);
        // Transparent from the first frame: a window opened Opaque cannot be
        // seen through until the swap chain is switched.
        options.window_background = WindowBackgroundAppearance::Transparent;
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Stage::new(window, cx)))
            .expect("the backdrop window opens");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_opaque_window_ignores_the_slider() {
        assert_eq!(fill_alpha(WindowBackgroundAppearance::Opaque, 40.0), 1.0);
    }

    #[test]
    fn the_slider_is_the_alpha_for_every_see_through_appearance() {
        for (_, appearance) in &APPEARANCES[1..] {
            assert!((fill_alpha(*appearance, 65.0) - 0.65).abs() < 1e-6);
        }
    }

    #[test]
    fn the_alpha_stays_inside_the_slider_range() {
        let blurred = WindowBackgroundAppearance::Blurred;
        assert!((fill_alpha(blurred, 0.0) - 0.4).abs() < 1e-6);
        assert_eq!(fill_alpha(blurred, 250.0), 1.0);
    }

    #[test]
    fn a_surface_keeps_its_colour_and_takes_only_the_alpha() {
        let tokens = argus_gpui::theme::themes::ledger::tokens();
        let faded = surface(tokens.bg_chrome, 0.5);
        assert_eq!(faded.a, 0.5);
        assert_eq!((faded.h, faded.s, faded.l), (tokens.bg_chrome.h, tokens.bg_chrome.s, tokens.bg_chrome.l));
    }

    #[cfg(windows)]
    #[test]
    fn colorref_swaps_red_and_blue() {
        assert_eq!(colorref(0x23252a), 0x2a2523);
        assert_eq!(colorref(0xff0000), 0x0000ff);
    }
}
