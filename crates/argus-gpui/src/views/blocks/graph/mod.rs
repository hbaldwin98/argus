//! A graph block: boxes and arrows laid out by core (`views::blocks::graph`), in a framed box that
//! scales the drawing down to its panel and never up past the size it was laid out at (GRAPH-1 to
//! GRAPH-4).
//!
//! The arrows are painted (`paint`); each box is an element, so a box that opens a saved view is
//! pressed where it is, a clipped label has its whole text for a tooltip and a reader that cannot
//! see is told what each box says. Every node that opens a view is also a link in a row under the
//! graph, which is the keyboard's way in.
//!
//! The drawing opens larger (`figure`), drawn again at the scale it is zoomed to.

pub mod paint;

use std::rc::Rc;

use argus_desktop_core::views::blocks::graph::describe::{describe, title};
use argus_desktop_core::views::blocks::graph::layout::{layout, Layout, MAX_LABEL};
use argus_desktop_core::views::blocks::label::clip;
use argus_protocol::{GraphDirection, GraphEdge, GraphNode};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::figure::{expandable, Figure};
use super::pen::Pen;
use crate::connection::caller::Caller;
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;
use crate::theme::Tokens;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::chip::tone_colour;
use crate::views::ctx::ViewCtx;
use crate::views::opens::{open_call, opens_mark};

pub fn graph(nodes: &[GraphNode], edges: &[GraphEdge], direction: GraphDirection, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}-graph", ctx.path);
    let frame = |inner: AnyElement| {
        div().id(SharedString::from(format!("{key}-frame"))).test_support().min_w_0().overflow_hidden().bg(tokens.bg_sunken).border_1().border_color(tokens.line).rounded(rem(tokens.r_sm)).child(inner)
    };
    if nodes.is_empty() {
        return frame(div().p(rem(tokens.sp_12)).text_color(tokens.fg_faint).typed(tokens.type_small).child("Nothing to draw").into_any_element()).into_any_element();
    }
    let laid = Rc::new(layout(nodes, edges, direction == GraphDirection::Right));
    // The drawing is scaled down to the panel, never up past the size it was laid out at.
    let room = pen.width(&format!("{key}|box"));
    let scale = if room > 0.0 { (room / laid.width).min(1.0) } else { 1.0 };
    let inline = drawing(nodes, edges, &laid, scale, &key, ctx, (pen.draw.caller, tokens));
    // Opened larger it is drawn from the same layout at the scale it is zoomed to, as its own boxes.
    let larger = {
        let (nodes, edges, laid, key, ctx, caller, tokens) = (nodes.to_vec(), edges.to_vec(), laid.clone(), format!("{key}-larger"), ctx.clone(), pen.draw.caller.clone(), *tokens);
        Rc::new(move |scale: f32| drawing(&nodes, &edges, &laid, scale, &key, &ctx, (&caller, &tokens)))
    };
    let figure = Figure { title: title(nodes.len()), source: None, size: (laid.width, laid.height), draw: larger };
    let links: Vec<AnyElement> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.opens.is_some())
        .map(|(i, n)| {
            h_flex()
                .child(SharedString::from(n.label.clone()))
                .children(opens_mark(n.opens.as_deref(), &format!("{key}-link{i}"), ctx, pen))
                .into_any_element()
        })
        .collect();
    v_flex()
        .id(SharedString::from(key.clone()))
        .role(Role::Image)
        .test_support()
        .aria_label(describe(nodes, edges))
        .relative()
        .min_w_0()
        .child(pen.watch(format!("{key}|box")))
        .child(expandable(frame(h_flex().justify_center().child(inline).into_any_element()).into_any_element(), figure, &key, tokens))
        .children((!links.is_empty()).then(|| {
            h_flex().flex_wrap().gap_x(rem(tokens.sp_12)).gap_y(rem(tokens.sp_4)).mt(rem(tokens.sp_6)).text_size(rem(tokens.fs_sm)).children(links)
        }))
        .into_any_element()
}

/// The laid-out drawing at `scale`: the arrows painted, the boxes over them.
fn drawing(nodes: &[GraphNode], edges: &[GraphEdge], laid: &Rc<Layout>, scale: f32, key: &str, ctx: &ViewCtx, (caller, tokens): (&Caller, &Tokens)) -> AnyElement {
    let unit = |v: f32| rem(v * scale);
    let labels: Vec<Option<String>> = edges.iter().map(|e| e.label.clone().filter(|l| !l.is_empty())).collect();
    let boxes = nodes.iter().zip(&laid.nodes).enumerate().map(|(i, (node, at))| {
        let shown = clip(&node.label, MAX_LABEL);
        let colour = tone_colour(node.tone, tokens);
        let cut = shown != node.label;
        let opens = node.opens.as_deref().map(|id| (caller.clone(), open_call(id, ctx)));
        div()
            .id(SharedString::from(format!("{key}-node{i}")))
            .test_support()
            .aria_label(node.label.clone())
            .absolute()
            .left(unit(at.x))
            .top(unit(at.y))
            .w(unit(at.w))
            .h(unit(at.h))
            .flex()
            .items_center()
            .justify_center()
            .rounded(unit(tokens.vg_radius))
            .border(unit(tokens.vg_stroke))
            .border_color(colour)
            .bg(tokens.bg_surface)
            .text_color(tokens.fg)
            .text_size(unit(tokens.vg_node_font))
            .whitespace_nowrap()
            .when(cut, |node_box| node_box.tooltip(tip(node.label.clone())))
            .when_some(opens, |node_box, (caller, asked)| {
                node_box.cursor_pointer().on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    caller.call(asked.clone());
                    // A box pressed in the larger view opened a view behind it: show it (FIGURE-5).
                    Overlays::get(cx).close(Overlay::Figure, window, cx);
                })
            })
            .child(SharedString::from(shown))
            .into_any_element()
    });
    div()
        .relative()
        .flex_shrink_0()
        .w(unit(laid.width))
        .h(unit(laid.height))
        .child(paint::arrows(laid.clone(), labels, tokens))
        .children(boxes)
        .into_any_element()
}
