//! Where a panel dropped at a point would land, the area to preview, and the
//! call that moves it there.
//!
//! The regions are the page's one rule (`layout/drag.ts`): a strip along each
//! side of the work area is that side's dock, the bottom of the stage is the
//! bottom dock, and the rest is the stage for a panel that has a page. The
//! decision is Rust's (`DockGesture::Move` and `Stage`); this only names the
//! place the pointer is over, and draws the area it would fill. Over the
//! grid, a panel that may have a cell keeps the docks to a slim band
//! (`in_band`) and splits the cell under it anywhere else.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{Dock, DockGesture, Panel};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, point, prelude::*, px, size, AnyElement, Bounds, Pixels, Point, SharedString};

use crate::theme::scale::rem;
use crate::theme::Tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Dock(Dock),
    Stage,
}

/// How deep a side's drop strip goes, at most: a fifth of the work area.
const EDGE: f32 = 160.0;
const BOTTOM_STRIP: f32 = 160.0;
const SIDE_PREVIEW: f32 = 560.0;
const BOTTOM_PREVIEW: f32 = 320.0;
/// How deep the band at the work area's edges is in which a panel carried over the grid docks
/// rather than splitting a cell, in the page's pixels.
const BAND: f32 = 40.0;

fn left(b: Bounds<Pixels>) -> f32 {
    b.origin.x.as_f32()
}

fn top(b: Bounds<Pixels>) -> f32 {
    b.origin.y.as_f32()
}

fn width(b: Bounds<Pixels>) -> f32 {
    b.size.width.as_f32()
}

fn height(b: Bounds<Pixels>) -> f32 {
    b.size.height.as_f32()
}

fn area(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
}

/// Where a drop at `at` lands, or `None` outside the work area, or over the
/// stage for a panel that has no page (`stage` is false).
pub fn target_at(at: Point<Pixels>, work: Bounds<Pixels>, center: Bounds<Pixels>, stage: bool) -> Option<Target> {
    let (x, y) = (at.x.as_f32(), at.y.as_f32());
    if x < left(work) || x > left(work) + width(work) || y < top(work) || y > top(work) + height(work) {
        return None;
    }
    let edge = EDGE.min(width(work) * 0.2);
    if x < left(work) + edge {
        return Some(Target::Dock(Dock::Left));
    }
    if x > left(work) + width(work) - edge {
        return Some(Target::Dock(Dock::Right));
    }
    if y > top(center) + height(center) - BOTTOM_STRIP.min(height(center) * 0.3) {
        return Some(Target::Dock(Dock::Bottom));
    }
    stage.then_some(Target::Stage)
}

/// The area a drop on `target` would fill, for the preview.
pub fn landing(target: Target, work: Bounds<Pixels>, center: Bounds<Pixels>) -> Bounds<Pixels> {
    match target {
        Target::Dock(Dock::Left) => {
            area(left(work), top(work), SIDE_PREVIEW.min(width(work) * 0.35), height(work))
        }
        Target::Dock(Dock::Right) => {
            let side = SIDE_PREVIEW.min(width(work) * 0.35);
            area(left(work) + width(work) - side, top(work), side, height(work))
        }
        Target::Dock(Dock::Bottom) => {
            let tall = BOTTOM_PREVIEW.min(height(center) * 0.4);
            area(left(center), top(center) + height(center) - tall, width(center), tall)
        }
        Target::Stage => center,
    }
}

/// Whether `at` is in the slim band at the work area's left, right or bottom edge, at `zoom`.
pub fn in_band(at: Point<Pixels>, work: Bounds<Pixels>, zoom: f32) -> bool {
    let (x, y, band) = (at.x.as_f32(), at.y.as_f32(), BAND * zoom);
    x < left(work) + band || x > left(work) + width(work) - band || y > top(work) + height(work) - band
}

/// The area a drop on `target` would fill, with what it does in words, drawn in the work area's box.
pub fn preview(target: Target, work: Bounds<Pixels>, center: Bounds<Pixels>, tokens: &Tokens) -> AnyElement {
    let land = landing(target, work, center);
    let words = SharedString::from(words(target));
    div()
        .id("drop-preview")
        .test_support()
        .aria_label(words.clone())
        .absolute()
        .left(land.origin.x - work.origin.x)
        .top(land.origin.y - work.origin.y)
        .w(land.size.width)
        .h(land.size.height)
        .flex()
        .items_center()
        .justify_center()
        .rounded(rem(tokens.r_sm))
        .border_2()
        .border_color(tokens.accent)
        .bg(tokens.accent.opacity(tokens.share_landing))
        .child(div().px_2().py_1().rounded(rem(tokens.r_sm)).bg(tokens.bg_surface).text_sm().text_color(tokens.fg).child(words))
        .into_any_element()
}

/// What the preview says it would do.
pub fn words(target: Target) -> &'static str {
    match target {
        Target::Dock(Dock::Left) => "Dock on the left",
        Target::Dock(Dock::Right) => "Dock on the right",
        Target::Dock(Dock::Bottom) => "Dock at the bottom",
        Target::Stage => "Open on the stage",
    }
}

/// The call a drop sends: the move the dock menus send, or the stage's.
pub fn call(target: Target, panel: Panel) -> Call {
    let gesture = match target {
        Target::Dock(dock) => DockGesture::Move { panel, dock },
        Target::Stage => DockGesture::Stage { panel },
    };
    Call::Docks { gesture }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> Bounds<Pixels> {
        area(248.0, 34.0, 1000.0, 600.0)
    }

    fn center() -> Bounds<Pixels> {
        work()
    }

    fn over(x: f32, y: f32, stage: bool) -> Option<Target> {
        target_at(point(px(x), px(y)), work(), center(), stage)
    }

    #[test]
    fn a_strip_along_each_side_is_that_sides_dock() {
        assert_eq!(over(260.0, 300.0, false), Some(Target::Dock(Dock::Left)));
        assert_eq!(over(1240.0, 300.0, false), Some(Target::Dock(Dock::Right)));
        // 20 % of the work area is 200 px, held to 160.
        assert_eq!(over(248.0 + 159.0, 300.0, false), Some(Target::Dock(Dock::Left)));
        assert_eq!(over(248.0 + 161.0, 300.0, true), Some(Target::Stage));
    }

    #[test]
    fn the_bottom_of_the_stage_is_the_bottom_dock_and_the_rest_the_stage_for_a_panel_with_a_page() {
        // The bottom strip is 30 % of a 600 px stage, held to 160.
        assert_eq!(over(700.0, 34.0 + 600.0 - 100.0, false), Some(Target::Dock(Dock::Bottom)));
        assert_eq!(over(700.0, 300.0, true), Some(Target::Stage));
        assert_eq!(over(700.0, 300.0, false), None, "a panel with no page has no stage to land on");
    }

    #[test]
    fn outside_the_work_area_nothing_is_a_target() {
        assert_eq!(over(100.0, 300.0, true), None, "over the sidebar");
        assert_eq!(over(700.0, 10.0, true), None, "over the header");
    }

    #[test]
    fn the_band_a_panel_docks_in_over_the_grid_is_slim_and_grows_with_the_zoom() {
        let at = |x: f32, y: f32| point(px(x), px(y));
        assert!(in_band(at(248.0 + 39.0, 300.0), work(), 1.0));
        assert!(!in_band(at(248.0 + 41.0, 300.0), work(), 1.0), "a dock's full strip would be 160");
        assert!(in_band(at(700.0, 34.0 + 600.0 - 39.0), work(), 1.0));
        assert!(!in_band(at(700.0, 34.0 + 600.0 - 41.0), work(), 1.0));
        assert!(in_band(at(248.0 + 1000.0 - 79.0, 300.0), work(), 2.0));
    }

    #[test]
    fn a_preview_fills_what_the_drop_would() {
        let side = landing(Target::Dock(Dock::Right), work(), center());
        assert_eq!((left(side), width(side)), (248.0 + 1000.0 - 350.0, 350.0), "35 % of the area, under 560");
        let bottom = landing(Target::Dock(Dock::Bottom), work(), center());
        assert_eq!((top(bottom), height(bottom)), (34.0 + 600.0 - 240.0, 240.0), "40 % of the stage");
        assert_eq!(landing(Target::Stage, work(), center()), center());
    }

    #[test]
    fn a_drop_sends_the_move_the_dock_menus_send_or_the_stages() {
        let sent = |target| format!("{:?}", call(target, Panel::Views));
        assert_eq!(sent(Target::Dock(Dock::Left)), "Docks { gesture: Move { panel: Views, dock: Left } }");
        assert_eq!(sent(Target::Stage), "Docks { gesture: Stage { panel: Views } }");
    }
}
