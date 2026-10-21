//! The spike's binary: the frameless Ledger window with an empty stage.

// A console window behind the GUI is noise outside a debug build.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

use argus_gpui::theme::Ledger;
use argus_gpui::window;
use gpui_kit::{div, prelude::*, App, Context, Window};

struct Stage;

impl Render for Stage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(ledger.faint)
            .child("argus-gpui spike")
    }
}

fn main() {
    window::application().run(|cx: &mut App| {
        window::init(cx);
        window::open(cx, |_, cx| cx.new(|_| Stage)).expect("the spike window opens");
        cx.activate(true);
    });
}
