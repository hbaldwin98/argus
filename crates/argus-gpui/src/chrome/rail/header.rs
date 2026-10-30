//! The panel's header: the project the rail lists with its switcher, then Search and New as icon
//! buttons (`project/Blend.dc.html`).
//!
//! It is a 44px band with a hairline under it, level with the strip's logo cell and the stage's
//! title bar, and its empty part moves the window as the title bar's left end does. Once the tree
//! has loaded the name is a button, with one project, several or none, that opens the switcher under
//! it (`switcher`), as the page's does; before that the heading is empty. The button is outside the
//! drag area, as the page's is `no-drag`. A folded rail has no panel, so Search and New are the
//! strip's then.

use argus_desktop_core::connection::page::Rail;
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{h_flex, ActiveTheme as _};
use gpui_kit::{div, prelude::*, Context, MouseButton, WindowControlArea};

use super::popup;
use super::row::Ctx;
use super::slice::Slice;
use super::switcher;
use super::RailView;
use crate::chrome::nav::Nav;
use crate::keys::over::run_over;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem, Type};
use crate::theme::Tokens;

impl RailView {
    pub(super) fn header(&self, rail: &Rail, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let name = rail.project.as_ref().map_or_else(|| "Projects".to_owned(), |project| project.name.clone());
        let switchable = rail.loaded;
        let tokens = *tokens;
        let label = Type { weight: tokens.weight_semibold, size: 14.0, leading: 1.3 };
        let drag = |area: gpui_kit::Div| {
            area.window_control_area(WindowControlArea::Drag)
                .when(!cfg!(windows), |this| this.on_mouse_down(MouseButton::Left, |_, window, _| crate::window::drag::start_move(window)))
        };
        let row = h_flex().id("rail-brand").flex_shrink_0().h(rem(tokens.sp_44)).pl(rem(14.0)).pr(rem(tokens.sp_8)).gap(rem(tokens.sp_6)).items_center().border_b_1().border_color(tokens.line);
        let held = ctx.held;
        let fill = tokens.fill;
        let search = icon_button("nav-search", "Search", false, &tokens)
            .tooltip(tip(slice.titles.first().cloned().unwrap_or_else(|| "Search".into())))
            .on_click(move |_, window, cx| {
                // The palette is the window's own, as Ctrl K is: it opens it and the palette tells the session.
                if !held {
                    run_over(Action::Palette, window, cx);
                }
            })
            .child(Glyph::Search.at(15.0));
        let at_new = Nav::ALL.iter().position(|nav| *nav == Nav::New).unwrap_or_default();
        let lit = Nav::New.lit(slice.rail.stage, slice.git_up);
        let new = icon_button("nav-new", "New", lit, &tokens)
            .tooltip(tip(slice.titles.get(at_new).cloned().unwrap_or_else(|| "New".into())))
            .bg(fill)
            .text_color(if lit { tokens.accent } else { tokens.fg })
            .on_click(cx.listener(move |this, _, _, cx| this.go(Nav::New, held, cx)))
            .child(Glyph::Plus.at(15.0));
        let tools = h_flex().flex_none().gap(rem(tokens.sp_4)).items_center().child(search).child(new);
        if !switchable {
            return row.child(drag(h_flex().h_full().flex_1().min_w_0())).child(tools).into_any_element();
        }
        let items = switcher::items(rail);
        let (weak, heard) = (cx.weak_entity(), self.heard.clone());
        let variant = ButtonCustomVariant::new(cx).foreground(tokens.fg).hover(tokens.fill).active(tokens.bg_selected);
        // `.projbtn`: the name and a chevron on a button that lifts to the fill.
        let button = Button::new("rail-projects")
            .custom(variant)
            .h(rem(tokens.control_h))
            .px(rem(tokens.sp_6))
            .rounded(pixels(tokens.r_sm, cx.theme().font_size))
            .tooltip("Switch project")
            .child(
                h_flex()
                    .gap(rem(tokens.sp_6))
                    .items_center()
                    .child(div().min_w_0().truncate().typed(label).child(name))
                    .child(Glyph::Down.at(tokens.icon_xs).text_color(tokens.fg_faint)),
            )
            .dropdown_menu(move |menu, window, cx| popup::fill(menu, &items, &weak, &heard, tokens, window, cx));
        row.child(div().min_w_0().child(button)).child(drag(h_flex().h_full().flex_1().min_w_0())).child(tools).into_any_element()
    }
}
