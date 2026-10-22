//! The strip across the top of the stage: the breadcrumb over the title, the drag region that
//! moves the window, and its three controls.

use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::{div, prelude::*, px, App, MouseButton, SharedString, Window, WindowControlArea};

use super::window_controls::{control, Control};
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::Tokens;

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
        let tokens = *Tokens::global(cx);
        let mono = cx.theme().mono_font_family.clone();
        let maximize = if window.is_maximized() { Glyph::WinRestore } else { Glyph::WinMax };
        // `.strip`: on the surface colour, a hairline under it.
        h_flex()
            .id("header")
            .h(px(tokens.header_h))
            .flex_shrink_0()
            .items_stretch()
            .bg(tokens.bg_surface)
            .border_b_1()
            .border_color(tokens.line)
            .child(
                v_flex()
                    .id("drag")
                    .flex_1()
                    .min_w_0()
                    .justify_center()
                    .gap(px(tokens.sp_2))
                    .pl(px(tokens.sp_20))
                    .pr(px(tokens.sp_16))
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
                                .h(px(tokens.sp_16))
                                .gap(px(tokens.sp_4))
                                .items_center()
                                .text_size(px(tokens.fs_xs))
                                .text_color(tokens.fg_faint)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(repository)
                                .child("/")
                                .child(div().font_family(mono).child(branch)),
                        )
                    })
                    .child(
                        div()
                            .typed(tokens.type_heading)
                            .text_color(tokens.fg)
                            .truncate()
                            .child(self.title),
                    ),
            )
            .child(control("minimize", Glyph::WinMin, Control::Minimize, tokens))
            .child(control("maximize", maximize, Control::Maximize, tokens))
            .child(control("close", Glyph::WinClose, Control::Close, tokens))
    }
}
