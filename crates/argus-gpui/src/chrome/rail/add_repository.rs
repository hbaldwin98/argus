//! The quiet row at the foot of a project's repositories: `+ Add a repository`, which opens the add
//! dialog already pointed at that project (the same as the repository menu's first item).
//!
//! It asks the session for the dialog and draws none; a folded rail has no room for the words, and
//! the repository menu is still there.

use argus_desktop_core::chrome::add_page::AddKind;
use argus_desktop_core::connection::page::{Call, Project};
use gpui_kit::{div, prelude::*, Context};

use super::row::{line, Ctx};
use super::RailView;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The row's words.
pub const ADD_REPOSITORY: &str = "Add a repository";

impl RailView {
    pub(super) fn add_repository(&self, project: &Project, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, held) = (project.id, ctx.held);
        let (fill, text) = (tokens.fill, tokens.fg);
        div().px(rem(tokens.sp_8)).mt(rem(tokens.sp_8)).child(
            line("rail-add-repository", tokens.rail_row_h, tokens)
                .px(rem(tokens.sp_4))
                .gap(rem(tokens.sp_6))
                .text_size(rem(tokens.fs_sm))
                .text_color(tokens.fg_faint)
                .aria_label(ADD_REPOSITORY)
                .hover(move |style| style.bg(fill).text_color(text))
                .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::OpenAddToProject { project: id, kind: AddKind::Repository })))
                .child(Glyph::Plus.at(tokens.icon_sm))
                .child(div().typed(tokens.type_small).child(ADD_REPOSITORY)),
        )
    }
}
