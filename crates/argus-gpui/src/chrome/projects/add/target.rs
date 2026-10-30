//! Where the add goes: the project the rail lists, another one of the tree, or a project of its own.
//!
//! The choice is the session's (`AddProjectPage::into`); a press asks for it with `AddTo` and the
//! page says which is chosen. Another project opens a menu of the projects the rail is not listing.

use argus_desktop_core::chrome::add_outcome::{add_to, other_projects, AddTo, ANOTHER_PROJECT, A_NEW_PROJECT};
use argus_desktop_core::chrome::add_page::{AddProjectGesture, AddProjectPage};
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::{div, prelude::*, Context, MouseButton, Role, SharedString};

use super::AddView;
use crate::kit::dialog::field_label;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::kit::segment::{pill, track};
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

impl AddView {
    /// `Add to`: a radio group of where the add goes, over the kinds of add.
    pub(super) fn target(&self, page: &AddProjectPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let chosen = add_to(page);
        let current = page.current.clone().map(|current| {
            let id = current.id;
            pill("ap-to-current", Role::RadioButton, chosen == AddTo::Current, tokens)
                .aria_label(SharedString::from(current.name.clone()))
                .child(div().truncate().child(SharedString::from(current.name)))
                .on_click(cx.listener(move |this, _, _, _| this.send(AddProjectGesture::AddTo { project: Some(id) })))
        });
        let others = other_projects(page);
        let another = (!others.is_empty()).then(|| {
            let items: Vec<MenuItem<u64>> = others.iter().map(|p| MenuItem::choice(p.name.clone(), p.id).marked(page.into.as_ref().is_some_and(|into| into.id == p.id))).collect();
            let words = match (&page.into, chosen) {
                (Some(into), AddTo::Another) => into.name.clone(),
                _ => ANOTHER_PROJECT.to_owned(),
            };
            let (caller, heard, tokens) = (self.caller.clone(), self.heard.clone(), *tokens);
            let menu = Button::new("ap-to-another-button")
                .ghost()
                .small()
                .rounded(pixels(tokens.r_xs, cx.theme().font_size))
                .child(SharedString::from(format!("{words} \u{25be}")))
                .dropdown_menu(move |menu, window, cx| {
                    let (caller, held) = (caller.clone(), heard.clone());
                    popup::fill(
                        menu,
                        &items,
                        tokens,
                        move |cx| held.read(cx).held(),
                        move |project, _, _| caller.call(Call::AddProject { gesture: AddProjectGesture::AddTo { project: Some(*project) } }),
                        window,
                        cx,
                    )
                });
            pill("ap-to-another", Role::RadioButton, chosen == AddTo::Another, &tokens)
                .aria_label(SharedString::from(ANOTHER_PROJECT))
                .px_0()
                // A press on the button must not reach what is under the dialog's scrim.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(menu)
        });
        let new = pill("ap-to-new", Role::RadioButton, chosen == AddTo::New, tokens)
            .aria_label(SharedString::from(A_NEW_PROJECT))
            .child(A_NEW_PROJECT)
            .on_click(cx.listener(|this, _, _, _| this.send(AddProjectGesture::AddTo { project: None })));
        v_flex()
            .id("ap-to")
            .gap(rem(tokens.sp_6))
            .child(field_label("Add to", tokens))
            .child(h_flex().child(track("ap-targets", "Add to", Role::RadioGroup, tokens).children(current).children(another).child(new)))
    }
}
