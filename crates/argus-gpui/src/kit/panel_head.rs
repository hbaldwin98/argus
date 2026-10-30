//! A panel's head (`project/Kit.dc.html`, "Panel head"): a 44px line with the title, its count
//! muted after it, and an action or the close button at the right.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, ElementId, SharedString};

use super::icon_button::Node;
use crate::style::Typed as _;

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The head of a panel: `title` semibold, `count` regular and muted, then `trailing` pushed to the
/// right. The line is as tall as a file head so the two line up across columns.
pub fn panel_head(id: impl Into<ElementId>, title: impl Into<SharedString>, count: Option<SharedString>, trailing: Vec<AnyElement>, tokens: &Tokens) -> Node {
    let title = title.into();
    h_flex()
        .id(id)
        .test_support()
        .aria_label(title.clone())
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(rem(tokens.sp_44))
        .pl(rem(14.0))
        .pr(rem(tokens.sp_8))
        .gap(rem(tokens.sp_8))
        .items_center()
        .border_b_1()
        .border_color(tokens.line)
        .text_size(rem(tokens.fs_md))
        .whitespace_nowrap()
        .child(div().min_w_0().truncate().weighted(tokens.weight_semibold).text_color(tokens.fg).child(title))
        .children(count.map(|count| div().flex_none().text_color(tokens.fg_muted).child(count)))
        .child(div().flex_1())
        .child(h_flex().flex_none().gap(rem(tokens.sp_4)).items_center().children(trailing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::{host, icon_button::icon_button};
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn the_head_is_44px_with_its_action_at_the_right_edge(cx: &mut TestAppContext) {
        let handle = host::open(cx, 260.0, |tokens| {
            panel_head("ph", "Commits", Some("main".into()), vec![icon_button("act", "Fold all", false, tokens).into_any_element()], tokens).into_any_element()
        });
        let (left, _, width, height) = host::bounds(cx, handle, "ph");
        let (act_left, _, act_width, _) = host::bounds(cx, handle, "act");
        assert_eq!(height, 44.0);
        assert!((left + width - 8.0 - (act_left + act_width)).abs() < 1.0, "the action sits 8px from the right edge");
    }
}
