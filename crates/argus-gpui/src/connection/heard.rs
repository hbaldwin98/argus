//! What the window has heard from its session: the latest of each kind of
//! event a view draws, held in one model every view observes.
//!
//! The events arrive in batches over the channel `connection::owner` fills,
//! drained here on the UI thread. A kind is held only once a view draws it,
//! so a later surface adds its own field and arm; until then its events are
//! dropped on arrival, which is safe because every surface but the terminal
//! is sent whole each time it changes.
//!
//! The terminals are the exception: their frames arrive many times a second
//! and only the panes on screen draw them, so they do not notify the views
//! that draw the rest. A frame is applied to the screen of its side (the
//! Workspace's, and the console's first and second), and a batch that
//! changed any emits a [`Painted`] for each side it changed, which the
//! terminal of that side alone repaints for.

use std::rc::Rc;
use std::sync::Arc;

use argus_desktop_core::connection::page::{Connection, Event, HistoryPage, Rail, ReviewPage, Side, Status};
use argus_desktop_core::keys::keymap::Group;
use argus_desktop_core::layout::docks::{DocksPage, Panel};
use argus_desktop_core::panes::console::{ConsolePage, ConsolePlace};
use gpui_kit::{Context, EventEmitter, Task};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::panes::screen::Screen;
use crate::review::entry::{Entry, Open};

/// How the foot says the daemon is reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// No tree heard yet.
    Connecting,
    /// Connecting again after a tree was heard: the tree on screen is the
    /// last daemon's.
    Reconnecting,
    Connected { host: Option<String> },
    Stopped,
}

/// Emitted by [`Heard`] when a batch of events changed the screen of a side.
pub struct Painted(pub Side);

pub struct Heard {
    pub status: Option<Status>,
    pub rail: Rail,
    /// Where each panel sits and which one each dock shows: the last the
    /// session said, which the docks draw and never change themselves.
    pub docks: DocksPage,
    /// The console as the page draws it: where it is (a dock draws it only
    /// while it is `Docked`, as the page puts its element in the dock only
    /// then, SHELL-DOCK-9), its tabs, and which consoles its sides show.
    pub console: ConsolePage,
    /// Every binding the session sent, for a menu to show the keys beside what it does.
    pub keymap: Vec<Group>,
    /// The review the session holds, while it is open: what the Git stage's
    /// Changes tab draws. Shared, so a view that keeps its rows across
    /// unrelated events does not copy them.
    pub review: Option<Rc<ReviewPage>>,
    /// The history the session holds, while it is open.
    pub history: Option<Rc<HistoryPage>>,
    /// What only the window knows of the Git stage (`review::entry`): that Git
    /// was pressed with no checkout to open, and the tab pressed last. Set by
    /// the rail and the stage, as `resizing` is by the work area.
    pub git: Entry,
    /// The Workspace pane's rows. Shared with the element drawing them, which
    /// holds the last one while the next is applied (copy on write).
    pub screen: Arc<Screen>,
    /// The console's first side's rows, and its second's while it is split.
    pub first: Arc<Screen>,
    pub second: Arc<Screen>,
    /// A dock's sash is held: the stage is being resized by a gesture, and a
    /// pane waits for it to end before reporting its size (SHELL-EDGE-1), as the
    /// page marks the body `resizing`. Set by the work area, read by the pane.
    pub resizing: bool,
    /// The sides whose screen a frame changed since [`Heard::listen`] last said so.
    painted: Vec<Side>,
    /// The loop that drains the channel; held so it runs as long as this does.
    listening: Option<Task<()>>,
}

impl Heard {
    /// Hears nothing until [`Heard::listen`] is given a channel.
    pub fn new() -> Self {
        Self {
            status: None,
            rail: Rail::default(),
            docks: DocksPage::default(),
            console: ConsolePage::default(),
            keymap: Vec::new(),
            review: None,
            history: None,
            git: Entry::default(),
            screen: Arc::new(Screen::new()),
            first: Arc::new(Screen::new()),
            second: Arc::new(Screen::new()),
            resizing: false,
            painted: Vec::new(),
            listening: None,
        }
    }

    /// Drains `events` into this model, a batch at a time, and tells the
    /// views each batch that changed something.
    pub fn listen(&mut self, mut events: UnboundedReceiver<Vec<Event>>, cx: &mut Context<Self>) {
        self.listening = Some(cx.spawn(async move |this, cx| {
            while let Some(batch) = events.recv().await {
                let alive = this.update(cx, |heard, cx| {
                    let mut changed = false;
                    for event in batch {
                        changed |= heard.hear(event);
                    }
                    if changed {
                        cx.notify();
                    }
                    for side in std::mem::take(&mut heard.painted) {
                        cx.emit(Painted(side));
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
    }

    /// Takes in one event; whether it changed what is held.
    pub fn hear(&mut self, event: Event) -> bool {
        match event {
            Event::Status(status) => self.status = Some(status),
            Event::Rail(rail) => self.rail = *rail,
            Event::Docks(docks) => self.docks = *docks,
            Event::Console(console) => self.console = *console,
            Event::Keymap { groups, .. } => self.keymap = groups,
            // A dialog is held only while it is open: `open` false is the session's word that it is not.
            Event::Review(page) => {
                if page.open {
                    self.git.opened(page.checkout);
                }
                self.review = page.open.then(|| Rc::new(*page));
            }
            Event::History(page) => {
                if page.open {
                    self.git.history_opened();
                }
                self.history = page.open.then(|| Rc::new(*page));
            }
            // The Workspace's side and the console's two are drawn; the
            // splits' and the views' frames are dropped, as a page drops one
            // for a side it has no screen for.
            Event::Frame(frame) if matches!(frame.side, Side::Workspace | Side::First | Side::Second) => {
                let side = frame.side;
                let screen = match side {
                    Side::First => &mut self.first,
                    Side::Second => &mut self.second,
                    _ => &mut self.screen,
                };
                Arc::make_mut(screen).apply(frame);
                if !self.painted.contains(&side) {
                    self.painted.push(side);
                }
                return false;
            }
            _ => return false,
        }
        true
    }

    /// Whether a dock showing `panel` has its element on screen: the console
    /// is on the stage instead while it is a tab, fullscreen or hidden
    /// (`layout/docks.ts`, `element`), and the dock is closed then.
    pub fn on_screen_in_dock(&self, panel: &Panel) -> bool {
        *panel != Panel::Console || self.console.place == ConsolePlace::Docked
    }

    /// The screen a side draws.
    pub fn screen_of(&self, side: Side) -> &Arc<Screen> {
        match side {
            Side::First => &self.first,
            Side::Second => &self.second,
            _ => &self.screen,
        }
    }

    /// The pane a side is on screen to draw: the one the rail says is shown
    /// for the Workspace, the console each of the console's sides shows. The
    /// old rows of another pane are not that pane's, so a view draws a
    /// screen only while it is of this one.
    pub fn pane_of(&self, side: Side) -> Option<u64> {
        match side {
            Side::First => self.console.showing,
            Side::Second => self.console.beside,
            _ => self.rail.shown.as_ref().map(|shown| shown.pane),
        }
    }

    /// Whether the console says its keys are in `pane`.
    pub fn keyed(&self, pane: Option<u64>) -> bool {
        pane.is_some() && self.console.typing == pane
    }

    /// What the session has open of the Git stage.
    pub fn open(&self) -> Open {
        Open { review: self.review.is_some(), history: self.history.is_some() }
    }

    /// Whether the Git stage is up: the work area draws it in the pane's place.
    pub fn git_up(&self) -> bool {
        self.git.up(self.open())
    }

    pub fn connection(&self) -> Connection {
        self.status.as_ref().map_or(Connection::Connecting, |s| s.connection)
    }

    pub fn link(&self) -> Link {
        match self.connection() {
            Connection::Connecting if self.rail.loaded => Link::Reconnecting,
            Connection::Connecting => Link::Connecting,
            Connection::Connected => Link::Connected { host: self.status.as_ref().and_then(|s| s.host.clone()) },
            Connection::Stopped => Link::Stopped,
        }
    }

    /// Whether what the sidebar lists is only what the last daemon said:
    /// dimmed and answering no press, since nothing a press sends would
    /// reach a daemon (decision 49 of `GPUI.md`; the page dimmed only a
    /// stopped one).
    pub fn held(&self) -> bool {
        matches!(self.link(), Link::Reconnecting | Link::Stopped)
    }
}

impl EventEmitter<Painted> for Heard {}

impl Default for Heard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{AppContext as _, TestAppContext};

    fn status(connection: Connection) -> Event {
        Event::Status(Status { connection, detail: None, alarm: None, chime: true, host: None })
    }

    fn loaded() -> Event {
        Event::Rail(Box::new(Rail { loaded: true, ..Rail::default() }))
    }

    #[test]
    fn a_window_that_has_heard_nothing_is_connecting_and_not_held() {
        let heard = Heard::new();
        assert_eq!(heard.link(), Link::Connecting);
        assert!(!heard.held());
    }

    #[test]
    fn connecting_after_a_tree_is_reconnecting_and_holds_the_rail() {
        let mut heard = Heard::new();
        heard.hear(status(Connection::Connected));
        heard.hear(loaded());
        assert_eq!(heard.link(), Link::Connected { host: None });
        assert!(!heard.held());
        heard.hear(status(Connection::Connecting));
        assert_eq!(heard.link(), Link::Reconnecting);
        assert!(heard.held());
    }

    #[test]
    fn stopped_holds_the_rail() {
        let mut heard = Heard::new();
        heard.hear(status(Connection::Stopped));
        assert_eq!(heard.link(), Link::Stopped);
        assert!(heard.held());
    }

    #[test]
    fn an_event_no_view_draws_yet_changes_nothing() {
        let mut heard = Heard::new();
        assert!(!heard.hear(Event::Chime));
        assert!(!heard.hear(Event::Notice { text: "x".into() }), "a notice is held once a view draws one");
    }

    fn frame(side: argus_desktop_core::connection::page::Side) -> Event {
        use argus_desktop_core::connection::page::{Frame, FrameCursor};
        Event::Frame(Frame {
            pane: 1,
            side,
            rows: 1,
            cols: 4,
            full: true,
            cursor: FrameCursor { row: 0, col: 0, visible: true, shape: "block" },
            alternate_screen: false,
            mouse: false,
            lines: Vec::new(),
            parked: None,
        })
    }

    #[test]
    fn a_workspace_frame_is_applied_without_telling_the_views_that_draw_the_rest() {
        use argus_desktop_core::connection::page::Side;
        let mut heard = Heard::new();
        assert!(!heard.hear(frame(Side::Workspace)), "the rail and the foot have nothing new to draw");
        assert_eq!(heard.screen.pane(), Some(1));
        assert_eq!(heard.painted, [Side::Workspace]);
    }

    #[test]
    fn a_frame_for_another_side_is_dropped() {
        use argus_desktop_core::connection::page::Side;
        let mut heard = Heard::new();
        for side in [Side::Beside(0), Side::View(0)] {
            heard.hear(frame(side));
        }
        assert_eq!(heard.screen.pane(), None);
        assert!(heard.painted.is_empty());
    }

    #[test]
    fn a_consoles_frames_are_held_for_their_own_side_and_say_which_painted() {
        use argus_desktop_core::connection::page::Side;
        let mut heard = Heard::new();
        heard.hear(frame(Side::First));
        heard.hear(frame(Side::Second));
        heard.hear(frame(Side::Second));
        assert_eq!((heard.first.pane(), heard.second.pane(), heard.screen.pane()), (Some(1), Some(1), None));
        assert_eq!(heard.painted, [Side::First, Side::Second], "each side once");
    }

    #[test]
    fn a_side_names_the_pane_it_is_on_screen_to_draw() {
        use argus_desktop_core::connection::page::Side;
        let mut heard = Heard::new();
        let page = ConsolePage { showing: Some(5), beside: Some(6), typing: Some(6), ..ConsolePage::default() };
        heard.hear(Event::Console(Box::new(page)));
        assert_eq!(
            (heard.pane_of(Side::First), heard.pane_of(Side::Second), heard.pane_of(Side::Workspace)),
            (Some(5), Some(6), None)
        );
        assert!(heard.keyed(Some(6)) && !heard.keyed(Some(5)) && !heard.keyed(None));
    }

    /// The loop is held by the model it fills, so it cannot outlive the
    /// window whose views were that model's only holders: dropping the model
    /// drops the receiver, which is how the owner's frame tick learns the
    /// window is gone (`Owner::listening`).
    #[gpui_kit::test]
    fn the_drain_loop_ends_with_the_model_it_fills(cx: &mut TestAppContext) {
        let (events, listening) = tokio::sync::mpsc::unbounded_channel::<Vec<Event>>();
        let heard = cx.new(|_| Heard::new());
        heard.update(cx, |heard, cx| heard.listen(listening, cx));
        cx.run_until_parked();
        assert!(!events.is_closed(), "the loop ended while its model was alive");
        drop(heard);
        // A model is released in the effect cycle after its last handle goes.
        cx.update(|_| {});
        cx.run_until_parked();
        assert!(events.is_closed(), "the loop outlived its model");
    }
}
