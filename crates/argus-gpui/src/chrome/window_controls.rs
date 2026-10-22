//! The window's three controls: what each looks like, and what it asks of the window.

use gpui_kit::{div, prelude::*, px, Hsla, WindowControlArea};

use crate::kit::icons::Glyph;
use crate::theme::Tokens;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Control {
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
    fn hover(self, tokens: Tokens) -> (Hsla, Hsla) {
        match self {
            Control::Close => (tokens.win_close_bg, tokens.win_close_fg),
            _ => (tokens.fill, tokens.fg),
        }
    }
}

/// `.winbtn`: 46 wide and as tall as the strip, so the corner is the button's to the edge.
pub fn control(id: &'static str, glyph: Glyph, which: Control, tokens: Tokens) -> impl IntoElement {
    let (hover_bg, hover_fg) = which.hover(tokens);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(tokens.window_control_w))
        .h_full()
        .flex_shrink_0()
        .text_color(tokens.fg_muted)
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
