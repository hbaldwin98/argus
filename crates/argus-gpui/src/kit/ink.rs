//! What the drawn blocks paint with: a drawing in its own units mapped to the window's pixels, and
//! the strokes, fills, dots and text painted in them. Charts, sequence diagrams and graphs are all
//! numbers from core and these few calls.
//!
//! A drawing is scaled to its box as the page's SVG `viewBox` is, so a position is in the drawing's
//! units and scales with it, and a stroke that must not (a chart's line) is asked for in the page's
//! pixels, which follow the zoom only. Text is placed by its baseline, as SVG places it.

use gpui_kit::{
    fill, font, outline, point, px, size, App, BorderStyle, Bounds, Corners, FontStyle, FontWeight, Hsla, PathBuilder, Pixels, Point, SharedString, TextAlign, TextRun, Window,
};

use crate::theme::scale::zoom;

/// A point of a drawing.
pub type Pt = (f32, f32);

/// How a drawing's units become pixels.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Where the drawing's (0, 0) is on screen.
    pub origin: Point<Pixels>,
    /// The window's real pixels in one unit of the drawing.
    pub unit: f32,
    /// The window's real pixels in one of the page's pixels: the zoom.
    pub css: f32,
}

impl Frame {
    /// A drawing whose units are `unit` real pixels each, in a window whose rem size is `rem_size`.
    pub fn new(origin: Point<Pixels>, unit: f32, rem_size: Pixels) -> Self {
        Self { origin, unit, css: zoom(rem_size) }
    }

    /// The screen's point for a point of the drawing.
    pub fn at(&self, p: Pt) -> Point<Pixels> {
        point(self.origin.x + px(p.0 * self.unit), self.origin.y + px(p.1 * self.unit))
    }

    /// A length of the drawing in pixels.
    pub fn len(&self, units: f32) -> Pixels {
        px(units * self.unit)
    }

    /// A length of the page's pixels, which the drawing's scale does not touch.
    pub fn fixed(&self, css: f32) -> Pixels {
        px(css * self.css)
    }

    /// The bounds of a box of the drawing.
    pub fn bounds(&self, x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds { origin: self.at((x, y)), size: size(self.len(w), self.len(h)) }
    }
}

/// A line along `points`, `width` thick, in `colour`; `dash` is the lengths of its on and off pieces
/// in pixels, empty for solid. Fewer than two points draw nothing.
pub fn stroke(window: &mut Window, frame: &Frame, points: &[Pt], width: Pixels, colour: Hsla, dash: &[Pixels]) {
    let [first, rest @ ..] = points else { return };
    if rest.is_empty() {
        return;
    }
    let mut path = PathBuilder::stroke(width);
    if !dash.is_empty() && dash.iter().all(|d| f32::from(*d) > 0.0) {
        path = path.dash_array(dash);
    }
    path.move_to(frame.at(*first));
    for p in rest {
        path.line_to(frame.at(*p));
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, colour);
    }
}

/// A closed shape through `points`, filled.
pub fn polygon(window: &mut Window, frame: &Frame, points: &[Pt], colour: Hsla) {
    if points.len() < 3 {
        return;
    }
    let mut path = PathBuilder::fill();
    path.add_polygon(&points.iter().map(|p| frame.at(*p)).collect::<Vec<_>>(), true);
    if let Ok(path) = path.build() {
        window.paint_path(path, colour);
    }
}

/// A circle at `centre` of radius `r` (units), filled and ringed.
pub fn circle(window: &mut Window, frame: &Frame, centre: Pt, r: f32, fill_colour: Option<Hsla>, ring: Option<(Pixels, Hsla)>) {
    let bounds = frame.bounds(centre.0 - r, centre.1 - r, r * 2.0, r * 2.0);
    let radii = Corners::all(frame.len(r));
    if let Some(colour) = fill_colour {
        window.paint_quad(fill(bounds, colour).corner_radii(radii));
    }
    if let Some((width, colour)) = ring {
        let mut ring = outline(bounds, colour, BorderStyle::Solid).corner_radii(radii);
        ring.border_widths = gpui_kit::Edges::all(width);
        window.paint_quad(ring);
    }
}

/// A box with rounded corners, filled and edged.
pub fn boxed(window: &mut Window, frame: &Frame, at: (f32, f32, f32, f32), radius: f32, fill_colour: Option<Hsla>, edge: Option<(Pixels, Hsla)>) {
    let bounds = frame.bounds(at.0, at.1, at.2, at.3);
    let radii = Corners::all(frame.len(radius));
    if let Some(colour) = fill_colour {
        window.paint_quad(fill(bounds, colour).corner_radii(radii));
    }
    if let Some((width, colour)) = edge {
        let mut edged = outline(bounds, colour, BorderStyle::Solid).corner_radii(radii);
        edged.border_widths = gpui_kit::Edges::all(width);
        window.paint_quad(edged);
    }
}

/// The three corners of a filled arrowhead whose tip is at `tip` and which points away from `from`:
/// `len` long and `half` either side of the line.
pub fn head(tip: Pt, from: Pt, len: f32, half: f32) -> [Pt; 3] {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let norm = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if norm == 0.0 { (1.0, 0.0) } else { (dx / norm, dy / norm) };
    let base = (tip.0 - ux * len, tip.1 - uy * len);
    [tip, (base.0 - uy * half, base.1 + ux * half), (base.0 + uy * half, base.1 - ux * half)]
}

/// Which part of its text a position is: its start, middle or end along the baseline, or its centre,
/// which sets it by the middle of its height and not its baseline (`dominant-baseline: central`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
    Centre,
}

/// What text is painted in.
#[derive(Clone, Copy)]
pub struct Face<'a> {
    pub family: &'a str,
    pub size: Pixels,
    pub weight: f32,
    pub italic: bool,
    pub colour: Hsla,
}

/// `text` with its baseline at `at` and `anchor` at `at`'s x.
pub fn text(window: &mut Window, cx: &mut App, words: &str, at: Point<Pixels>, anchor: Anchor, face: Face) {
    if words.is_empty() {
        return;
    }
    let mut f = font(SharedString::from(face.family.to_owned()));
    f.weight = FontWeight(face.weight);
    if face.italic {
        f.style = FontStyle::Italic;
    }
    let run = TextRun { len: words.len(), font: f, color: face.colour, background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(SharedString::from(words.to_owned()), face.size, &[run], None);
    let (ascent, height) = (line.ascent, line.ascent + line.descent);
    let x = match anchor {
        Anchor::Start => at.x,
        Anchor::Middle | Anchor::Centre => at.x - line.width() / 2.0,
        Anchor::End => at.x - line.width(),
    };
    let top = if anchor == Anchor::Centre { at.y - height / 2.0 } else { at.y - ascent };
    line.paint(point(x, top), height, TextAlign::Left, None, window, cx).ok();
}

/// Where the copies that make a text's halo are, round it, for a halo `reach` wide: eight points on
/// a ring, which is a stroke under the text in the colour of what it sits over.
pub fn halo_offsets(reach: f32) -> [(f32, f32); 8] {
    let d = reach * std::f32::consts::FRAC_1_SQRT_2;
    [(reach, 0.0), (d, d), (0.0, reach), (-d, d), (-reach, 0.0), (-d, -d), (0.0, -reach), (d, -d)]
}

/// `text` with a halo of `ground` round it, `reach` pixels out, so it stays legible over a line.
pub fn haloed(window: &mut Window, cx: &mut App, words: &str, at: Point<Pixels>, anchor: Anchor, face: Face, halo: (Hsla, Pixels)) {
    let (ground, reach) = halo;
    for (dx, dy) in halo_offsets(f32::from(reach)) {
        text(window, cx, words, point(at.x + px(dx), at.y + px(dy)), anchor, Face { colour: ground, ..face });
    }
    text(window, cx, words, at, anchor, face);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_of_the_drawing_lands_scaled_from_its_origin() {
        let frame = Frame { origin: point(px(10.0), px(20.0)), unit: 2.0, css: 1.5 };
        assert_eq!(frame.at((3.0, 4.0)), point(px(16.0), px(28.0)));
        assert_eq!(frame.len(5.0), px(10.0));
    }

    #[test]
    fn a_fixed_stroke_follows_the_zoom_and_not_the_drawings_scale() {
        let small = Frame { origin: point(px(0.0), px(0.0)), unit: 0.5, css: 1.5 };
        let large = Frame { unit: 3.0, ..small };
        assert_eq!(small.fixed(2.0), px(3.0));
        assert_eq!(large.fixed(2.0), px(3.0));
    }

    #[test]
    fn an_arrowhead_has_its_tip_at_the_end_and_its_base_back_along_the_line() {
        let [tip, a, b] = head((10.0, 0.0), (0.0, 0.0), 8.0, 3.0);
        assert_eq!(tip, (10.0, 0.0));
        assert!((a.0 - 2.0).abs() < 1e-5 && (b.0 - 2.0).abs() < 1e-5, "the base is 8 back");
        assert!(((a.1 - b.1).abs() - 6.0).abs() < 1e-5 && a.1 * b.1 < 0.0, "one corner either side");
    }

    #[test]
    fn an_arrowhead_turns_with_the_line() {
        let [tip, a, b] = head((0.0, 10.0), (0.0, 0.0), 8.0, 3.0);
        assert_eq!(tip, (0.0, 10.0));
        assert!((a.1 - 2.0).abs() < 1e-5 && (b.1 - 2.0).abs() < 1e-5);
        assert!(((a.0 - b.0).abs() - 6.0).abs() < 1e-5);
    }

    #[test]
    fn a_halo_is_a_ring_of_copies_all_the_same_distance_out() {
        for (dx, dy) in halo_offsets(2.0) {
            assert!(((dx * dx + dy * dy).sqrt() - 2.0).abs() < 1e-5);
        }
        let all = halo_offsets(2.0);
        assert!(all.iter().any(|o| o.0 > 1.9) && all.iter().any(|o| o.0 < -1.9) && all.iter().any(|o| o.1 > 1.9) && all.iter().any(|o| o.1 < -1.9));
    }

    #[test]
    fn an_arrowhead_with_no_line_to_point_along_points_right() {
        let [tip, a, _] = head((5.0, 5.0), (5.0, 5.0), 4.0, 2.0);
        assert_eq!(tip, (5.0, 5.0));
        assert_eq!(a.0, 1.0);
    }
}
