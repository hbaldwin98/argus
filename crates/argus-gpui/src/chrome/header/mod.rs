//! The strip across the top of the stage: the crumb over the title with the pane's status chip and
//! resumed note, the pane's size or parked place, the drag region that moves the window, the
//! stage's tools, and the window's three controls (SHELL-STAGE-7..9).
//!
//! What it says is `StripPage` (core); what the stage's own view adds is a [`slots`] offer. The
//! pieces that need a handle on the session (the `...` menu, the Views launcher) are built by the
//! view that has one and handed in, so this stays an element.

pub mod crumb;
pub mod launcher;
pub mod more;
pub mod slots;

use argus_desktop_core::chrome::strip::StripPage;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, AnyElement, App, MouseButton, SharedString, Window, WindowControlArea};

use super::stage::route::narrow;
use super::window_controls::{control, Control};
use crate::kit::icons::Glyph;
use crate::kit::pill::{status_pill, status_word};
use crate::style::Typed as _;
use crate::theme::Tokens;

/// The strip: a drag region that grows, holding the crumb over the title as the page's
/// `.head-stack` does, then the stage's tools, then the three window controls.
#[derive(IntoElement)]
pub struct Header {
    strip: StripPage,
    size: SharedString,
    /// The daemon is gone: what the strip says is the last it heard, and dimmed (SHELL-STAGE-14).
    held: bool,
    launcher: Option<AnyElement>,
    /// The stage's tools, or the pane's `...` menu when it has no tools of its own.
    tools: Option<AnyElement>,
}

impl Header {
    /// A strip that only names the window, as the probe window shows it.
    pub fn new() -> Self {
        Self { strip: StripPage::titled("argus"), size: SharedString::default(), held: false, launcher: None, tools: None }
    }

    /// A strip that is only this title, as the examples and the probe show it.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.strip = StripPage::titled(title);
        self
    }

    /// What the strip says.
    pub fn strip(mut self, strip: StripPage) -> Self {
        self.strip = strip;
        self
    }

    /// What the pane's grid is, or how far back its parked view is, as the page's `.head-size`
    /// says it: faint and mono at the strip's end, gone when empty or when the window is narrow.
    pub fn size(mut self, size: impl Into<SharedString>) -> Self {
        self.size = size.into();
        self
    }

    pub fn held(mut self, held: bool) -> Self {
        self.held = held;
        self
    }

    pub fn launcher(mut self, launcher: Option<AnyElement>) -> Self {
        self.launcher = launcher;
        self
    }

    pub fn tools(mut self, tools: Option<AnyElement>) -> Self {
        self.tools = tools;
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
        let mono = SharedString::from(tokens.font_mono);
        let width = f32::from(window.viewport_size().width);
        let maximize = if window.is_maximized() { Glyph::WinRestore } else { Glyph::WinMax };
        let StripPage { crumb, title, status, resumed, .. } = self.strip;
        // `.head-status` is drawn for the folded rail alone, where the row is not seen.
        let chip = status.filter(|_| narrow(width, tokens.narrow_below));
        let tools = self.launcher.is_some() || self.tools.is_some();
        // `.strip`: on the surface colour, a hairline under it.
        h_flex()
            .id("header")
            .test_support()
            .h(px(tokens.header_h))
            .flex_shrink_0()
            .items_stretch()
            .bg(tokens.bg_surface)
            .border_b_1()
            .border_color(tokens.line)
            .child(
                h_flex()
                    .id("drag")
                    .test_support()
                    .aria_label(if self.held { "Strip, as last heard" } else { "Strip" })
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(tokens.sp_12))
                    .pl(px(tokens.sp_20))
                    .pr(px(tokens.sp_16))
                    .when(self.held, |head| head.opacity(tokens.opacity_dim))
                    // Windows answers a hit test over this area with HTCAPTION,
                    // which gives drag, double-click to maximize and Win11's
                    // snap layouts for free; elsewhere the move starts here.
                    .window_control_area(WindowControlArea::Drag)
                    .when(!cfg!(windows), |this| {
                        this.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .justify_center()
                            .gap(px(tokens.sp_2))
                            .when(!crumb.is_empty(), |this| this.child(crumb::crumb(&crumb, &tokens)))
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap(px(tokens.sp_8))
                                    .items_center()
                                    .child(div().typed(tokens.type_heading).text_color(tokens.fg).truncate().child(SharedString::from(title)))
                                    .children(chip.map(|status| status_pill("head-status", status_word(status).to_owned(), status, &tokens)))
                                    .children(resumed.map(|resumed| {
                                        // `.meta.ellipsis`
                                        div().id("head-resumed").test_support().aria_label(SharedString::from(resumed.clone())).min_w_0().typed(tokens.type_small).text_color(tokens.fg_muted).truncate().child(SharedString::from(resumed))
                                    })),
                            ),
                    )
                    .when(!self.size.is_empty() && width > tokens.head_size_hidden_below, |this| {
                        // `.head-size`: debugging detail, so faint, and gone when the strip is short of room.
                        this.child(
                            div()
                                .id("head-size")
                                .test_support()
                                .aria_label(self.size.clone())
                                .flex_none()
                                .font_family(mono)
                                .text_size(px(tokens.fs_xs))
                                .text_color(tokens.fg_faint)
                                .whitespace_nowrap()
                                .child(self.size),
                        )
                    }),
            )
            .when(tools, |strip| {
                // `.strip-tools`: the launcher and the stage's tools, a hairline before the controls.
                strip.child(
                    h_flex()
                        .id("strip-tools")
                        .flex_none()
                        .items_center()
                        .gap(px(tokens.sp_8))
                        .pr(px(tokens.sp_8))
                        .children(self.launcher)
                        .children(self.tools)
                        .child(div().ml(px(tokens.sp_4)).w(px(tokens.hairline)).h(px(tokens.sp_20)).bg(tokens.line)),
                )
            })
            .child(control("minimize", Glyph::WinMin, Control::Minimize, tokens))
            .child(control("maximize", maximize, Control::Maximize, tokens))
            .child(control("close", Glyph::WinClose, Control::Close, tokens))
    }
}

#[cfg(test)]
mod tests;
