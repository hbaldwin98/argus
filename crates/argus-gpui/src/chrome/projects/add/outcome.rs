//! What adding would do, and the fields that say what to call it (SHELL-ADD-6, 7): the outcome
//! beside the folder list for each kind, the project name, and what to skip.
//!
//! The words are core's (`chrome::add_outcome`); the fields are the view's own and keep what is
//! typed over what was sent (`add`, `schedule`).

use argus_desktop_core::chrome::add_outcome::{always_skipped, outcome, Outcome, NEW_FOLDER_LABEL, NEW_NOTE, NOT_A_REPOSITORY};
use argus_desktop_core::chrome::add_project::AddProjectPage;
use gpui_kit::component::input::Input;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, Context, SharedString, TestSupportExt as _};

use super::AddView;
use crate::kit::dialog::{field_label, label};
use crate::kit::ghosts::ghosts;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.meta`: the small muted type.
fn meta(words: impl Into<SharedString>, tokens: &Tokens) -> gpui_kit::Div {
    div().typed(tokens.type_small).text_color(tokens.fg_muted).child(words.into())
}

/// `.meta.mono.ellipsis`: a path or a name, in the code face, cut at the edge.
fn mono(words: impl Into<SharedString>, tokens: &Tokens) -> gpui_kit::Div {
    meta(words, tokens).font_family(SharedString::from(tokens.font_mono)).truncate()
}

impl AddView {
    pub(super) fn outcome(&self, page: &AddProjectPage, tokens: &Tokens, _: &mut Context<Self>) -> impl IntoElement {
        let shell = v_flex().id("ap-outcome").test_support().flex_shrink_0().min_w_0().gap(rem(tokens.sp_6));
        match outcome(page) {
            Outcome::New { target } => shell
                .child(field_label(NEW_FOLDER_LABEL, tokens))
                .child(div().font_family(SharedString::from(tokens.font_mono)).child(Input::new(self.folder.input()).aria_label(NEW_FOLDER_LABEL).h(rem(tokens.control_h))))
                .children(target.map(|target| mono(target.clone(), tokens).id("ap-target").tooltip(tip(target))))
                .child(meta(NEW_NOTE, tokens)),
            Outcome::Looking => shell.child(label("Looking for repositories\u{2026}", tokens)).child(div().h(rem(3.0 * (tokens.ghost_h + tokens.sp_8) + tokens.sp_8)).child(ghosts("ap-looking", &[0.6, 0.8, 0.5], tokens))),
            Outcome::Repository { would_add: Some(name) } => shell.child(label("Would add", tokens)).child(mono(name, tokens)),
            Outcome::Repository { would_add: None } => shell.child(label("Not a repository", tokens)).child(meta(NOT_A_REPOSITORY, tokens)),
            Outcome::Folder { heading, repositories, searched } => shell
                .child(label(heading, tokens))
                .child(
                    v_flex()
                        .id("ap-found")
                        .gap(rem(tokens.sp_2))
                        .max_h(rem(tokens.folders_max_h))
                        .overflow_y_scroll()
                        .children(repositories.into_iter().map(|repo| mono(repo, tokens))),
                )
                .children(searched.map(|words| meta(words, tokens))),
        }
    }

    /// `Project name`, with the default name showing while it is empty.
    pub(super) fn name_row(&self, _: &AddProjectPage, tokens: &Tokens) -> impl IntoElement {
        v_flex()
            .id("ap-name")
            .gap(rem(tokens.sp_6))
            .child(field_label("Project name", tokens))
            .child(Input::new(self.name.input()).aria_label("Project name").h(rem(tokens.control_h)))
    }

    /// `Also skip`, with what is always skipped beside it once the scan has said.
    pub(super) fn skip_row(&self, page: &AddProjectPage, tokens: &Tokens) -> impl IntoElement {
        v_flex()
            .id("ap-skip")
            .gap(rem(tokens.sp_6))
            .child(field_label("Also skip", tokens))
            .children(always_skipped(page).map(|words| meta(words, tokens)))
            .child(div().font_family(SharedString::from(tokens.font_mono)).child(Input::new(self.skip.input()).aria_label("Also skip").h(rem(tokens.control_h))))
    }
}
