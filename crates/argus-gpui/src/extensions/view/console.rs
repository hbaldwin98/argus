//! A console view's body: the terminal on the screen the session gave the view, under a strip that
//! is hidden while it runs, says "Starting..." until the daemon names its pane, and once it ended
//! says how, with the view-wide actions (EXTVIEW-25, -34).
//!
//! The terminal is made once for the side the session numbered the view, and kept as the view moves
//! between a dock and the page, so its screen and its focus stay. A console view that leaves the
//! screen lets its screen go (the terminal is dropped, which unbinds the side). The one console
//! with the keys, which the session says, takes GPUI's focus when it is newly given them, and again
//! if the focus was left on nothing by the body it moved out of.

use argus_desktop_core::connection::page::Side;
use argus_desktop_core::extensions::views::{ExtConsoleEnd, ExtTable, ExtTableConsole};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context, Entity, SharedString, Window};

use super::actions::wide_button;
use super::ExtensionView;
use crate::panes::terminal::Terminal;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The screen a console view draws on, and whether it had the keys at the last look.
#[derive(Default)]
pub struct Held {
    terminal: Option<(Side, Entity<Terminal>)>,
    keyed: bool,
}

impl Held {
    /// The terminal drawing the view, while it has a screen.
    pub fn terminal(&self) -> Option<&Entity<Terminal>> {
        self.terminal.as_ref().map(|(_, terminal)| terminal)
    }
}

/// How a console ended, in words.
pub fn ended_words(end: &ExtConsoleEnd) -> String {
    match end.code {
        Some(code) => format!("Ended (exit {code})"),
        None => "Ended by a signal".to_owned(),
    }
}

/// What the strip says, if it is drawn: nothing while the console runs.
pub fn strip_words(console: &ExtTableConsole) -> Option<String> {
    match (console.pane, &console.ended) {
        (None, _) => Some("Starting\u{2026}".to_owned()),
        (Some(_), Some(end)) => Some(ended_words(end)),
        (Some(_), None) => None,
    }
}

impl ExtensionView {
    /// Gives the view a screen on the side the session numbered it, or lets it go: a view off screen
    /// costs the daemon nothing to send. The keys follow what the session says.
    pub(super) fn sync_console(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let console = self.table(cx).and_then(|table| table.console);
        let side = console.as_ref().and_then(|console| console.side);
        let keyed = console.as_ref().is_some_and(|console| console.keyed);
        match side {
            Some(side) => {
                if self.console.terminal.as_ref().map(|(held, _)| *held) != Some(side) {
                    let (heard, caller) = (self.heard.clone(), self.caller.clone());
                    let terminal = cx.new(|cx| Terminal::new(heard, caller, side, window, cx));
                    self.console.terminal = Some((side, terminal));
                    self.console.keyed = false;
                }
            }
            None => {
                self.console.terminal = None;
                self.console.keyed = false;
            }
        }
        let Some(terminal) = self.console.terminal() else { return };
        let focus = terminal.read(cx).focus_handle().clone();
        // A body moved from a dock to the stage left the focus on nothing.
        if keyed && (!self.console.keyed || (!focus.is_focused(window) && window.focused(cx).is_none())) {
            window.focus(&focus, cx);
        }
        self.console.keyed = keyed;
    }

    pub(super) fn console_body(&self, view: &ExtTable, console: &ExtTableConsole, tokens: &Tokens, _: &mut Context<Self>) -> AnyElement {
        let strip = strip_words(console).map(|words| {
            let wide: Vec<AnyElement> = if console.ended.is_some() {
                view.actions.iter().filter(|action| action.view_wide).map(|action| wide_button(view, action, &self.caller, tokens).into_any_element()).collect()
            } else {
                Vec::new()
            };
            h_flex()
                .id(SharedString::from(format!("ext-strip-{}", view.panel)))
                .test_support()
                .aria_label(SharedString::from(words.clone()))
                .flex_shrink_0()
                .items_center()
                .gap(rem(tokens.sp_8))
                .px(rem(tokens.sp_16))
                .py(rem(tokens.sp_6))
                .border_b_1()
                .border_color(tokens.line)
                .bg(tokens.bg_surface)
                .child(div().flex_1().text_color(tokens.fg_muted).child(SharedString::from(words)))
                .children(wide)
        });
        v_flex()
            .id(SharedString::from(format!("ext-console-{}", view.panel)))
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .children(strip)
            .child(div().flex_1().min_h_0().children(self.console.terminal().cloned()))
            .into_any_element()
    }
}
