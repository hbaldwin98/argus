//! What says a column is off to the right: a fade at the edge and a chip naming the columns cut off
//! (`withScrollCue` of `feature_stage.ts`).
//!
//! A column counts as cut off as soon as any part of it lies right of the lanes' edge by more than a
//! pixel, since the half read under the fade is the one a person needs to be told about. The chip
//! shows only while there is more to scroll to and at least one column is cut; pressing it scrolls
//! the first cut column into view with a tenth of the lanes' width to spare, clear of the fade. The
//! page measures after a layout and on every scroll; this measures as it is drawn and asks to be
//! drawn again when what it measured is not what it drew.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::board::index_words::FeatureColumn;
use gpui_kit::component::h_flex;
use gpui_kit::{canvas, div, linear_color_stop, linear_gradient, prelude::*, px, AnyElement, Bounds, Context, Entity, Pixels, ScrollHandle, SharedString, TestSupportExt as _};

use super::Lanes;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What is cut off now.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cue {
    /// Whether the lanes can be scrolled further right (the fade).
    pub more: bool,
    /// The columns cut off, in the order the page draws them.
    pub cut: Vec<FeatureColumn>,
}

/// The chip's words: how many are cut off, and which.
pub fn words(cut: &[FeatureColumn]) -> (String, String) {
    (format!("{} more", cut.len()), cut.iter().map(|column| column.label()).collect::<Vec<_>>().join(" \u{b7} "))
}

/// What is cut off of `view`, the lanes' box, given every column's box and where the lanes are scrolled.
/// The kit counts the lanes' own padding into what scrolls (`max_offset` is 32 px with everything in view), so
/// `padding` is what to take off it, both sides of it.
pub fn of(view: &Bounds<Pixels>, boxes: &[(FeatureColumn, Bounds<Pixels>)], scroll: &ScrollHandle, zoom: f32, padding: Pixels) -> Cue {
    let edge = view.right();
    let cut = super::DRAWN.iter().copied().filter(|column| boxes.iter().any(|(held, bounds)| held == column && bounds.right() > edge + px(zoom))).collect();
    let rest = scroll.max_offset().x + scroll.offset().x - padding;
    Cue { more: rest > px(4.0 * zoom), cut }
}

/// `.board-fade`: the sunken colour coming in over the last stretch, while there is more.
pub fn fade(cue: &Cue, tokens: &Tokens) -> impl IntoElement {
    let sunken = tokens.bg_sunken;
    div()
        .id("feature-board-fade")
        .test_support()
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(rem(tokens.fx_fade_w))
        .opacity(if cue.more { 1.0 } else { 0.0 })
        .bg(linear_gradient(90.0, linear_color_stop(sunken.opacity(0.0), 0.0), linear_color_stop(sunken, 0.7)))
}

/// `.board-more`: the chip, when something is cut off and there is more to reach.
pub fn chip(cue: &Cue, scroll: ScrollHandle, view: Bounds<Pixels>, tokens: &Tokens, cx: &mut Context<Lanes>) -> Option<AnyElement> {
    if !cue.more || cue.cut.is_empty() {
        return None;
    }
    let (count, names) = words(&cue.cut);
    let (bounds, first) = (view, cue.cut[0]);
    let this = cx.entity();
    let hover = tokens.bg_selected;
    Some(
        h_flex()
            .id("feature-board-more")
            .test_support()
            .aria_label(SharedString::from(format!("{count} {names}")))
            .absolute()
            .bottom(rem(tokens.sp_16))
            .right(rem(tokens.sp_12))
            .h(rem(tokens.control_h))
            .px(rem(tokens.sp_12))
            .gap(rem(tokens.sp_8))
            .items_center()
            .border_1()
            .border_color(tokens.line_strong)
            .rounded(rem(tokens.r_pill))
            .bg(tokens.bg_raised)
            .text_color(tokens.fg)
            .text_size(rem(tokens.fs_sm))
            .shadow(tokens.shadow_pop())
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(move |_, window, cx| {
                this.update(cx, |lanes, cx| {
                    lanes.reveal(first, &scroll, &bounds, window.rem_size());
                    cx.notify();
                });
            })
            .child(div().weighted(tokens.weight_bold).child(count))
            .child(div().text_color(tokens.fg_muted).child(names))
            .child(Glyph::Right.at(tokens.icon_xs))
            .into_any_element(),
    )
}

/// Asks the lanes to be drawn again when, after they were laid out, what is cut off is not what
/// was drawn: the boxes are measured as they are laid out, so the first frame has none.
pub fn stale(view: Rc<Cell<Bounds<Pixels>>>, boxes: super::Boxes<FeatureColumn>, scroll: ScrollHandle, zoom: f32, padding: Pixels, drawn: Cue, this: Entity<Lanes>) -> impl IntoElement {
    canvas(
        move |_, _, cx| {
            if of(&view.get(), &boxes.borrow(), &scroll, zoom, padding) != drawn {
                let this = this.clone();
                cx.defer(move |cx| this.update(cx, |_, cx| cx.notify()));
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}

/// How far to scroll so a column whose right edge is at `next` clears the fade: its overshoot past
/// the lanes' `edge`, and a tenth of their `width` more.
pub fn clear(next: Pixels, edge: Pixels, width: Pixels) -> Pixels {
    next - edge + width * 0.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chip_says_how_many_are_cut_off_and_names_them_in_the_order_they_are_drawn() {
        assert_eq!(words(&[FeatureColumn::Held, FeatureColumn::Archived]), ("2 more".to_owned(), "Held \u{b7} Archived".to_owned()));
        assert_eq!(words(&[FeatureColumn::Done]), ("1 more".to_owned(), "Done".to_owned()));
    }

    #[test]
    fn a_column_clears_the_fade_by_its_overshoot_and_a_tenth_of_the_lanes() {
        assert_eq!(clear(px(1000.0), px(900.0), px(800.0)), px(180.0));
    }

    #[test]
    fn nothing_is_cut_off_while_every_column_is_inside_the_lanes() {
        let view = Bounds::new(gpui_kit::point(px(0.0), px(0.0)), gpui_kit::size(px(1000.0), px(500.0)));
        let inside = |x: f32| Bounds::new(gpui_kit::point(px(x), px(0.0)), gpui_kit::size(px(200.0), px(500.0)));
        let boxes = [(FeatureColumn::Queued, inside(0.0)), (FeatureColumn::Held, inside(800.0))];
        assert_eq!(of(&view, &boxes, &ScrollHandle::new(), 1.0, px(0.0)).cut, []);
        let cut = [(FeatureColumn::Queued, inside(0.0)), (FeatureColumn::Held, inside(802.0))];
        assert_eq!(of(&view, &cut, &ScrollHandle::new(), 1.0, px(0.0)).cut, [FeatureColumn::Held], "more than a pixel past the edge");
        let one_pixel = [(FeatureColumn::Held, inside(801.0))];
        assert_eq!(of(&view, &one_pixel, &ScrollHandle::new(), 1.0, px(0.0)).cut, [], "a pixel is not cut off");
    }
}
