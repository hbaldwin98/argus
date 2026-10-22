//! Which regions the window is made of, and how they sit: the sidebar down
//! the left at full height, and beside it the stage under its header, which
//! holds the window controls.
//!
//! No title bar and no status bar (`GPUI.md`, Decided): the foot of the
//! sidebar holds the counts and the daemon line. Under the stage's header is
//! the work area (`layout/work`): the live pane with the docks around it, or the
//! Git stage in its place while it is up (`chrome/stage`). The header names
//! the pane on screen, or Git.

use argus_desktop_core::connection::page::Side;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, Entity, Subscription, Window};

use crate::chrome::notice::NoticeView;
use crate::chrome::rail::RailView;
use crate::chrome::stage::StageView;
use crate::connection::caller::Caller;
use crate::connection::heard::{Heard, Ran};
use crate::keys::extension::Commands;
use crate::keys::layer::Layers;
use crate::keys::over::run_over;
use crate::kit::overlay::host::Overlays;
use crate::layout::work::WorkView;
use crate::panes::console::ConsoleView;
use crate::panes::terminal::Terminal;
use crate::review::stage::GitStage;
use crate::theme::Tokens;
use crate::chrome::header::Header;

pub struct Shell {
    heard: Entity<Heard>,
    rail: Entity<RailView>,
    work: Entity<WorkView>,
    notice: Entity<NoticeView>,
    _observing: Subscription,
    /// A key the session heard in a pane that opens the palette, the sheet or the settings.
    _running: Subscription,
}

impl Shell {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The window's own keys ask the session as the page does.
        Layers::of(cx).serve(caller.clone());
        let terminal = cx.new(|cx| Terminal::new(heard.clone(), caller.clone(), Side::Workspace, window, cx));
        let stage = terminal.read(cx).focus_handle().clone();
        // The Workspace's pane takes the keys back from the dialogs that took them, while one is on screen.
        let on_screen = heard.clone();
        Overlays::get(cx).watch_pane(stage.clone(), move |cx| on_screen.read(cx).rail.shown.is_some());
        let rail = cx.new(|cx| RailView::new(heard.clone(), caller.clone(), stage.clone(), window, cx));
        let console = cx.new(|cx| ConsoleView::new(heard.clone(), caller.clone(), stage, window, cx));
        // The Workspace's pane is the stage the docks sit around.
        let git = cx.new(|cx| GitStage::new(heard.clone(), caller.clone(), window, cx));
        let stage = cx.new(|cx| StageView::new(heard.clone(), terminal.into(), git.into(), cx));
        let work = cx.new(|cx| WorkView::new(heard.clone(), caller, stage.into(), console.into(), cx));
        let notice = cx.new(|cx| NoticeView::new(heard.clone(), cx));
        let observing = cx.observe(&heard, |_, heard, cx| {
            let commands = heard.read(cx).extension_commands.clone();
            Commands::of(cx).set(commands);
            cx.notify();
        });
        let running = cx.subscribe_in(&heard, window, |_, _, Ran(action): &Ran, window, cx| {
            run_over(*action, window, cx);
        });
        Self { heard, rail, work, notice, _observing: observing, _running: running }
    }

    #[cfg(test)]
    pub(crate) fn rail(&self) -> &Entity<RailView> {
        &self.rail
    }

    /// The repository and branch under the title, while a pane is on screen
    /// (`.head-stack .crumbs`): Git is named by its title alone.
    pub fn crumb(heard: &Heard) -> Option<(String, String)> {
        if heard.git_up() {
            return None;
        }
        heard.rail.shown.as_ref().map(|shown| {
            (shown.repository.clone(), if shown.branch.is_empty() { shown.checkout.clone() } else { shown.branch.clone() })
        })
    }

    /// What the header says: Git while its stage is up, else the pane on screen.
    pub fn title(heard: &Heard) -> String {
        if heard.git_up() {
            return "Git".to_owned();
        }
        heard.rail.shown.as_ref().map_or_else(|| "Argus".to_owned(), |shown| shown.title.clone())
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let (title, crumb) = {
            let heard = self.heard.read(cx);
            (Shell::title(heard), Shell::crumb(heard))
        };
        h_flex()
            .size_full()
            .bg(tokens.bg_sunken)
            .text_color(tokens.fg)
            .child(self.rail.clone())
            .child(
                v_flex()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(Header::new().title(title).crumb(crumb))
                    .child(div().id("stage").flex_1().min_h_0().child(self.work.clone()))
                    // Over the stage, under nothing: a notice is the last thing drawn.
                    .child(self.notice.clone()),
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
            browser: false,
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
