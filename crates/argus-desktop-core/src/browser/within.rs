//! A click's point in the box a picture is fitted into, as the page's own
//! pixels: the picture is shown whole, its shape kept and centred
//! (`object-fit: contain`), so the bands either side of it are no part of
//! the page.

/// A rectangle in the window's own units, whatever they are, so long as
/// the click's point is in the same.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box2 {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// A point in the page's CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct At {
    pub x: f64,
    pub y: f64,
}

/// `(x, y)` in `box_`, found in a page of `page` pixels fitted into it;
/// `None` in the bands either side of the picture. The picture's edges are
/// inside (SHELL-BROWSER-13a). A page or box with no area has no point.
pub fn within(box_: Box2, page: (u32, u32), x: f64, y: f64) -> Option<At> {
    let (width, height) = (f64::from(page.0), f64::from(page.1));
    if width <= 0.0 || height <= 0.0 || box_.width <= 0.0 || box_.height <= 0.0 {
        return None;
    }
    let scale = (box_.width / width).min(box_.height / height);
    let left = box_.left + (box_.width - width * scale) / 2.0;
    let top = box_.top + (box_.height - height * scale) / 2.0;
    let at = At { x: (x - left) / scale, y: (y - top) / scale };
    (at.x >= 0.0 && at.y >= 0.0 && at.x <= width && at.y <= height).then_some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: Box2 = Box2 { left: 10.0, top: 20.0, width: 400.0, height: 400.0 };

    #[test]
    fn the_bands_either_side_of_a_wide_picture_are_no_click() {
        // A 1280x800 page in a square box: 400 wide, 250 tall, bands of 75 above and below.
        assert_eq!(within(BOX, (1280, 800), 210.0, 20.0 + 74.0), None);
        assert_eq!(within(BOX, (1280, 800), 210.0, 20.0 + 326.0), None);
        let centre = within(BOX, (1280, 800), 210.0, 220.0).unwrap();
        assert!((centre.x - 640.0).abs() < 1e-9 && (centre.y - 400.0).abs() < 1e-9, "{centre:?}");
    }

    #[test]
    fn the_pictures_edges_are_inside() {
        let corner = within(BOX, (1280, 800), 10.0, 95.0).unwrap();
        assert!(corner.x.abs() < 1e-9 && corner.y.abs() < 1e-9, "{corner:?}");
        assert!(within(BOX, (1280, 800), 410.0, 345.0).is_some());
    }

    #[test]
    fn a_page_with_no_area_has_no_point() {
        assert_eq!(within(BOX, (0, 800), 100.0, 100.0), None);
        assert_eq!(within(Box2 { width: 0.0, ..BOX }, (10, 10), 10.0, 100.0), None);
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // SHELL-BROWSER-13a: a point found is on the page.
            #[test]
            fn a_point_found_is_on_the_page(
                bw in 1.0f64..2000.0, bh in 1.0f64..2000.0, pw in 1u32..4000, ph in 1u32..4000,
                fx in -0.2f64..1.2, fy in -0.2f64..1.2,
            ) {
                let b = Box2 { left: 5.0, top: 7.0, width: bw, height: bh };
                let (x, y) = (b.left + fx * bw, b.top + fy * bh);
                if let Some(at) = within(b, (pw, ph), x, y) {
                    prop_assert!(at.x >= 0.0 && at.x <= f64::from(pw));
                    prop_assert!(at.y >= 0.0 && at.y <= f64::from(ph));
                }
            }

            // SHELL-BROWSER-13a: the page is fitted whole and centred, so the box's centre is the page's.
            #[test]
            fn the_centre_of_the_box_is_the_centre_of_the_page(
                bw in 1.0f64..2000.0, bh in 1.0f64..2000.0, pw in 1u32..4000, ph in 1u32..4000,
            ) {
                let b = Box2 { left: 3.0, top: 4.0, width: bw, height: bh };
                let at = within(b, (pw, ph), b.left + bw / 2.0, b.top + bh / 2.0).unwrap();
                prop_assert!((at.x - f64::from(pw) / 2.0).abs() < 1e-6 * f64::from(pw).max(1.0));
                prop_assert!((at.y - f64::from(ph) / 2.0).abs() < 1e-6 * f64::from(ph).max(1.0));
            }
        }
    }
}
