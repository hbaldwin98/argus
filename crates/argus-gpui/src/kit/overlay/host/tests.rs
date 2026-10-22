use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{
    div, point, px, size, AnyWindowHandle, Bounds, Context, Entity, MouseButton, Render, TestAppContext, WindowBounds,
    WindowOptions,
};

/// A pane with keys of its own, standing under the overlays.
struct Pane {
    focus: FocusHandle,
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().id("pane").track_focus(&self.focus).size_full()
    }
}

struct Opened {
    handle: AnyWindowHandle,
    pane: Entity<Pane>,
    shown: Rc<Cell<bool>>,
    host: Overlays,
}

fn open(cx: &mut TestAppContext) -> Opened {
    cx.update(crate::window::init);
    let shown = Rc::new(Cell::new(true));
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(800.0), px(600.0))))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let pane = cx.new(|cx| Pane { focus: cx.focus_handle() });
                let focus = pane.read(cx).focus.clone();
                window.focus(&focus, cx);
                let watching = shown.clone();
                let host = Overlays::get(cx);
                host.watch_pane(focus, move |_| watching.get());
                built = Some((pane.clone(), host));
                pane
            },
        )
        .expect("open test window")
    });
    let (pane, host) = built.unwrap();
    let opened = Opened { handle, pane, shown, host };
    settle(cx, &opened);
    opened
}

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn show(cx: &mut TestAppContext, opened: &Opened, spec: Spec) -> bool {
    let host = opened.host.clone();
    let did = cx.update_window(opened.handle, |_, window, cx| host.open(spec, |dialog, _, _, _| dialog.title("x"), window, cx)).unwrap();
    settle(cx, opened);
    did
}

fn dialogs(cx: &mut TestAppContext, opened: &Opened) -> bool {
    cx.update_window(opened.handle, |_, window, cx| window.has_active_dialog(cx)).unwrap()
}

fn pane_has_the_keys(cx: &mut TestAppContext, opened: &Opened) -> bool {
    cx.update_window(opened.handle, |_, window, cx| opened.pane.read(cx).focus.is_focused(window)).unwrap()
}

fn escape(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| window.press("escape", cx)).unwrap();
    settle(cx, opened);
}

fn close(cx: &mut TestAppContext, opened: &Opened, kind: Overlay) -> bool {
    let host = opened.host.clone();
    let closed = cx.update_window(opened.handle, |_, window, cx| host.close(kind, window, cx)).unwrap();
    settle(cx, opened);
    closed
}

fn press_the_scrim(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(point(px(790.0), px(590.0)), MouseButton::Left, cx);
        window.pointer_up(point(px(790.0), px(590.0)), MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, opened);
}

#[gpui_kit::test]
fn opening_one_of_the_palette_the_sheet_and_the_settings_closes_the_others(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, Spec::new(Overlay::Palette));
    show(cx, &opened, Spec::new(Overlay::Settings));
    assert_eq!(opened.host.open_kinds(), [Overlay::Settings]);
    assert!(close(cx, &opened, Overlay::Settings));
    assert!(!dialogs(cx, &opened), "the palette's dialog was left up under the settings");
}

#[gpui_kit::test]
fn opening_what_is_up_does_nothing_and_a_toggle_closes_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert!(show(cx, &opened, Spec::new(Overlay::Sheet)));
    assert!(!show(cx, &opened, Spec::new(Overlay::Sheet)), "idempotent");
    let host = opened.host.clone();
    cx.update_window(opened.handle, |_, window, cx| host.toggle(Spec::new(Overlay::Sheet), |dialog, _, _, _| dialog, window, cx)).unwrap();
    settle(cx, &opened);
    assert!(opened.host.open_kinds().is_empty());
    assert!(!dialogs(cx, &opened));
}

#[gpui_kit::test]
fn escape_closes_a_dialog_and_runs_its_answer_first(cx: &mut TestAppContext) {
    let opened = open(cx);
    let answered: Rc<RefCell<u32>> = Rc::default();
    let said = answered.clone();
    let answer = Dismiss::Answers(Rc::new(move |_, _| *said.borrow_mut() += 1));
    show(cx, &opened, Spec::new(Overlay::Question).dismissed(answer));
    escape(cx, &opened);
    assert_eq!(*answered.borrow(), 1, "the question was answered no once");
    assert!(opened.host.open_kinds().is_empty());
    assert!(!dialogs(cx, &opened));
}

#[gpui_kit::test]
fn the_stopped_card_answers_neither_escape_nor_the_scrim(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, Spec::new(Overlay::Stopped).dismissed(Dismiss::Never));
    escape(cx, &opened);
    assert_eq!(opened.host.open_kinds(), [Overlay::Stopped]);
    press_the_scrim(cx, &opened);
    assert_eq!(opened.host.open_kinds(), [Overlay::Stopped], "a press on the scrim closed the card");
    assert!(dialogs(cx, &opened));
    let host = opened.host.clone();
    assert!(!cx.update_window(opened.handle, |_, window, cx| host.escape(window, cx)).unwrap(), "nothing for Escape to close");
}

#[gpui_kit::test]
fn a_press_on_the_scrim_dismisses_a_dialog_that_can_be_dismissed(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, Spec::new(Overlay::Question));
    press_the_scrim(cx, &opened);
    assert!(opened.host.open_kinds().is_empty());
    assert!(!dialogs(cx, &opened));
}

#[gpui_kit::test]
fn escape_passes_over_the_stopped_card_to_the_dialog_under_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, Spec::new(Overlay::AddProject));
    show(cx, &opened, Spec::new(Overlay::Stopped).dismissed(Dismiss::Never));
    let host = opened.host.clone();
    assert!(cx.update_window(opened.handle, |_, window, cx| host.escape(window, cx)).unwrap());
    settle(cx, &opened);
    assert_eq!(opened.host.open_kinds(), [Overlay::Stopped], "the dialog under the card was closed, not the card");
    assert!(dialogs(cx, &opened), "the card is still up");
}

#[gpui_kit::test]
fn closing_a_dialog_under_another_leaves_the_other_up(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, Spec::new(Overlay::AddProject));
    show(cx, &opened, Spec::new(Overlay::Rename));
    assert!(close(cx, &opened, Overlay::AddProject));
    assert_eq!(opened.host.open_kinds(), [Overlay::Rename]);
    assert!(dialogs(cx, &opened));
    assert!(close(cx, &opened, Overlay::Rename));
    assert!(!dialogs(cx, &opened));
}

#[gpui_kit::test]
fn a_builder_is_told_once_that_it_is_the_first_run(cx: &mut TestAppContext) {
    let opened = open(cx);
    let (firsts, runs): (Rc<Cell<u32>>, Rc<Cell<u32>>) = Default::default();
    let (counting_firsts, counting_runs) = (firsts.clone(), runs.clone());
    let host = opened.host.clone();
    cx.update_window(opened.handle, |_, window, cx| {
        host.open(
            Spec::new(Overlay::Rename),
            move |dialog, _, _, first| {
                counting_runs.set(counting_runs.get() + 1);
                counting_firsts.set(counting_firsts.get() + u32::from(first));
                dialog
            },
            window,
            cx,
        )
    })
    .unwrap();
    settle(cx, &opened);
    // Another dialog over it rebuilds the layer, and runs the first one's builder again.
    show(cx, &opened, Spec::new(Overlay::AddProject));
    assert!(runs.get() >= 2, "the layer was not rebuilt, so this shows nothing ({})", runs.get());
    assert_eq!(firsts.get(), 1);
}

#[gpui_kit::test]
fn an_overlay_that_keeps_focus_leaves_the_keys_where_they_were(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert!(pane_has_the_keys(cx, &opened));
    show(cx, &opened, Spec::new(Overlay::Extension).keeping_focus());
    assert!(pane_has_the_keys(cx, &opened), "an extension's question took the keys from the pane");
    close(cx, &opened, Overlay::Extension);
    show(cx, &opened, Spec::new(Overlay::Rename));
    assert!(!pane_has_the_keys(cx, &opened), "an ordinary dialog takes them");
}

#[gpui_kit::test]
fn the_pane_has_the_keys_back_when_the_dialog_that_took_them_closes(cx: &mut TestAppContext) {
    // SHELL-SEAM-10.
    let opened = open(cx);
    show(cx, &opened, Spec::new(Overlay::AddProject));
    assert!(!pane_has_the_keys(cx, &opened));
    close(cx, &opened, Overlay::AddProject);
    assert!(pane_has_the_keys(cx, &opened));
}

#[gpui_kit::test]
fn the_pane_is_not_given_the_keys_when_no_pane_is_shown_by_the_time_the_dialog_closes(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, Spec::new(Overlay::AddProject));
    opened.shown.set(false);
    close(cx, &opened, Overlay::AddProject);
    assert!(!pane_has_the_keys(cx, &opened), "keys were given to a pane nobody can see");
}

#[gpui_kit::test]
fn a_pane_that_did_not_have_the_keys_does_not_gain_them(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.update_window(opened.handle, |_, window, cx| window.blur(cx)).unwrap();
    settle(cx, &opened);
    show(cx, &opened, Spec::new(Overlay::AddProject));
    close(cx, &opened, Overlay::AddProject);
    assert!(!pane_has_the_keys(cx, &opened));
}

#[gpui_kit::test]
fn the_pane_waits_for_the_last_dialog_that_took_the_keys(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, Spec::new(Overlay::AddProject));
    show(cx, &opened, Spec::new(Overlay::Settings));
    close(cx, &opened, Overlay::Settings);
    assert!(!pane_has_the_keys(cx, &opened), "the add-project dialog is still up");
    close(cx, &opened, Overlay::AddProject);
    assert!(pane_has_the_keys(cx, &opened));
}
