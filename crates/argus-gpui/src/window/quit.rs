//! Ending what ran for the window when the last one closes, and answering a close by quitting.

use gpui_kit::{App, Subscription, Window};

/// Runs `stop` and quits once the last window has closed, so what was kept
/// running for the window (the daemon loop, the frame tick) ends with it
/// rather than updating a window that is gone. `stop` is run before the quit
/// and only once, whichever way the window closed: the header's control, the
/// system's close, or the window being removed from code.
pub fn quit_when_closed(cx: &mut App, stop: impl FnMut() + 'static) -> Subscription {
    quit_when_closed_with(cx, stop, |cx| cx.quit())
}

/// Answers a close of the only window by quitting the app with the window
/// still open. Closing it first makes gpui-pre-windows touch the destroyed
/// handle as the app winds down ("window not found", "Invalid window
/// handle"); a quit tears the window down in the order a timed quit does,
/// which logs nothing. The system's close, Alt+F4 and the header's control
/// all ask here first.
pub fn quit_instead_of_closing(window: &mut Window, cx: &App) {
    window.on_window_should_close(cx, |_, cx| {
        if cx.windows().len() > 1 {
            return true;
        }
        cx.quit();
        false
    });
}

/// [`quit_when_closed`] with the quit given: the test platform's quit does
/// nothing and runs no quit observer, so a test can only see it asked for.
fn quit_when_closed_with(cx: &mut App, mut stop: impl FnMut() + 'static, mut quit: impl FnMut(&mut App) + 'static) -> Subscription {
    cx.on_window_closed(move |cx, _| {
        if cx.windows().is_empty() {
            stop();
            quit(cx);
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;


    use gpui_kit::{div, prelude::*, AnyWindowHandle, Context, Render, TestAppContext, Window};

    use super::*;
    use crate::window::{init, open};

    fn open_empty(cx: &mut TestAppContext) -> AnyWindowHandle {
        cx.update(|cx| {
            open(cx, |_, cx| cx.new(|_| Nothing)).expect("open test window").0
        })
    }

    struct Nothing;

    impl Render for Nothing {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn close(cx: &mut TestAppContext, handle: AnyWindowHandle) {
        cx.update_window(handle, |_, window, _| window.remove_window()).unwrap();
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn closing_the_last_window_stops_what_ran_for_it(cx: &mut TestAppContext) {
        cx.update(init);
        let stops = Rc::new(Cell::new(0));
        let counted = stops.clone();
        let quits = Rc::new(Cell::new(0));
        let quit = quits.clone();
        let _watching = cx.update(|cx| {
            quit_when_closed_with(cx, move || counted.set(counted.get() + 1), move |_| quit.set(quit.get() + 1))
        });
        let first = open_empty(cx);
        let second = open_empty(cx);

        close(cx, first);
        assert_eq!(stops.get(), 0, "stopped while a window was still open");
        assert_eq!(quits.get(), 0, "quit while a window was still open");
        close(cx, second);
        assert_eq!(stops.get(), 1, "the last window closed and nothing stopped");
        assert_eq!(quits.get(), 1, "the last window closed and the app kept running");
    }
}
