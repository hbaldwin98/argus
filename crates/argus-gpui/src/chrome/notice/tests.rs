use std::time::Duration;

use super::*;
use gpui_kit::px;
use argus_desktop_core::connection::page::Event;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{size, AnyWindowHandle, Bounds, MouseButton, TestAppContext, WindowBounds, WindowOptions};

struct Opened {
    handle: AnyWindowHandle,
    heard: Entity<Heard>,
}

fn open(cx: &mut TestAppContext) -> Opened {
    cx.update(crate::window::init);
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(800.0), px(400.0))))),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let heard = cx.new(|_| Heard::new());
                let notice = cx.new(|cx| NoticeView::new(heard.clone(), cx));
                built = Some(heard);
                cx.new(|_| Stage { notice })
            },
        )
        .expect("open test window")
    });
    let opened = Opened { handle, heard: built.unwrap() };
    settle(cx, &opened);
    opened
}

/// The stage the line sits over, as `chrome/shell` has it.
struct Stage {
    notice: Entity<NoticeView>,
}

impl Render for Stage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().child(self.notice.clone())
    }
}

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn say(cx: &mut TestAppContext, opened: &Opened, text: &str) {
    cx.update(|cx| {
        opened.heard.update(cx, |heard, cx| {
            heard.hear(Event::refused(text.to_owned()));
            cx.notify();
        })
    });
    settle(cx, opened);
}

fn showing(cx: &mut TestAppContext, opened: &Opened) -> Option<String> {
    cx.update_window(opened.handle, |_, window, _| window.try_find("notice".to_owned()).and_then(|found| found.label().map(str::to_owned))).unwrap()
}

fn wait(cx: &mut TestAppContext, opened: &Opened, by: Duration) {
    cx.executor().advance_clock(by);
    settle(cx, opened);
}

#[gpui_kit::test]
fn a_notice_is_a_line_with_its_words_and_nothing_is_drawn_without_one(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert_eq!(showing(cx, &opened), None);
    say(cx, &opened, "no pane is on screen");
    assert_eq!(showing(cx, &opened).as_deref(), Some("no pane is on screen"));
}

#[gpui_kit::test]
fn a_notice_goes_after_six_seconds_and_not_before(cx: &mut TestAppContext) {
    let opened = open(cx);
    say(cx, &opened, "could not copy: no clipboard");
    wait(cx, &opened, NOTICE_FOR - Duration::from_millis(100));
    assert!(showing(cx, &opened).is_some(), "gone before its six seconds");
    wait(cx, &opened, Duration::from_millis(200));
    assert_eq!(showing(cx, &opened), None);
}

#[gpui_kit::test]
fn dismiss_puts_it_away(cx: &mut TestAppContext) {
    let opened = open(cx);
    say(cx, &opened, "nothing on the clipboard to paste");
    let at = cx.update_window(opened.handle, |_, window, _| window.find("notice-dismiss".to_owned()).bounds().center()).unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert_eq!(showing(cx, &opened), None);
}

#[gpui_kit::test]
fn a_new_notice_replaces_the_one_showing_and_restarts_the_timer(cx: &mut TestAppContext) {
    let opened = open(cx);
    say(cx, &opened, "first");
    wait(cx, &opened, Duration::from_secs(4));
    say(cx, &opened, "second");
    assert_eq!(showing(cx, &opened).as_deref(), Some("second"), "one line, the newest");
    // The first one's six seconds end here; the second's have four to go.
    wait(cx, &opened, Duration::from_secs(4));
    assert_eq!(showing(cx, &opened).as_deref(), Some("second"), "the first one's timer took the second down");
    wait(cx, &opened, Duration::from_secs(3));
    assert_eq!(showing(cx, &opened), None);
}

#[gpui_kit::test]
fn the_same_words_again_start_the_six_seconds_over(cx: &mut TestAppContext) {
    let opened = open(cx);
    say(cx, &opened, "no pane is on screen");
    wait(cx, &opened, Duration::from_secs(4));
    say(cx, &opened, "no pane is on screen");
    wait(cx, &opened, Duration::from_secs(4));
    assert!(showing(cx, &opened).is_some(), "a repeat did not restart the timer");
}

fn say_as(cx: &mut TestAppContext, opened: &Opened, event: Event) {
    cx.update(|cx| {
        opened.heard.update(cx, |heard, cx| {
            heard.hear(event);
            cx.notify();
        })
    });
    settle(cx, opened);
}

fn present(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

#[gpui_kit::test]
fn a_done_notice_has_no_close_and_the_others_have_one(cx: &mut TestAppContext) {
    let opened = open(cx);
    say_as(cx, &opened, Event::done("committed"));
    assert!(!present(cx, &opened, "notice-dismiss"));
    for event in [Event::arrived("pasted a.png"), Event::refused("no pane"), Event::failed("could not copy")] {
        say_as(cx, &opened, event);
        assert!(present(cx, &opened, "notice-dismiss"));
    }
}

#[gpui_kit::test]
fn a_long_notice_is_cut_at_360px_and_keeps_its_whole_words_for_the_tooltip_and_the_screen_reader(cx: &mut TestAppContext) {
    let opened = open(cx);
    let words = "That session's pane has ended; open it again from the repository's recent sessions, which are listed under it";
    say_as(cx, &opened, Event::refused(words));
    let (width, label) = cx
        .update_window(opened.handle, |_, window, _| {
            let found = window.find("notice".to_owned());
            (f32::from(found.bounds().size.width), found.label().map(str::to_owned))
        })
        .unwrap();
    assert!(width <= 360.5, "{width}");
    assert_eq!(label.as_deref(), Some(words));
}

#[test]
fn the_fade_holds_then_goes_over_the_last_of_the_six_seconds() {
    assert_eq!(opacity(0.0), 1.0);
    assert_eq!(opacity(1.0 - FADE), 1.0);
    assert!(opacity(1.0 - FADE / 2.0) < 1.0 && opacity(1.0 - FADE / 2.0) > 0.0);
    assert_eq!(opacity(1.0), 0.0);
}
