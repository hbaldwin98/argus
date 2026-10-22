//! The brand row: the logo and the project the rail lists, as the page's `.brand` with its
//! switcher button. It is the title bar's left end, so it moves the window.
//!
//! With several projects the name is a button that opens the list of them under it (`menus::projects`),
//! as the page's does; the rest of that menu is the project switcher's (audit item 30). The button
//! is outside the drag area, as the page's is `no-drag`. A narrow rail draws the logo alone.

use argus_desktop_core::connection::page::Rail;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::h_flex;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::{div, prelude::*, px, Context, MouseButton, WindowControlArea};

use super::menus;
use super::popup;
use super::row::Ctx;
use super::RailView;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::Type;
use crate::theme::Tokens;

impl RailView {
    pub(super) fn brand(&self, rail: &Rail, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let name = rail.project.as_ref().map_or_else(|| "Argus".to_owned(), |project| project.name.clone());
        let switchable = rail.projects.len() > 1 && !ctx.icons;
        let tokens = *tokens;
        let label = Type { weight: tokens.weight_semibold, size: tokens.fs_md, leading: 1.3 };
        // The sidebar runs to the window's top edge: its brand row is the title bar's left end
        // (`.rail .brand`), so it moves the window.
        let drag = |area: gpui_kit::Div| {
            area.window_control_area(WindowControlArea::Drag)
                .when(!cfg!(windows), |this| this.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()))
        };
        let logo = drag(h_flex().h_full().items_center().pl(px(if ctx.icons { 0.0 } else { tokens.sp_16 })).pr(px(tokens.sp_8)).when(ctx.icons, |logo| logo.flex_1().justify_center()))
            .child(Glyph::Logo.at(tokens.icon_glyph).text_color(tokens.accent));
        let row = h_flex().id("rail-brand").flex_shrink_0().h(px(tokens.header_h)).items_center().child(logo);
        if !switchable {
            return row
                .when(!ctx.icons, |row| {
                    row.child(drag(h_flex().h_full().flex_1().min_w_0().items_center().pr(px(tokens.sp_16))).child(
                        div().min_w_0().truncate().typed(label).text_color(tokens.fg).child(name),
                    ))
                })
                .into_any_element();
        }
        let items = menus::projects(&rail.projects, rail.project.as_ref().map(|project| project.id));
        let (weak, heard) = (cx.weak_entity(), self.heard.clone());
        let variant = ButtonCustomVariant::new(cx).foreground(tokens.fg).hover(tokens.fill).active(tokens.bg_selected);
        // `.projbtn`: the name and a chevron on a button that lifts to the fill.
        let button = Button::new("rail-projects")
            .custom(variant)
            .h(px(tokens.control_h))
            .px(px(tokens.sp_8))
            .rounded(px(tokens.r_sm))
            .tooltip("Projects")
            .child(
                h_flex()
                    .gap(px(tokens.sp_6))
                    .items_center()
                    .child(div().min_w_0().truncate().typed(label).child(name))
                    .child(Glyph::Down.at(tokens.icon_xs).text_color(tokens.fg_faint)),
            )
            .dropdown_menu(move |menu, _, cx| popup::fill(menu, &items, &weak, &heard, tokens, cx));
        row.child(div().min_w_0().child(button))
            .child(drag(h_flex().h_full().flex_1().min_w_0()))
            .into_any_element()
    }
}
