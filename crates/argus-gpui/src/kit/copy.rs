//! A button that copies some text and says so for a moment: the page's `copyButton`
//! (`system/copy.ts`), which a view's copy button is (SHELL-SYSTEM-3).
//!
//! The text is read at the press and handed to Rust (`Call::CopyText`), never written by the view
//! (SHELL-SYSTEM-8); whether the write worked is Rust's to say, as a notice, so the button's
//! "Copied" is only the press acknowledged, as the page's is. A second press inside the moment
//! starts it again, so the label never goes back while the person is still copying.

use std::time::Duration;

use argus_desktop_core::connection::page::Call;
use gpui_kit::{prelude::*, App, Context, SharedString, Task, Window};

use crate::connection::caller::Caller;
use crate::kit::button::{button, Look};
use crate::theme::Tokens;

/// How long the button says `Copied`: 1200 ms, as the page does.
pub const COPIED_FOR: Duration = Duration::from_millis(1200);

pub struct CopyButton {
    id: SharedString,
    label: SharedString,
    /// Read when the button is pressed.
    text: Box<dyn Fn(&App) -> String>,
    caller: Caller,
    /// The timer that puts the label back; replaced, and so cancelled, by the next press.
    copied: Option<Task<()>>,
}

impl CopyButton {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, text: impl Fn(&App) -> String + 'static, caller: Caller) -> Self {
        Self { id: id.into(), label: label.into(), text: Box::new(text), caller, copied: None }
    }

    fn press(&mut self, cx: &mut Context<Self>) {
        self.caller.call(Call::CopyText { text: (self.text)(cx) });
        self.copied = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }
}

impl Render for CopyButton {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let shown = if self.copied.is_some() { SharedString::from("Copied") } else { self.label.clone() };
        button(self.id.clone(), shown.clone(), Look::Plain, true, &tokens).child(shown).on_click(cx.listener(|this, _, _, cx| {
            cx.stop_propagation();
            this.press(cx)
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{px, size, AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions};

    use super::*;

    type Seen = (AnyWindowHandle, Rc<RefCell<Vec<String>>>, Rc<RefCell<String>>);

    fn open(cx: &mut TestAppContext) -> Seen {
        cx.update(crate::window::init);
        let calls: Rc<RefCell<Vec<String>>> = Rc::default();
        let text: Rc<RefCell<String>> = Rc::new(RefCell::new("first".into()));
        let (sink, source) = (calls.clone(), text.clone());
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(300.0), px(120.0))))),
                    ..Default::default()
                },
                cx,
                move |_, cx| {
                    let caller = Caller::new(move |call| sink.borrow_mut().push(format!("{call:?}")));
                    cx.new(|_| CopyButton::new("copy", "Copy", move |_| source.borrow().clone(), caller))
                },
            )
            .expect("open test window")
        });
        settle(cx, handle);
        (handle, calls, text)
    }

    fn settle(cx: &mut TestAppContext, handle: AnyWindowHandle) {
        for _ in 0..2 {
            cx.run_until_parked();
            cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
        }
    }

    fn says(cx: &mut TestAppContext, handle: AnyWindowHandle) -> String {
        cx.update_window(handle, |_, window, _| window.find("copy".to_owned()).label().map(str::to_owned)).unwrap().unwrap_or_default()
    }

    fn press(cx: &mut TestAppContext, handle: AnyWindowHandle) {
        cx.update_window(handle, |_, window, cx| window.click("copy".to_owned(), cx)).unwrap();
        settle(cx, handle);
    }

    fn wait(cx: &mut TestAppContext, handle: AnyWindowHandle, by: Duration) {
        cx.executor().advance_clock(by);
        settle(cx, handle);
    }

    #[gpui_kit::test]
    fn pressing_hands_the_text_read_then_to_rust_and_says_copied_for_1200_ms(cx: &mut TestAppContext) {
        let (handle, calls, text) = open(cx);
        assert_eq!(says(cx, handle), "Copy");
        *text.borrow_mut() = "second".into();
        press(cx, handle);
        assert_eq!(*calls.borrow(), ["CopyText { text: \"second\" }"], "read at the press, not when it was built");
        assert_eq!(says(cx, handle), "Copied");
        wait(cx, handle, COPIED_FOR - Duration::from_millis(1));
        assert_eq!(says(cx, handle), "Copied", "still within the moment");
        wait(cx, handle, Duration::from_millis(2));
        assert_eq!(says(cx, handle), "Copy");
    }

    #[gpui_kit::test]
    fn a_second_press_inside_the_moment_starts_it_again(cx: &mut TestAppContext) {
        let (handle, calls, _) = open(cx);
        press(cx, handle);
        wait(cx, handle, Duration::from_millis(1000));
        press(cx, handle);
        wait(cx, handle, Duration::from_millis(1000));
        assert_eq!(says(cx, handle), "Copied", "the first press's timer was replaced, not left to put the label back");
        wait(cx, handle, Duration::from_millis(300));
        assert_eq!(says(cx, handle), "Copy");
        assert_eq!(calls.borrow().len(), 2);
    }
}
