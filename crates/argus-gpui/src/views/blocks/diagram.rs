//! A diagram block: Mermaid's sequence diagram drawn from the lifelines and steps Rust read out of
//! it, in a scrolling box, with its source folded under it (DIAGRAM-1, DIAGRAM-2).
//!
//! The reading is Rust's and arrives with the view (`ViewCtx::diagrams`, found by source); a source
//! it could not read is still what the agent meant, so it is shown as a `mermaid` code block. A view
//! with nothing to draw says so and keeps its source. The painting is `kit/sequence`'s.
//!
//! The drawing opens larger (`figure`), with its source beside it on request.

use std::rc::Rc;

use argus_desktop_core::connection::page::PageDiagram;
use argus_desktop_core::views::blocks::diagram::{layout, title};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::code::code;
use super::figure::{expandable, Figure};
use super::pen::Pen;
use crate::kit::icons::Glyph;
use crate::kit::sequence::sequence;
use crate::kit::wheel::Wheel;
use crate::theme::scale::rem;
use crate::views::ctx::ViewCtx;

pub fn diagram(source: &str, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let key = format!("{}-diagram", ctx.path);
    let Some(read) = ctx.diagrams.iter().find(|d| d.source == source) else {
        return code(Some("mermaid"), source, &ctx.path, pen);
    };
    let tokens = pen.draw.tokens;
    let drawn = match layout(read) {
        Some(drawn) => {
            let scroll = pen.scroll(&format!("{key}-scroll"));
            let wheel = Wheel::new(&scroll);
            let drawn = Rc::new(drawn);
            let larger = {
                let (laid, tokens) = (drawn.clone(), *tokens);
                Rc::new(move |scale: f32| div().child(sequence(laid.clone(), scale, &tokens)).into_any_element())
            };
            let figure = Figure { title: title(read), source: Some(read.source.clone()), size: (drawn.width, drawn.height), draw: larger };
            let inline = div()
                .id(SharedString::from(format!("{key}-scroll")))
                .role(Role::Image)
                .test_support()
                .aria_label(drawn.summary.clone())
                .overflow_x_scroll()
                .track_scroll(&scroll)
                .on_scroll_wheel(wheel.keep())
                .child(wheel.probe())
                .bg(tokens.bg_sunken)
                .border_1()
                .border_color(tokens.line)
                .rounded(rem(tokens.r_sm))
                .child(div().flex_shrink_0().child(sequence(drawn, 1.0, tokens)))
                .into_any_element();
            expandable(inline, figure, &key, tokens)
        }
        None => div().id(SharedString::from(format!("{key}-empty"))).test_support().text_color(tokens.fg_muted).text_size(rem(tokens.fs_sm)).child("Nothing to draw").into_any_element(),
    };
    v_flex().min_w_0().gap(rem(tokens.sp_6)).child(drawn).child(source_fold(read, &key, ctx, pen)).into_any_element()
}

/// `.vd-source`: a `Source` summary that opens to the source as it was written. Open or closed as the
/// person last left it.
fn source_fold(read: &PageDiagram, key: &str, ctx: &ViewCtx, pen: &Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let fold = format!("{}|source", ctx.path);
    let open = pen.memory.borrow().open(&fold, false);
    let (memory, repaint) = (pen.memory.clone(), pen.repaint);
    let summary = h_flex()
        .id(SharedString::from(format!("{key}-source")))
        .role(Role::Button)
        .test_support()
        .aria_label("Source")
        .gap(rem(tokens.sp_4))
        .cursor_pointer()
        .text_color(tokens.fg_faint)
        .text_size(rem(tokens.fs_sm))
        .on_click(move |_, _, cx| {
            memory.borrow_mut().set_open(&fold, !open);
            cx.notify(repaint);
        })
        .child(if open { Glyph::Down } else { Glyph::Right }.at(tokens.icon_sm))
        .child("Source");
    let body = open.then(|| {
        div()
            .id(SharedString::from(format!("{key}-source-text")))
            .test_support()
            .mt(rem(tokens.sp_6))
            .p(rem(tokens.sp_6))
            .overflow_x_hidden()
            .bg(tokens.bg_sunken)
            .rounded(rem(tokens.r_sm))
            .text_color(tokens.fg_muted)
            .font_family(tokens.font_mono)
            .text_size(rem(tokens.fs_sm))
            .whitespace_nowrap()
            .child(SharedString::from(read.source.clone()))
    });
    v_flex().child(summary).children(body).into_any_element()
}
