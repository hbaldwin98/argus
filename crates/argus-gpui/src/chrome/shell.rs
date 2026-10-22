//! Which regions the window is made of, and how they sit: the sidebar down
//! the left at full height, and beside it the stage under its header, which
//! holds the window controls.
//!
//! No title bar and no status bar (`GPUI.md`, Decided): the foot of the
//! sidebar holds the counts and the daemon line. Under the stage's header is
//! the work area (`layout/work`): the stage that `chrome/stage/route` picks with the docks around
//! it. The header says what `StripPage` (core) makes of the pane on screen or the stage up, with
//! the stage's own words and tools from `chrome/header/slots`.

use argus_desktop_core::chrome::strip::{Says, StripPage};
use argus_desktop_core::connection::page::{Call, Side, Stage};
use argus_desktop_core::panes::console::ConsolePlace;
use argus_desktop_core::system::clock::Clock;
use argus_desktop_core::views::head;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, Entity, Subscription, Window};

use crate::browser::stage::BrowsersStage;
use crate::checkouts::stage::CheckoutsStage;
use crate::code::stage::CodeStage;
use crate::chrome::header::launcher::launcher;
use crate::chrome::header::more;
use crate::chrome::header::slots::Slots;
use crate::chrome::header::Header;
use crate::chrome::notice::NoticeView;
use crate::chrome::projects::add::AddView;
use crate::chrome::projects::question;
use crate::chrome::rail::RailView;
use crate::chrome::stage::route::frame;
use crate::chrome::stage::{stopped, Bodies, StageView};
use crate::connection::caller::Caller;
use crate::connection::heard::{Heard, Ran};
use crate::keys::extension::Commands;
use crate::keys::layer::Layers;
use crate::keys::over::{run_over, Surfaces};
use crate::keys::palette::PaletteView;
use crate::keys::sheet::SheetView;
use crate::kit::overlay::host::Overlays;
use crate::layout::work::WorkView;
use crate::panes::console::ConsoleView;
use crate::panes::split::SplitView;
use crate::panes::terminal::{Labelled, Terminal};
use crate::review::stage::GitStage;
use crate::settings::look;
use crate::settings::preferences::PreferencesView;
use crate::theme::Tokens;
use crate::views;

pub struct Shell {
    heard: Entity<Heard>,
    caller: Caller,
    rail: Entity<RailView>,
    work: Entity<WorkView>,
    notice: Entity<NoticeView>,
    /// The Workspace's screen, whose size the header says.
    terminal: Entity<Terminal>,
    /// The grid of panes, for a test to find a cell's screen.
    #[cfg(test)]
    split: Entity<SplitView>,
    _observing: Subscription,
    /// The add-a-project dialog, which opens itself over the window while the session holds it.
    _add: Entity<AddView>,
    /// The words of the pane's size changed.
    _labelled: Subscription,
    /// The system's light or dark changed.
    _appearance: Subscription,
    /// A key the session heard in a pane that opens the palette, the sheet or the settings.
    _running: Subscription,
}

impl Shell {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The window's own keys ask the session as the page does.
        Layers::of(cx).serve(caller.clone());
        crate::extensions::install::layers(&heard, cx);
        // The tracker dialogs open themselves over the window while the session holds them.
        crate::board::tracker::install::install(&heard, &caller, window, cx);
        let terminal = cx.new(|cx| Terminal::new(heard.clone(), caller.clone(), Side::Workspace, window, cx));
        let stage = terminal.read(cx).focus_handle().clone();
        // The Workspace's pane takes the keys back from the dialogs that took them, while one is on screen.
        let on_screen = heard.clone();
        Overlays::get(cx).watch_pane(stage.clone(), move |cx| on_screen.read(cx).rail.shown.is_some());
        let rail = cx.new(|cx| RailView::new(heard.clone(), caller.clone(), stage.clone(), window, cx));
        let console = cx.new(|cx| ConsoleView::new(heard.clone(), caller.clone(), stage, window, cx));
        // The agents' views draw in their dock and as the page over the stage.
        views::agent::install::install(&heard, &caller, cx);
        crate::board::install::install(&heard, &caller, window, cx);
        // The composer is the stage of New, and the sessions dialog opens over the window while the session holds it.
        crate::agents::install::install(&heard, &caller, window, cx);
        // The Workspace's pane is the stage the docks sit around.
        let git = cx.new(|cx| GitStage::new(heard.clone(), caller.clone(), window, cx));
        let checkouts = cx.new(|cx| CheckoutsStage::new(heard.clone(), caller.clone(), window, cx));
        let code = cx.new(|cx| CodeStage::new(heard.clone(), caller.clone(), window, cx));
        let browsers = cx.new(|cx| BrowsersStage::new(heard.clone(), caller.clone(), cx));
        // The Feature stage's index is the page of its stage, found where the stage looks for one.
        crate::board::feature_stage::install(&heard, &caller, window, cx);
        let split = cx.new(|cx| SplitView::new(heard.clone(), caller.clone(), terminal.clone(), window, cx));
        let stage = cx.new(|cx| StageView::new(heard.clone(), caller.clone(), Bodies { pane: split.clone().into(), git: git.into(), checkouts: checkouts.into(), code: code.into(), browsers: browsers.into() }, cx));
        let work = cx.new(|cx| WorkView::new(heard.clone(), caller.clone(), stage.into(), console.into(), cx));
        let notice = cx.new(|cx| NoticeView::new(heard.clone(), cx));
        let add = cx.new(|cx| AddView::new(heard.clone(), caller.clone(), window, cx));
        Surfaces {
            palette: cx.new(|cx| PaletteView::new(heard.clone(), caller.clone(), window, cx)),
            sheet: cx.new(|cx| SheetView::new(heard.clone(), window, cx)),
            settings: cx.new(|cx| PreferencesView::new(heard.clone(), caller.clone(), window, cx)),
        }
        .install(cx);
        // The window takes the look the app is in: the backdrop, the zoom and the border are its own.
        look::dress_new(window, cx);
        let stopping = caller.clone();
        let observing = cx.observe_in(&heard, window, move |_, heard, window, cx| {
            // The look follows every choice the session sends, whether the dialog is open or not.
            if let Some(page) = heard.read(cx).settings.clone() {
                look::follow(&page, window, cx);
            }
            let commands = heard.read(cx).extension_commands.clone();
            Commands::of(cx).set(commands);
            // The Stopped card is up exactly while the daemon is stopped.
            stopped::follow(&heard.read(cx).link(), &stopping, window, cx);
            // The removal question is up exactly while the rail holds one.
            let removing = heard.read(cx).rail.removing.clone();
            question::follow(removing.as_ref(), &stopping, window, cx);
            // An extension's two questions are up exactly while the session holds them.
            crate::extensions::install::follow(&heard, &stopping, window, cx);
            // The recent-sessions dialog is up exactly while the session holds it.
            crate::agents::install::follow(window, cx);
            cx.notify();
        });
        let running = cx.subscribe_in(&heard, window, |_, _, Ran(action): &Ran, window, cx| {
            run_over(*action, window, cx);
        });
        let labelled = cx.subscribe(&terminal, |_, _, _: &Labelled, cx| cx.notify());
        // A theme that follows the system draws the other when the system goes dark or light.
        let appearance = window.observe_window_appearance(|window, cx| {
            look::system_changed(window, cx);
        });
        Self { heard, caller, rail, work, notice, terminal, #[cfg(test)] split, _observing: observing, _add: add, _labelled: labelled, _appearance: appearance, _running: running }
    }

    #[cfg(test)]
    pub(crate) fn rail(&self) -> &Entity<RailView> {
        &self.rail
    }

    #[cfg(test)]
    pub(crate) fn split(&self) -> &Entity<SplitView> {
        &self.split
    }

    /// What the strip says: Git is named by its title alone; else the stage up, or the pane on
    /// screen, by `StripPage`, with what the stage's own view says (`says`).
    pub fn strip(heard: &Heard, says: &Says, clock: &Clock) -> StripPage {
        if heard.git_up() {
            return StripPage::titled("Git");
        }
        StripPage::of(&heard.rail, heard.console.place == ConsolePlace::Fullscreen, says, clock)
    }

    /// The stage the strip is of: a fullscreen console counts as the Console, whatever is under it.
    fn strip_stage(heard: &Heard) -> Stage {
        if heard.console.place == ConsolePlace::Fullscreen { Stage::Console } else { heard.rail.stage }
    }

    /// How many views agents have shown, whether a dock shows the panel, and what a press of the launcher asks.
    fn views_waiting(heard: &Heard) -> (usize, bool, Option<Call>) {
        let page = heard.views.as_deref();
        let out = head::out(&heard.docks);
        (page.map_or(0, |page| page.views.len()), out, page.and_then(|page| head::launch(page, out)))
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let slots = Slots::of(cx);
        let (strip, stage, held, frame, waiting) = {
            let heard = self.heard.read(cx);
            let stage = Shell::strip_stage(heard);
            let says = slots.says(stage, cx);
            (Shell::strip(heard, &says, &Clock::local()), stage, heard.held(), frame(&heard.rail, heard.console.place), Shell::views_waiting(heard))
        };
        // The grid of the pane on screen: Git and the other stages have none to name.
        let size = if strip.pane.is_some() { self.terminal.read(cx).size_label(cx).to_owned() } else { String::new() };
        // A pane has its `...` menu; another stage has the tools its own view offers.
        let tools = match &strip.pane {
            Some(pane) => Some(more::button(pane, &self.caller, &self.heard, &tokens, cx).into_any_element()),
            None if self.heard.read(cx).git_up() => None,
            None => slots.tools(stage, window, cx),
        };
        let launcher = launcher(waiting.0, waiting.1, waiting.2, held, &self.caller, &tokens).map(IntoElement::into_any_element);
        let scrimmed = !Overlays::get(cx).open_kinds().is_empty();
        h_flex()
            .relative()
            .size_full()
            .text_color(tokens.fg)
            // Filling the window puts the sidebar away; the strip stays, since it holds the window's buttons.
            .when(frame.rail, |window| window.child(self.rail.clone()))
            .child(
                v_flex()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(Header::new().strip(strip).size(size).held(held).launcher(launcher).tools(tools))
                    .child(div().id("stage").flex_1().min_h_0().bg(tokens.see_sunken).child(self.work.clone()))
                    // Over the stage, under nothing: a notice is the last thing drawn.
                    .child(self.notice.clone()),
            )
            // The page's `.scrim`: the kit paints none of its own, so a dialog that is up dims the window here, under the dialog layer.
            .when(scrimmed, |shell| shell.child(div().absolute().top_0().left_0().size_full().bg(tokens.scrim)))
    }
}

#[cfg(test)]
mod tests;
