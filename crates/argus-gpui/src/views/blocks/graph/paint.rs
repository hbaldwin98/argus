//! A graph's arrows: each edge's right-angled route with its head, and its label at the middle of
//! the route in a halo of the box's own colour so it reads over a line (GRAPH-2).

use std::rc::Rc;

use argus_desktop_core::views::blocks::graph::layout::{Layout, MAX_LABEL};
use argus_desktop_core::views::blocks::label::clip;
use gpui_kit::{canvas, prelude::*, App, Bounds, Pixels, Window};

use crate::kit::ink::{haloed, head, polygon, stroke, Anchor, Face, Frame};
use crate::theme::Tokens;

/// The canvas that paints `laid`'s routes, as big as the element it is the child of. `labels` is
/// each edge's label, in the edges' order.
pub fn arrows(laid: Rc<Layout>, labels: Vec<Option<String>>, tokens: &Tokens) -> impl IntoElement {
    let tokens = *tokens;
    canvas(
        |_, _, _| (),
        move |area: Bounds<Pixels>, _, window: &mut Window, cx: &mut App| {
            let frame = Frame::new(area.origin, f32::from(area.size.width) / laid.width.max(1.0), window.rem_size());
            for (route, label) in laid.routes.iter().zip(&labels) {
                let Some(route) = route else { continue };
                stroke(window, &frame, &route.points, frame.len(tokens.vg_edge_stroke), tokens.fg_muted, &[]);
                // The page's marker is `markerWidth` strokes wide, with its tip on the line's end.
                if let [.., from, tip] = route.points.as_slice() {
                    let len = tokens.vg_head * tokens.vg_edge_stroke;
                    polygon(window, &frame, &head(*tip, *from, len, len * 0.4), tokens.fg_muted);
                }
                if let Some(label) = label {
                    let face = Face { family: tokens.font_ui, size: frame.len(tokens.vg_label_font), weight: tokens.weight_regular, italic: false, colour: tokens.fg_muted };
                    haloed(window, cx, &clip(label, MAX_LABEL), frame.at(route.mid), Anchor::Centre, face, (tokens.bg_sunken, frame.len(tokens.vg_halo / 2.0)));
                }
            }
        },
    )
    .absolute()
    .size_full()
}
