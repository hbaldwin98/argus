//! Which regions the window is made of, and how they sit: the sidebar down
//! the left at full height, and beside it the stage under its header, which
//! holds the window controls.
//!
//! No title bar and no status bar (`GPUI.md`, Decided): the foot of the
//! sidebar holds the counts and the daemon line. The stage is the live pane;
//! its header names the pane on screen.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, Entity, Subscription, Window};

use crate::chrome::rail::RailView;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::panes::terminal::Terminal;
use crate::theme::Ledger;
use crate::window::Header;

pub struct Shell {
    heard: Entity<Heard>,
    rail: Entity<RailView>,
    terminal: Entity<Terminal>,
    _observing: Subscription,
}

impl Shell {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let terminal = cx.new(|cx| Terminal::new(heard.clone(), caller.clone(), window, cx));
        let stage = terminal.read(cx).focus_handle().clone();
        let rail = cx.new(|cx| RailView::new(heard.clone(), caller, stage, window, cx));
        let observing = cx.observe(&heard, |_, _, cx| cx.notify());
        Self { heard, rail, terminal, _observing: observing }
    }

    #[cfg(test)]
    pub(crate) fn rail(&self) -> &Entity<RailView> {
        &self.rail
    }

    /// What the header says: the pane on screen.
    pub fn title(heard: &Heard) -> String {
        heard.rail.shown.as_ref().map_or_else(|| "Argus".to_owned(), |shown| shown.title.clone())
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let title = Shell::title(self.heard.read(cx));
        h_flex()
            .size_full()
            .bg(ledger.background)
            .text_color(ledger.foreground)
            .child(self.rail.clone())
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(Header::new().title(title))
                    .child(div().id("stage").flex_1().min_h_0().child(self.terminal.clone())),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::{Event, Rail, Shown};

    fn shown(title: &str) -> Shown {
        Shown {
            pane: 1,
            repository: "argus".into(),
            checkout: "main".into(),
            branch: "main".into(),
            agent: true,
            title: title.into(),
            status: "working",
            resumed: None,
        }
    }

    #[test]
    fn the_header_names_the_pane_on_screen() {
        let mut heard = Heard::new();
        assert_eq!(Shell::title(&heard), "Argus");
        heard.hear(Event::Rail(Box::new(Rail { shown: Some(shown("Trim context")), ..Rail::default() })));
        assert_eq!(Shell::title(&heard), "Trim context");
    }
}
