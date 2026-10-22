//! The brand row: the logo and the project the rail lists, as the page's `.brand` with its
//! switcher button. It is the title bar's left end, so it moves the window.

use argus_desktop_core::connection::page::Rail;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, px, MouseButton, WindowControlArea};

use super::RailView;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::Type;
use crate::theme::Tokens;

impl RailView {
    /// The brand row: the logo and the project the rail lists, as the page's
    /// `.brand` with its switcher button. The projects themselves are rows
    /// under it (`projects`), so no press is lost to a menu that is not here.
    pub(super) fn brand(&self, rail: &Rail, tokens: &Tokens) -> impl IntoElement {
        let name = rail.project.as_ref().map_or_else(|| "Argus".to_owned(), |project| project.name.clone());
        let switchable = rail.projects.len() > 1;
        h_flex()
            .id("rail-brand")
            .flex_shrink_0()
            .h(px(tokens.header_h))
            .px(px(tokens.sp_16))
            .gap(px(tokens.sp_8))
            .items_center()
            // The sidebar runs to the window's top edge: its brand row is the
            // title bar's left end (`.rail .brand`), so it moves the window.
            .window_control_area(WindowControlArea::Drag)
            .when(!cfg!(windows), |this| {
                this.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            })
            .child(Glyph::Logo.at(16.0).text_color(tokens.accent))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .typed(Type { weight: tokens.weight_semibold, size: tokens.fs_md, leading: 1.3 })
                    .text_color(tokens.fg)
                    .child(name),
            )
            .when(switchable, |row| row.child(Glyph::Down.at(12.0).text_color(tokens.fg_faint)))
    }
}
