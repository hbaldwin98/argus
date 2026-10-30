//! The `Into` row: the project that keeps its name and settings, chosen from a menu of the
//! workspace's projects, with how many repositories it holds now.
//!
//! The choice is the session's (`CombinePage::into`); a pick asks for it with `Into` and the page
//! says which is chosen.

use argus_desktop_core::chrome::combine_outcome::{repositories, INTO_LABEL, INTO_NOTE};
use argus_desktop_core::chrome::combine_page::{CombineGesture, CombinePage};
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{h_flex, ActiveTheme as _, Sizable as _};
use gpui_kit::{prelude::*, Context, IntoElement, MouseButton, SharedString};

use super::CombineView;
use crate::kit::caption::note;
use crate::kit::dialog::field_label;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

impl CombineView {
    pub(super) fn destination(&self, page: &CombinePage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let chosen = page.into.as_ref().map_or(0, |into| into.id);
        let items: Vec<MenuItem<u64>> = page
            .choices
            .iter()
            .map(|choice| MenuItem::choice(choice.name.clone(), choice.id).marked(choice.id == chosen).noted(repositories(choice.repositories)))
            .collect();
        let shown = page.choices.iter().find(|choice| choice.id == chosen).map_or_else(String::new, |choice| format!("{} \u{b7} {}", choice.name, repositories(choice.repositories)));
        let (caller, heard, tokens) = (self.caller.clone(), self.heard.clone(), *tokens);
        let menu = Button::new("cp-into-button")
            .ghost()
            .small()
            .rounded(pixels(tokens.r_xs, cx.theme().font_size))
            .child(SharedString::from(format!("{shown} \u{25be}")))
            .dropdown_menu(move |menu, window, cx| {
                let (caller, held) = (caller.clone(), heard.clone());
                popup::fill(
                    menu,
                    &items,
                    tokens,
                    move |cx| held.read(cx).held(),
                    move |project, _, _| caller.call(Call::CombineProjects { gesture: CombineGesture::Into { project: *project } }),
                    window,
                    cx,
                )
            });
        h_flex()
            .id("cp-into")
            .items_center()
            .gap(rem(tokens.sp_12))
            .child(field_label(INTO_LABEL, &tokens))
            // A press on the button must not reach what is under the dialog's scrim.
            .child(h_flex().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(menu))
            .child(note("cp-into-note", INTO_NOTE, &tokens))
    }
}
