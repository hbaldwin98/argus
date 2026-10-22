//! A tree block: nested nodes that fold. Which nodes the person opened or folded is the window's
//! (`views/memory`, by the node's place under its view); a node's own `open` only says how it starts.

use argus_protocol::TreeNode;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::pen::Pen;
use crate::kit::icons::Glyph;
use crate::theme::scale::rem;
use crate::views::chip::chip;
use crate::views::ctx::ViewCtx;
use crate::views::opens::opens_mark;
use crate::views::values::value;

pub fn tree(nodes: &[TreeNode], ctx: &ViewCtx, pen: &Pen) -> AnyElement {
    v_flex().children(rows(nodes, ctx, &ctx.path, 0, pen)).into_any_element()
}

fn rows(nodes: &[TreeNode], ctx: &ViewCtx, prefix: &str, depth: usize, pen: &Pen) -> Vec<AnyElement> {
    let tokens = pen.draw.tokens;
    nodes
        .iter()
        .enumerate()
        .map(|(k, node)| {
            let key = format!("{prefix}/{k}");
            let branch = !node.children.is_empty();
            let open = branch && pen.memory.borrow().open(&key, node.open);
            let chevron = if branch {
                let (memory, at, repaint, fill) = (pen.memory.clone(), key.clone(), pen.repaint, tokens.fill);
                div()
                    .id(SharedString::from(format!("{key}-fold")))
                    .role(Role::Button)
                    .test_support()
                    .aria_label(node.label.clone())
                    .aria_expanded(open)
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .size(rem(tokens.icon_box))
                    .rounded(rem(tokens.r_xs))
                    .text_color(tokens.fg_muted)
                    .cursor_pointer()
                    .hover(move |style| style.bg(fill))
                    .on_click(move |_, _, cx| {
                        memory.borrow_mut().set_open(&at, !open);
                        cx.notify(repaint);
                    })
                    .child((if open { Glyph::Down } else { Glyph::Right }).at(tokens.icon_xs))
                    .into_any_element()
            } else {
                div().flex_shrink_0().size(rem(tokens.icon_box)).into_any_element()
            };
            let head = h_flex()
                .min_h(rem(tokens.row_h))
                .gap(rem(tokens.sp_8))
                .pl(rem(depth as f32 * tokens.sp_16))
                .child(chevron)
                .child(div().min_w_0().child(SharedString::from(node.label.clone())))
                .children(opens_mark(node.opens.as_deref(), &key, ctx, pen))
                .children(node.value.as_ref().map(|v| div().text_color(tokens.fg_muted).child(value(v, &format!("{key}-value"), &pen.at(ctx)))))
                .children(node.chip.as_ref().map(|c| div().ml_auto().flex_shrink_0().child(chip(c, tokens))));
            v_flex().child(head).children(open.then(|| v_flex().children(rows(&node.children, ctx, &key, depth + 1, pen)))).into_any_element()
        })
        .collect()
}
