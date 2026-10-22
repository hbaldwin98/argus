//! The projects to choose among, once there is more than one: the page's project menu, laid out as
//! rows under the brand so no press is lost to a menu that is not here.

use argus_desktop_core::connection::page::{Call, ProjectChoice, Rail};
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, px, Context};

use super::RailView;
use super::row::line;
use crate::theme::Tokens;

impl RailView {
    /// The projects to choose among, once there is more than one: the
    /// page's project menu, laid out as rows under the brand.
    pub(super) fn projects(&self, rail: &Rail, held: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let current = rail.project.as_ref().map(|p| p.id);
        let fill = tokens.fill;
        v_flex()
            .flex_shrink_0()
            .px(px(tokens.sp_8))
            .pb(px(tokens.sp_8))
            .gap(px(tokens.sp_2))
            .border_b_1()
            .border_color(tokens.line)
            .children(rail.projects.iter().map(|choice: &ProjectChoice| {
                let id = choice.id;
                let chosen = Some(id) == current;
                line(format!("project-{id}"), tokens.row_h, tokens)
                    .px(px(tokens.sp_8))
                    .gap(px(tokens.sp_8))
                    .text_size(px(tokens.fs_md))
                    .text_color(if chosen { tokens.fg } else { tokens.fg_muted })
                    .aria_selected(chosen)
                    .aria_label(choice.name.clone())
                    .when(chosen, |row| row.bg(tokens.bg_selected))
                    .when(!chosen, |row| row.hover(move |style| style.bg(fill).text_color(tokens.fg)))
                    .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::SelectProject { project: id })))
                    .child(div().flex_1().min_w_0().truncate().child(choice.name.clone()))
                    .when(choice.needs_you > 0, |row| {
                        row.child(
                            div()
                                .text_size(px(tokens.fs_xs))
                                .text_color(tokens.attention)
                                .child(format!("▲ {}", choice.needs_you)),
                        )
                    })
            }))
    }
}
