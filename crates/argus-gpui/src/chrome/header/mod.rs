//! The title bar across the top of the stage: 44px, like the rail strip's logo cell and the panel's head.
//! The crumb (repository, branch, then the pane's status dot and name or the stage's), the stage's
//! tabs, the pane's size, the drag regions that move the window, the notice pill, the stage's tools
//! and the window's three controls (SHELL-STAGE-7..9).
//!
//! What it says is `StripPage` (core); what the stage's own view adds is a [`slots`] offer. The
//! pieces that need a handle on the session (the `...` menu, the Views launcher) are built by the
//! view that has one and handed in, so this stays an element.

pub mod crumb;
pub mod launcher;
pub mod meta;
pub mod more;
pub mod slots;
pub mod tabs;

use argus_desktop_core::chrome::strip::StripPage;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, App, MouseButton, SharedString, Window, WindowControlArea};

use self::tabs::Tabs;
use super::stage::route::narrow;
use super::window_controls::{control, Control};
use crate::kit::icons::Glyph;
use crate::kit::pill::{status_pill, status_word};
use crate::kit::status_dot::status_dot;
use crate::style::Typed as _;
use crate::theme::scale::{css, rem};
use crate::theme::Tokens;

/// The bar: a drag region holding the crumb and the title on one line (the page's `.head-stack`
/// sets them one over the other), then the stage's tabs, a drag region that grows, then the notice
/// and the stage's tools, then the three window controls.
#[derive(IntoElement)]
pub struct Header {
    strip: StripPage,
    size: SharedString,
    /// The daemon is gone: what the strip says is the last it heard, and dimmed (SHELL-STAGE-14).
    held: bool,
    /// The dictation chip, beside the launcher, in a window with dictation.
    dictation: Option<AnyElement>,
    launcher: Option<AnyElement>,
    /// The stage's tools, or the pane's `...` menu when it has no tools of its own.
    tools: Option<AnyElement>,
    /// The pill the window says something in, left of the dictation chip.
    notice: Option<AnyElement>,
    /// The status dot before the pane's name.
    dot: Option<&'static str>,
    /// The stage's tabs, right after the crumb.
    tabs: Option<Tabs>,
}

impl Header {
    /// A strip that only names the window, as the probe window shows it.
    pub fn new() -> Self {
        Self { strip: StripPage::titled("argus"), size: SharedString::default(), held: false, dictation: None, launcher: None, tools: None, notice: None, dot: None, tabs: None }
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

    pub fn dictation(mut self, chip: Option<AnyElement>) -> Self {
        self.dictation = chip;
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

    pub fn notice(mut self, notice: Option<AnyElement>) -> Self {
        self.notice = notice;
        self
    }

    /// The status of the pane the bar names, drawn as a dot before its name.
    pub fn dot(mut self, status: Option<&'static str>) -> Self {
        self.dot = status;
        self
    }

    /// The stage's tabs, which sit right after the crumb.
    pub fn tabs(mut self, tabs: Option<Tabs>) -> Self {
        self.tabs = tabs;
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
        let width = css(window.viewport_size().width, window.rem_size());
        let maximize = if window.is_maximized() { Glyph::WinRestore } else { Glyph::WinMax };
        let StripPage { crumb, title, status, resumed, meta, meta_tip, pill, .. } = self.strip;
        // `.head-status` is drawn for the folded rail alone, where the row is not seen.
        let chip = status.filter(|_| narrow(width, tokens.narrow_below));
        let tools = self.notice.is_some() || self.dictation.is_some() || self.launcher.is_some() || self.tools.is_some();
        let tabbed = self.tabs.is_some();
        let sized = !self.size.is_empty() && width > tokens.head_size_hidden_below;
        // The bar: on the surface colour, a hairline under it, as tall as the rail strip's logo cell and the panel's head.
        h_flex()
            .id("header")
            .test_support()
            .h(rem(tokens.sp_44))
            .flex_shrink_0()
            .items_stretch()
            .bg(tokens.see_surface)
            .border_b_1()
            .border_color(tokens.line)
            .child(
                h_flex()
                    .id("drag")
                    .test_support()
                    .aria_label(if self.held { "Strip, as last heard" } else { "Strip" })
                    .min_w_0()
                    .items_center()
                    .pl(rem(tokens.sp_20))
                    // With tabs after it the crumb takes what it needs; the bar's rest is the next drag area.
                    .when(!tabbed, |head| head.flex_1().pr(rem(tokens.sp_16)))
                    .when(self.held, |head| head.opacity(tokens.opacity_dim))
                    // Windows answers a hit test over this area with HTCAPTION,
                    // which gives drag, double-click to maximize and Win11's
                    // snap layouts for free; elsewhere the move starts here.
                    .window_control_area(WindowControlArea::Drag)
                    .when(!cfg!(windows), |this| {
                        this.on_mouse_down(MouseButton::Left, |_, window, _| crate::window::drag::start_move(window))
                    })
                    .child(
                        h_flex()
                            .min_w_0()
                            .items_center()
                            .gap(rem(tokens.sp_4))
                            .when(!crumb.is_empty(), |this| this.child(crumb::crumb(&crumb, &tokens)))
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap(rem(tokens.sp_8))
                                    .items_center()
                                    // The pane's status leads its name, as it does in the rail.
                                    .children(self.dot.map(|status| status_dot("head-dot", status, &tokens)))
                                    .child(div().typed(tokens.type_body).weighted(tokens.weight_medium).text_color(tokens.fg).truncate().child(SharedString::from(title)))
                                    .children(chip.map(|status| status_pill("head-status", status_word(status).to_owned(), status, &tokens)))
                                    .children(pill.map(|pill| status_pill("head-pill", pill.word, pill.status, &tokens)))
                                    .when(!meta.is_empty(), |row| row.child(self::meta::meta(&meta, meta_tip, &tokens)))
                                    .children(resumed.map(|resumed| {
                                        // `.meta.ellipsis`
                                        div().id("head-resumed").test_support().aria_label(SharedString::from(resumed.clone())).min_w_0().typed(tokens.type_small).text_color(tokens.fg_muted).truncate().child(SharedString::from(resumed))
                                    })),
                            ),
                    ),
            )
            // The tabs are outside the drag area: Windows would answer a press on them with a move.
            .children(self.tabs.map(|tabs| self::tabs::tabs(tabs, &tokens)))
            .when(tabbed || sized, |bar| {
                // The rest of the bar moves the window. `.head-size` is debugging detail at its end, faint, and gone when the bar is short of room.
                bar.child(
                    h_flex()
                        .id("drag-rest")
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .justify_end()
                        .px(rem(tokens.sp_16))
                        .window_control_area(WindowControlArea::Drag)
                        .when(!cfg!(windows), |this| {
                            this.on_mouse_down(MouseButton::Left, |_, window, _| crate::window::drag::start_move(window))
                        })
                        .when(sized, |this| {
                            this.child(
                                div()
                                    .id("head-size")
                                    .test_support()
                                    .aria_label(self.size.clone())
                                    .flex_none()
                                    .font_family(mono)
                                    .text_size(rem(tokens.fs_xs))
                                    .text_color(tokens.fg_faint)
                                    .whitespace_nowrap()
                                    .child(self.size),
                            )
                        }),
                )
            })
            .when(tools, |strip| {
                // `.strip-tools`: the notice, the launcher and the stage's tools, a hairline before the controls.
                strip.child(
                    h_flex()
                        .id("strip-tools")
                        .flex_none()
                        .items_center()
                        .gap(rem(tokens.sp_8))
                        .pr(rem(tokens.sp_8))
                        .children(self.notice)
                        .children(self.dictation)
                        .children(self.launcher)
                        .children(self.tools)
                        .child(div().ml(rem(tokens.sp_4)).w(rem(tokens.hairline)).h(rem(tokens.sp_20)).bg(tokens.line)),
                )
            })
            .child(control("minimize", Glyph::WinMin, Control::Minimize, tokens))
            .child(control("maximize", maximize, Control::Maximize, tokens))
            .child(control("close", Glyph::WinClose, Control::Close, tokens))
    }
}

#[cfg(test)]
mod tests;
