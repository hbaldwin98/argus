//! A drawing opened larger: the corner button and the click that open it, how it opens, zoom about the
//! pointer, a drag that moves it, the source beside it and the keys it holds (FIGURE-1 to FIGURE-7).

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Layer, Place};
use argus_desktop_core::views::blocks::diagram::layout;
use argus_desktop_core::views::blocks::zoom::{fit, percent, start, Zoom};
use gpui_kit::{point, px, InputEvent as _, ScrollDelta, ScrollWheelEvent, TestAppContext, Window};
use serde_json::{json, Value};

use super::drawn::{flow, graph, with_diagrams};
use super::*;
use crate::keys::layer::{Handler, Layers, Press, Verdict};
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;
use crate::views::blocks::figure::larger::Larger;
use crate::views::blocks::figure::open::up;

const SOURCE: &str = "sequenceDiagram\na->>b: hi";

fn diagram() -> Value {
    json!([{"kind": "diagram", "source": SOURCE}])
}

fn open_diagram(cx: &mut TestAppContext) -> Rig {
    open_as(cx, 900.0, diagram(), with_diagrams(vec![flow()]))
}

fn is_up(cx: &mut TestAppContext, rig: &Rig) -> bool {
    cx.update_window(rig.handle, |_, _, cx| Overlays::get(cx).is_open(Overlay::Figure)).unwrap()
}

fn larger(cx: &mut TestAppContext, rig: &Rig) -> gpui_kit::Entity<Larger> {
    cx.update_window(rig.handle, |_, _, cx| up(cx)).unwrap().expect("a larger view is up")
}

fn zoom(cx: &mut TestAppContext, rig: &Rig) -> Zoom {
    let view = larger(cx, rig);
    cx.update(|cx| view.read(cx).zoom()).expect("the stage was measured")
}

/// The stage's size and the window's place for it, in pixels (the rig's window is 1:1 with the page's).
fn stage(cx: &mut TestAppContext, rig: &Rig) -> (f32, f32, f32, f32) {
    let at = bounds(cx, rig, "figure-stage").expect("the stage is drawn");
    (f32::from(at.size.width), f32::from(at.size.height), f32::from(at.origin.x), f32::from(at.origin.y))
}

fn percent_shown(cx: &mut TestAppContext, rig: &Rig) -> String {
    cx.update_window(rig.handle, |_, window, _| window.find("figure-pct".to_owned()).value().map(str::to_owned).unwrap_or_default()).unwrap()
}

fn wheel(cx: &mut TestAppContext, rig: &Rig, at: (f32, f32), y: f32) {
    cx.update_window(rig.handle, |_, window, cx| {
        window.dispatch_event(ScrollWheelEvent { position: point(px(at.0), px(at.1)), delta: ScrollDelta::Pixels(point(px(0.0), px(y))), ..Default::default() }.to_platform_input(), cx);
    })
    .unwrap();
    settle(cx, rig);
}

fn press(cx: &mut TestAppContext, rig: &Rig, key: &str) {
    cx.update_window(rig.handle, |_, window, cx| window.press(key, cx)).unwrap();
    settle(cx, rig);
}

/// Opens the diagram's figure and lets the stage be measured.
fn open_larger(cx: &mut TestAppContext, rig: &Rig) {
    // The corner button shows while the pointer is over the drawing.
    cx.update_window(rig.handle, |_, window, cx| window.hover("7/v.0-diagram-inline".to_owned(), cx)).unwrap();
    settle(cx, rig);
    click(cx, rig, "7/v.0-diagram-open");
    for _ in 0..3 {
        settle(cx, rig);
    }
}

// FIGURE-1
#[gpui_kit::test]
fn a_drawing_opens_larger_from_its_corner_button_and_from_a_press_on_it(cx: &mut TestAppContext) {
    let rig = open_diagram(cx);
    assert_eq!(label(cx, &rig, "7/v.0-diagram-open").as_deref(), Some("Open Sequence diagram \u{b7} 2 steps larger"));
    assert!(!is_up(cx, &rig));
    open_larger(cx, &rig);
    assert!(is_up(cx, &rig));
    cx.update_window(rig.handle, |_, window, cx| {
        Overlays::get(cx).close(Overlay::Figure, window, cx);
    })
    .unwrap();
    click(cx, &rig, "7/v.0-diagram-inline");
    assert!(is_up(cx, &rig), "the drawing itself opens it");
}

#[gpui_kit::test]
fn a_box_that_opens_a_view_takes_the_press_and_the_drawing_does_not_open(cx: &mut TestAppContext) {
    let rig = open(cx, 900.0, graph());
    click(cx, &rig, "7/v.0-graph-node0");
    assert_eq!(calls(&rig), [said(Call::OpenSavedView { id: "release".into(), from: None })]);
    assert!(!is_up(cx, &rig), "the box had the press");
}

// FIGURE-2, FIGURE-3
#[gpui_kit::test]
fn the_larger_view_is_named_by_its_drawing_and_opens_as_core_says(cx: &mut TestAppContext) {
    let rig = open_diagram(cx);
    open_larger(cx, &rig);
    assert_eq!(label(cx, &rig, "figure").as_deref(), Some("Sequence diagram \u{b7} 2 steps"));
    let (sw, sh, ..) = stage(cx, &rig);
    let drawn = layout(&flow()).unwrap();
    let opened = start(drawn.width, drawn.height, sw, sh);
    assert_eq!(percent_shown(cx, &rig), percent(opened));
    assert_eq!(zoom(cx, &rig).scale, opened.scale);
}

// FIGURE-4
#[gpui_kit::test]
fn the_buttons_step_by_a_quarter_about_the_centre_and_the_percent_goes_back_to_actual_size(cx: &mut TestAppContext) {
    let rig = open_diagram(cx);
    open_larger(cx, &rig);
    click(cx, &rig, "figure-pct");
    assert_eq!(percent_shown(cx, &rig), "100%");
    click(cx, &rig, "figure-in");
    assert_eq!(percent_shown(cx, &rig), "125%");
    click(cx, &rig, "figure-out");
    click(cx, &rig, "figure-out");
    assert_eq!(percent_shown(cx, &rig), "80%");
    let (sw, sh, ..) = stage(cx, &rig);
    let drawn = layout(&flow()).unwrap();
    click(cx, &rig, "figure-fit");
    assert_eq!(percent_shown(cx, &rig), percent(fit(drawn.width, drawn.height, sw, sh)), "Fit is the whole drawing in the stage");
}

#[gpui_kit::test]
fn the_wheel_zooms_about_the_pointer_which_keeps_the_point_under_it(cx: &mut TestAppContext) {
    let rig = open_diagram(cx);
    open_larger(cx, &rig);
    click(cx, &rig, "figure-pct");
    let (_, _, ox, oy) = stage(cx, &rig);
    // Well off the stage's centre, so a zoom about the centre would move this point.
    let pointer = (ox + 90.0, oy + 40.0);
    let before = zoom(cx, &rig);
    let under = ((pointer.0 - ox - before.x) / before.scale, (pointer.1 - oy - before.y) / before.scale);
    wheel(cx, &rig, pointer, 200.0);
    let after = zoom(cx, &rig);
    assert!(after.scale > before.scale, "a turn away from the person (GPUI's positive) zooms in");
    let now = ((pointer.0 - ox - after.x) / after.scale, (pointer.1 - oy - after.y) / after.scale);
    assert!((now.0 - under.0).abs() < 0.05 && (now.1 - under.1).abs() < 0.05, "{under:?} then {now:?}");
    wheel(cx, &rig, pointer, -1.0e6);
    assert_eq!(zoom(cx, &rig).scale, argus_desktop_core::views::blocks::zoom::MIN_SCALE, "the scale is bounded");
}

// FIGURE-5, FIGURE-7
#[gpui_kit::test]
fn a_press_that_goes_five_pixels_drags_the_drawing_and_a_shorter_one_does_not(cx: &mut TestAppContext) {
    let rig = open_diagram(cx);
    open_larger(cx, &rig);
    let (_, _, ox, oy) = stage(cx, &rig);
    let from = point(px(ox + 300.0), px(oy + 300.0));
    let before = zoom(cx, &rig);
    cx.update_window(rig.handle, |_, window, cx| window.drag(from, point(px(ox + 304.0), px(oy + 302.0)), cx)).unwrap();
    settle(cx, &rig);
    assert_eq!(zoom(cx, &rig), before, "4 px is a click, not a drag");
    cx.update_window(rig.handle, |_, window, cx| window.drag(from, point(px(ox + 340.0), px(oy + 330.0)), cx)).unwrap();
    settle(cx, &rig);
    let after = zoom(cx, &rig);
    assert!((after.x - before.x - 40.0).abs() < 0.01 && (after.y - before.y - 30.0).abs() < 0.01, "{before:?} then {after:?}");
    assert_eq!(after.scale, before.scale);
}

// FIGURE-2
#[gpui_kit::test]
fn source_is_a_button_only_a_drawing_with_a_source_has_and_it_shows_the_source_beside_it(cx: &mut TestAppContext) {
    let rig = open_diagram(cx);
    open_larger(cx, &rig);
    assert!(!exists(cx, &rig, "figure-source-pane"));
    click(cx, &rig, "figure-source");
    assert!(exists(cx, &rig, "figure-source-pane"));
    click(cx, &rig, "figure-source");
    assert!(!exists(cx, &rig, "figure-source-pane"), "it is a toggle");
    let graph_rig = open(cx, 900.0, graph());
    click(cx, &graph_rig, "7/v.0-graph-open");
    for _ in 0..3 {
        settle(cx, &graph_rig);
    }
    assert!(is_up(cx, &graph_rig));
    assert!(!exists(cx, &graph_rig, "figure-source"), "a graph has no text it was drawn from");
}

#[gpui_kit::test]
fn opening_another_closes_the_one_up_and_the_close_button_closes_it(cx: &mut TestAppContext) {
    let rig = open_as(cx, 900.0, json!([{"kind": "diagram", "source": SOURCE}, graph()[0]]), with_diagrams(vec![flow()]));
    click(cx, &rig, "7/v.0-diagram-open");
    assert_eq!(label(cx, &rig, "figure").as_deref(), Some("Sequence diagram \u{b7} 2 steps"));
    cx.update_window(rig.handle, |_, window, cx| crate::views::blocks::figure::open::open(Rc::new(second()), window, cx)).unwrap();
    settle(cx, &rig);
    assert_eq!(label(cx, &rig, "figure").as_deref(), Some("Second"), "one at a time");
    click(cx, &rig, "figure-close");
    assert!(!is_up(cx, &rig));
}

fn second() -> crate::views::blocks::figure::Figure {
    crate::views::blocks::figure::Figure { title: "Second".into(), source: None, size: (100.0, 100.0), draw: Rc::new(|_| gpui_kit::div().into_any_element()) }
}

// FIGURE-5: a box pressed in the larger view opened a view behind it, which is then in sight.
#[gpui_kit::test]
fn a_box_pressed_in_the_larger_view_opens_its_view_and_closes_the_larger_one(cx: &mut TestAppContext) {
    let rig = open(cx, 900.0, graph());
    click(cx, &rig, "7/v.0-graph-open");
    for _ in 0..3 {
        settle(cx, &rig);
    }
    click(cx, &rig, "7/v.0-graph-larger-node0");
    assert_eq!(calls(&rig), [said(Call::OpenSavedView { id: "release".into(), from: None })]);
    assert!(!is_up(cx, &rig));
}

/// A layer that is always up and writes every key it is shown.
struct Listening(Rc<RefCell<Vec<String>>>);

impl Handler for Listening {
    fn open(&self, _: &Window, _: &gpui_kit::App) -> bool {
        true
    }

    fn keys(&self, press: &Press, _: &[Place], _: &mut Window, _: &mut gpui_kit::App) -> Verdict {
        self.0.borrow_mut().push(press.keystroke.key.clone());
        Verdict::Held
    }
}

// FIGURE-6
#[gpui_kit::test]
fn an_open_figure_holds_every_key_and_the_zoom_keys_and_escape_are_its_own(cx: &mut TestAppContext) {
    let rig = open_diagram(cx);
    let heard: Rc<RefCell<Vec<String>>> = Rc::default();
    cx.update(|cx| Layers::of(cx).register(Layer::Rail, Rc::new(Listening(heard.clone()))));
    press(cx, &rig, "j");
    assert_eq!(*heard.borrow(), ["j"], "with no figure up the layers hear it");
    open_larger(cx, &rig);
    press(cx, &rig, "0");
    assert_eq!(percent_shown(cx, &rig), "100%");
    press(cx, &rig, "=");
    assert_eq!(percent_shown(cx, &rig), "125%");
    press(cx, &rig, "-");
    assert_eq!(percent_shown(cx, &rig), "100%");
    press(cx, &rig, "f");
    let (sw, sh, ..) = stage(cx, &rig);
    let drawn = layout(&flow()).unwrap();
    assert_eq!(percent_shown(cx, &rig), percent(fit(drawn.width, drawn.height, sw, sh)));
    press(cx, &rig, "j");
    assert_eq!(*heard.borrow(), ["j"], "no layer under the figure heard the key");
    assert!(is_up(cx, &rig), "a key it has no use for leaves it up");
    press(cx, &rig, "escape");
    assert!(!is_up(cx, &rig));
    press(cx, &rig, "j");
    assert_eq!(*heard.borrow(), ["j", "j"], "and with it gone they hear again");
}

// FIGURE-3
#[gpui_kit::test]
fn a_drawing_nobody_has_moved_is_placed_again_whenever_the_stage_changes_size(cx: &mut TestAppContext) {
    let rig = open_diagram(cx);
    open_larger(cx, &rig);
    let drawn = layout(&flow()).unwrap();
    let (before, ..) = stage(cx, &rig);
    click(cx, &rig, "figure-source");
    for _ in 0..3 {
        settle(cx, &rig);
    }
    let (sw, sh, ..) = stage(cx, &rig);
    assert!(sw < before, "the source pane takes room from the stage");
    assert_eq!(zoom(cx, &rig), start(drawn.width, drawn.height, sw, sh), "still whole and centred in the smaller stage");
    click(cx, &rig, "figure-in");
    click(cx, &rig, "figure-source");
    for _ in 0..3 {
        settle(cx, &rig);
    }
    let (sw, sh, ..) = stage(cx, &rig);
    assert_ne!(zoom(cx, &rig), start(drawn.width, drawn.height, sw, sh), "a drawing the person zoomed stays where they left it");
}
