//! The top of the add dialog: what to add, the folder it is in, and the folders in it
//! (SHELL-ADD-3..5).

use argus_desktop_core::chrome::add_outcome::{folders, held_words, kinds, Folders, NO_FOLDERS};
use argus_desktop_core::chrome::add_page::{AddProjectGesture, AddProjectPage};
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, Role, SharedString, TestSupportExt as _};

use super::AddView;
use crate::kit::ghosts::ghosts;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::segment::{pill, track};
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl AddView {
    /// `What to add`: a radio group of the three kinds; a press asks for the kind and the page says it.
    pub(super) fn kinds(&self, page: &AddProjectPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let words = kinds(page).into_iter().enumerate().map(|(at, (kind, words))| {
            pill(("ap-kind", at), Role::RadioButton, page.kind == kind, tokens)
                .aria_label(SharedString::from(words))
                .child(words)
                .on_click(cx.listener(move |this, _, _, _| this.send(AddProjectGesture::Kind { kind })))
        });
        div().flex().child(track("ap-kinds", "What to add", Role::RadioGroup, tokens).children(words))
    }

    /// An up button, disabled with no parent, and the path, which Enter browses to.
    pub(super) fn path_row(&self, page: &AddProjectPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let parent = page.parent.clone();
        let up = icon_button("ap-up", "Up one folder", false, tokens).tooltip(tip("Up one folder")).child(Glyph::Up.at(tokens.icon_sm));
        let up = match parent {
            Some(parent) => up.on_click(cx.listener(move |this, _, _, _| this.send(AddProjectGesture::Browse { path: parent.clone() }))).into_any_element(),
            None => up.opacity(tokens.opacity_dim).cursor_default().into_any_element(),
        };
        h_flex().gap(rem(tokens.sp_6)).items_center().child(up).child(
            div()
                .id("ap-path")
                .test_support()
                .aria_label("Folder")
                .flex_1()
                .min_w_0()
                .font_family(SharedString::from(tokens.font_mono))
                .child(Input::new(self.path.input()).aria_label("Folder").h(rem(tokens.control_h))),
        )
    }

    /// `Folders in <path>`: bars while browsing, the listing's error, nothing in here, or a row a folder.
    pub(super) fn folders(&self, page: &AddProjectPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let words = match &page.path {
            Some(path) => format!("Folders in {path}"),
            None => "Folders".to_owned(),
        };
        let shell = v_flex()
            .id("ap-folders")
            .test_support()
            .aria_label(SharedString::from(words))
            .flex_shrink_0()
            .max_h(rem(tokens.folders_max_h))
            .min_w_0()
            .overflow_y_scroll()
            .gap(rem(tokens.sp_2))
            .p(rem(tokens.sp_6))
            .border_1()
            .border_color(tokens.line)
            .rounded(rem(tokens.r_md));
        match folders(page) {
            Folders::Waiting => shell.child(div().h(rem(tokens.folders_max_h - 2.0 * tokens.sp_6)).child(ghosts("ap-ghosts", &[0.5, 0.7, 0.4, 0.6], tokens))).into_any_element(),
            Folders::Failed(error) => shell.child(div().id("ap-error").test_support().aria_label(SharedString::from(error.clone())).p(rem(tokens.sp_8)).text_color(tokens.danger).child(SharedString::from(error))).into_any_element(),
            Folders::Empty => shell.child(div().p(rem(tokens.sp_8)).typed(tokens.type_small).text_color(tokens.fg_muted).child(NO_FOLDERS)).into_any_element(),
            Folders::Listed => {
                let rows = page.entries.iter().enumerate().map(|(at, entry)| {
                    let name = entry.name.clone();
                    let (hover, open) = (tokens.bg_selected, entry.name.clone());
                    h_flex()
                        .id(("ap-folder", at))
                        .role(Role::Button)
                        .test_support()
                        .aria_label(SharedString::from(format!("Open {}", entry.name)))
                        .gap(rem(tokens.sp_8))
                        .items_center()
                        .min_w_0()
                        .px(rem(tokens.sp_8))
                        .py(rem(tokens.sp_4))
                        .rounded(rem(tokens.r_sm))
                        .cursor_pointer()
                        .text_color(tokens.fg)
                        .hover(move |style| style.bg(hover))
                        .tooltip(tip(format!("Open {open}")))
                        .on_click(cx.listener(move |this, _, _, _| this.send(AddProjectGesture::Into { name: name.clone() })))
                        .child(div().text_color(tokens.fg_faint).child("\u{25b8}"))
                        .child(div().flex_1().min_w_0().truncate().child(SharedString::from(entry.name.clone())))
                        .children(entry.held.map(|held| {
                            let words = SharedString::from(held_words(held, page));
                            div().id(("ap-held", at)).test_support().aria_label(words.clone()).typed(tokens.type_small).text_color(tokens.fg_faint).child(words)
                        }))
                        .child(div().typed(tokens.type_small).text_color(tokens.fg_muted).child(if entry.is_repo { "git" } else { "folder" }))
                });
                shell.children(rows).into_any_element()
            }
        }
    }
}
