//! One line of text longer than its box, scrolled to its first mark and faded where it is cut: the
//! page's `.hline .tx` and `cropHits` (`code/stage.ts`). GPUI has no mask, so the cut edge is
//! covered by a gradient from the row's own colour.

use std::ops::Range;

use crate::theme::scale::pixels;
use gpui_kit::{canvas, fill, font, linear_color_stop, linear_gradient, point, px, size, Bounds, Canvas, ContentMask, FontWeight, Hsla, SharedString, TextAlign, TextRun};

/// How the line looks, all of it from the token space.
#[derive(Clone)]
pub struct Look {
    pub family: SharedString,
    pub size: f32,
    pub line_height: f32,
    pub colour: Hsla,
    pub mark_colour: Hsla,
    pub mark_background: Hsla,
    /// The colour behind the line, which the fade runs into.
    pub ground: Hsla,
    /// How much of the line shows before the first mark, and how wide the fade is. Every length is the page's pixels.
    pub lead: f32,
    pub fade: f32,
}

/// How far a line is scrolled so its first mark stands `lead` from the left, never past where its
/// end meets the box's, and which edges are then cut. A line that fits is not scrolled.
pub fn scroll(mark_x: Option<f32>, text: f32, within: f32, lead: f32) -> (f32, bool, bool) {
    if text <= within {
        return (0.0, false, false);
    }
    let most = text - within;
    let at = mark_x.map_or(0.0, |x| (x - lead).max(0.0)).min(most);
    (at, at > 0.0, most - at > 1.0)
}

/// The line `text` with `marks` (byte ranges) lit, to stand in a box as wide as its parent gives.
pub fn crop(text: SharedString, marks: Vec<Range<usize>>, look: Look) -> Canvas<()> {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            let mut face = font(look.family.clone());
            face.weight = FontWeight(400.0);
            let run = |len: usize, lit: bool| TextRun {
                len,
                font: face.clone(),
                color: if lit { look.mark_colour } else { look.colour },
                background_color: lit.then_some(look.mark_background),
                underline: None,
                strikethrough: None,
            };
            let mut runs = Vec::new();
            let mut at = 0;
            for mark in &marks {
                if mark.end > text.len() || mark.start < at {
                    continue;
                }
                if mark.start > at {
                    runs.push(run(mark.start - at, false));
                }
                runs.push(run(mark.end - mark.start, true));
                at = mark.end;
            }
            if at < text.len() {
                runs.push(run(text.len() - at, false));
            }
            let rem_size = window.rem_size();
            let (font_size, line_height, lead, fade) = (pixels(look.size, rem_size), pixels(look.line_height, rem_size), pixels(look.lead, rem_size), pixels(look.fade, rem_size));
            let line = window.text_system().shape_line(text.clone(), font_size, &runs, None);
            let first = marks.first().map(|mark| f32::from(line.x_for_index(mark.start)));
            let (left, cut_left, cut_right) = scroll(first, f32::from(line.width()), f32::from(bounds.size.width), f32::from(lead));
            let origin = point(bounds.origin.x - px(left), bounds.origin.y);
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                line.paint_background(origin, line_height, TextAlign::Left, None, window, cx).ok();
                line.paint(origin, line_height, TextAlign::Left, None, window, cx).ok();
                let edge = |x, to_clear| {
                    let (from, to) = if to_clear { (look.ground, look.ground.opacity(0.0)) } else { (look.ground.opacity(0.0), look.ground) };
                    fill(Bounds { origin: point(x, bounds.origin.y), size: size(fade, bounds.size.height) }, linear_gradient(90.0, linear_color_stop(from, 0.0), linear_color_stop(to, 1.0)))
                };
                if cut_left {
                    window.paint_quad(edge(bounds.origin.x, true));
                }
                if cut_right {
                    window.paint_quad(edge(bounds.origin.x + bounds.size.width - fade, false));
                }
            });
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CODE-30: a line that fits stays put and is cut nowhere.
    #[test]
    fn a_line_that_fits_is_not_scrolled() {
        assert_eq!(scroll(Some(300.0), 200.0, 200.0, 48.0), (0.0, false, false));
    }

    /// CODE-30: the first mark stands 48px in, and both edges are cut when there is more each way.
    #[test]
    fn the_first_mark_stands_a_lead_from_the_left() {
        assert_eq!(scroll(Some(300.0), 900.0, 200.0, 48.0), (252.0, true, true));
    }

    /// CODE-30: a mark near the start scrolls nothing, and one near the end stops at the end.
    #[test]
    fn the_scroll_stays_within_the_line() {
        assert_eq!(scroll(Some(20.0), 900.0, 200.0, 48.0), (0.0, false, true));
        assert_eq!(scroll(Some(880.0), 900.0, 200.0, 48.0), (700.0, true, false));
        assert_eq!(scroll(None, 900.0, 200.0, 48.0), (0.0, false, true));
    }
}
