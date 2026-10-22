//! The header of an extension's table: each shown column's title as a button that sorts by it, the
//! sort being local to this drawing (EXTVIEW-32).

use argus_desktop_core::views::blocks::table::sort::{click, Direction};
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context, Role, SharedString};

use super::Plan;
use crate::extensions::view::ExtensionView;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::blocks::table::head::track;

impl ExtensionView {
    pub(super) fn grid_head(&self, plan: &Plan, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let marked = plan.sort.map(|(c, _)| c);
        let ascending = plan.sort.is_some_and(|(_, up)| up);
        let (fg, fg_hot) = (tokens.fg_faint, tokens.fg);
        let cells: Vec<AnyElement> = plan
            .shaped
            .columns
            .iter()
            .enumerate()
            .filter(|(c, _)| plan.visible[*c])
            .map(|(c, column)| {
                let on = marked == Some(c);
                let arrow = if !on { "" } else if ascending { "\u{2191}" } else { "\u{2193}" };
                let (next, direction) = click(marked, ascending, c);
                track(plan.widths[c])
                    .child(
                        h_flex()
                            .id(SharedString::from(format!("ext-sort-{}-{c}", plan.view.panel)))
                            .role(Role::Button)
                            .test_support()
                            .aria_label(column.title.clone())
                            .w_full()
                            .gap(rem(tokens.sp_4))
                            .px(rem(tokens.sp_8))
                            .py(rem(tokens.sp_6))
                            .typed(tokens.type_label)
                            .text_color(if on { fg_hot } else { fg })
                            .cursor_pointer()
                            .whitespace_nowrap()
                            .when(column.numeric, |button| button.justify_end())
                            .hover(move |style| style.text_color(fg_hot))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.sort = Some((next, direction == Direction::Ascending));
                                cx.notify();
                            }))
                            .child(SharedString::from(column.title.to_uppercase()))
                            .child(div().min_w(rem(tokens.sp_8)).child(arrow)),
                    )
                    .into_any_element()
            })
            .collect();
        h_flex()
            .id(SharedString::from(format!("ext-head-{}", plan.view.panel)))
            .border_b_1()
            .border_color(tokens.line)
            .children(cells)
            .into_any_element()
    }
}
