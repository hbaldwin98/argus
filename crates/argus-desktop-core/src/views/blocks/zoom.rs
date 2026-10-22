//! Where a drawing sits in the larger view it opens in: how big it starts, and how a wheel, a button
//! or a drag moves it (FIGURE-3, FIGURE-4). Only numbers, so both windows and a test can hold it.
//!
//! The page's `views/blocks/zoom.ts`, with its test.

/// The drawing's scale, and where its top-left corner sits in the stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom {
    pub scale: f32,
    pub x: f32,
    pub y: f32,
}

pub const MIN_SCALE: f32 = 0.1;
pub const MAX_SCALE: f32 = 6.0;
/// What `+` and `-` and the buttons multiply the scale by, about the stage's centre.
pub const STEP: f32 = 1.25;
const MARGIN: f32 = 24.0;
/// Below this a label is too small to read, so a drawing tall enough to need it starts at the
/// stage's width instead and is dragged down.
const READABLE: f32 = 0.75;
/// A small drawing is enlarged to fill the stage, but only so far.
const MAX_START: f32 = 2.0;
/// How much a wheel notch zooms: the scale is multiplied by `exp(-delta * lines * WHEEL)`.
const WHEEL: f32 = 0.0015;
/// What a wheel counts a line as, in the page's pixels.
const LINE: f32 = 16.0;

fn clamp(scale: f32) -> f32 {
    scale.clamp(MIN_SCALE, MAX_SCALE)
}

/// The whole drawing in the stage, centred.
pub fn fit(w: f32, h: f32, stage_w: f32, stage_h: f32) -> Zoom {
    let scale = clamp(((stage_w - 2.0 * MARGIN) / w).min((stage_h - 2.0 * MARGIN) / h).min(MAX_START));
    Zoom { scale, x: (stage_w - w * scale) / 2.0, y: (stage_h - h * scale) / 2.0 }
}

/// How a drawing opens: whole when it can be read whole, else across the stage from its top, since a
/// long sequence is read downward.
pub fn start(w: f32, h: f32, stage_w: f32, stage_h: f32) -> Zoom {
    let whole = fit(w, h, stage_w, stage_h);
    if whole.scale >= READABLE {
        return whole;
    }
    let scale = clamp(((stage_w - 2.0 * MARGIN) / w).min(1.0));
    if scale <= whole.scale {
        return whole;
    }
    Zoom { scale, x: (stage_w - w * scale) / 2.0, y: MARGIN }
}

/// Scaled by `factor` about the stage point (`px`, `py`), which stays put under the pointer.
pub fn zoom_at(z: Zoom, factor: f32, px: f32, py: f32) -> Zoom {
    let scale = clamp(z.scale * factor);
    let k = scale / z.scale;
    Zoom { scale, x: px - (px - z.x) * k, y: py - (py - z.y) * k }
}

/// Scaled to 100% about `(px, py)`.
pub fn actual(z: Zoom, px: f32, py: f32) -> Zoom {
    zoom_at(z, 1.0 / z.scale, px, py)
}

/// What a wheel turn of `delta_y` zooms by, as the page reads one: `delta_y` is positive for a turn
/// toward the person (zoom out), counted in `unit`; a line is 16 pixels and a page is the stage's
/// height.
pub fn wheel_factor(delta_y: f32, unit: Unit) -> f32 {
    let lines = match unit {
        Unit::Pixels => 1.0,
        Unit::Lines => LINE,
        Unit::Page(height) => height,
    };
    (-delta_y * lines * WHEEL).exp()
}

/// What a wheel's delta is counted in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Pixels,
    Lines,
    /// Pages, each as tall as the stage.
    Page(f32),
}

/// The percent label: the scale, rounded.
pub fn percent(z: Zoom) -> String {
    format!("{}%", (z.scale * 100.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    // FIGURE-3: `zoom.test.mjs`, 'a fitted drawing is centred and inside the stage'.
    #[test]
    fn a_fitted_drawing_is_centred_and_inside_the_stage() {
        let z = fit(1000.0, 400.0, 800.0, 600.0);
        assert!(1000.0 * z.scale <= 800.0 && 400.0 * z.scale <= 600.0);
        assert!(near(z.x * 2.0 + 1000.0 * z.scale, 800.0));
        assert!(near(z.y * 2.0 + 400.0 * z.scale, 600.0));
    }

    // FIGURE-3: 'a long sequence opens across the stage from its top rather than shrunk whole'.
    #[test]
    fn a_long_sequence_opens_across_the_stage_from_its_top_rather_than_shrunk_whole() {
        let z = start(900.0, 4000.0, 1200.0, 800.0);
        assert!(z.scale > fit(900.0, 4000.0, 1200.0, 800.0).scale);
        assert!(z.y > 0.0 && z.y < 100.0, "starts at the top");
    }

    #[test]
    fn a_drawing_readable_whole_opens_whole() {
        assert_eq!(start(600.0, 400.0, 1200.0, 800.0), fit(600.0, 400.0, 1200.0, 800.0));
    }

    #[test]
    fn a_small_drawing_is_enlarged_to_fill_the_stage_but_only_twice() {
        assert_eq!(fit(50.0, 40.0, 1200.0, 800.0).scale, 2.0);
    }

    // FIGURE-4: 'the point under the wheel stays put while zooming, and the scale is bounded'.
    #[test]
    fn the_point_under_the_wheel_stays_put_while_zooming_and_the_scale_is_bounded() {
        for factor in [0.5, 0.9, 1.1, 3.0] {
            let z = Zoom { scale: 0.8, x: 40.0, y: -120.0 };
            let (px, py) = (300.0, 250.0);
            let before = ((px - z.x) / z.scale, (py - z.y) / z.scale);
            let after = zoom_at(z, factor, px, py);
            assert!(near((px - after.x) / after.scale, before.0));
            assert!(near((py - after.y) / after.scale, before.1));
        }
        assert_eq!(zoom_at(Zoom { scale: 1.0, x: 0.0, y: 0.0 }, 1000.0, 0.0, 0.0).scale, MAX_SCALE);
        assert_eq!(zoom_at(Zoom { scale: 1.0, x: 0.0, y: 0.0 }, 0.0001, 0.0, 0.0).scale, MIN_SCALE);
    }

    #[test]
    fn the_percent_button_resets_to_100_about_the_point_and_says_the_rounded_scale() {
        let z = Zoom { scale: 2.5, x: 10.0, y: 20.0 };
        let back = actual(z, 100.0, 100.0);
        assert!(near(back.scale, 1.0));
        assert!(near((100.0 - back.x) / back.scale, (100.0 - z.x) / z.scale), "the point under the centre stays put");
        assert_eq!(percent(back), "100%");
        assert_eq!(percent(Zoom { scale: 1.2549, ..z }), "125%");
    }

    // FIGURE-4: the wheel zooms by exp(-deltaY * lines * 0.0015); a line is 16, a page the stage's height.
    #[test]
    fn a_wheel_turn_zooms_by_its_distance_in_pixels_lines_or_pages() {
        assert!(near(wheel_factor(100.0, Unit::Pixels), (-0.15f32).exp()));
        assert!(near(wheel_factor(-3.0, Unit::Lines), (3.0f32 * 16.0 * 0.0015).exp()));
        assert!(near(wheel_factor(1.0, Unit::Page(800.0)), (-1.2f32).exp()));
        assert_eq!(wheel_factor(0.0, Unit::Pixels), 1.0);
        assert!(wheel_factor(-50.0, Unit::Pixels) > 1.0 && wheel_factor(50.0, Unit::Pixels) < 1.0, "toward the person zooms out");
    }
}
