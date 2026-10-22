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
use std::time::Duration;

use argus_desktop_core::browser::BrowsersPage;
use argus_desktop_core::checkouts::table::Checkouts;
use argus_desktop_core::code::edit::EditEvent;
use argus_desktop_core::code::stage::CodePage;
use argus_desktop_core::code::start::Opened;
use argus_desktop_core::chrome::add_project::AddProjectPage;
use argus_desktop_core::connection::page::{Connection, Event, HistoryPage, Palette, Rail, ReviewPage, SettingsPage, Side, Status, ViewsPage};
use argus_desktop_core::extensions::keys::{ExtCommandPage, ExtGroup};
use argus_desktop_core::extensions::dialog::ExtDialogPage;
use argus_desktop_core::extensions::views::{ExtConfirm, ExtTab, ExtTable, ExtTableConsole};
use argus_desktop_core::keys::keymap::{Action, Command, Group};
use argus_desktop_core::layout::docks::{DocksPage, Panel};
use argus_desktop_core::panes::console::{ConsolePage, ConsolePlace};
use argus_desktop_core::views::nav::{self, NavView};
use argus_desktop_core::views::saved::{SavedPanel, SavedViewsPage};
use gpui_kit::{Context, EventEmitter, Task};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::browser::decoding;
use crate::browser::pictures::Pictures;
use crate::browser::source::{Source, Standalone};
use crate::layout::holders::{Holder, Holders};
use crate::connection::screens::{Marked, Screens};
use crate::panes::screen::Screen;
use crate::review::entry::{Entry, Open};
use crate::views::memory::{self, Memory};

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

/// How long a notice stays: the page's timer (`chrome.ts`, `notice`), where
/// the kit's own default is 5 s.
pub const NOTICE_FOR: Duration = Duration::from_secs(6);

/// One line the window says along its bottom (SHELL-NOTICE-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    /// Which notice it is, counted from the first; a replacement with the
    /// same words has another, so its timer starts over.
    pub serial: u64,
}

/// Emitted by [`Heard`] when a batch of events changed the screen of a side.
pub struct Painted(pub Side);

/// Emitted by [`Heard`] when the picture of a browser's pane changed or went: the places that draw
/// that pane repaint for it and for nothing else, as a terminal does for [`Painted`].
pub struct Pictured(pub u64);

/// Emitted by [`Heard`] for each `Run` the session sent: a key it heard in a pane asked the window
/// to open the palette, the sheet or the settings (SHELL-KEYS-9). The window opens it, since only
/// a window can.
pub struct Ran(pub Action);

/// Emitted by [`Heard`] for each word of the daemon's about a file the editor has open, and for the
/// connection going: the editor's buffer is its own, so these pass through and are held by no one.
pub struct Edited(pub EditEvent);

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
    /// The commands the palette runs, sent with the keymap.
    pub commands: Vec<Command>,
    /// What the palette can go to, while it is open: the session sends it only then, and `open`
    /// false is its word that none is.
    pub palette: Option<Palette>,
    /// The add-a-project dialog, while the session holds it open.
    pub add_project: Option<AddProjectPage>,
    /// The choices of the Settings dialog, sent whenever one changes, whether the dialog is open
    /// or not: the look of the whole window is drawn from them (`settings/look`). Shared, so the
    /// dialog and the look keep them without copying the installed fonts.
    pub settings: Option<Rc<SettingsPage>>,
    /// The sheet's heading of each extension.
    pub extension_groups: Vec<ExtGroup>,
    /// Every extension command the session listed, for the keys that run them (`keys/extension`).
    pub extension_commands: Vec<ExtCommandPage>,
    /// What the sidebar's Extensions fold lists: every view of an extension listed where the person
    /// is, and whether the fold is open (Rust's, `toggle_extensions`).
    pub extension_views: Vec<ExtTab>,
    /// The extension views on screen, each as its table says (their tools ride a dock's strip).
    pub extension_tables: Vec<ExtTable>,
    pub extensions_listed: bool,
    /// The question an extension view's action asks before it is sent, while one waits on a yes.
    pub extension_confirm: Option<ExtConfirm>,
    /// The question an extension put to this window, the oldest, while one is open.
    pub extension_dialog: Option<ExtDialogPage>,
    /// What the sidebar's Views fold lists, and the panels showing saved views (which say which are
    /// lit), as the sidebar lists them. The views' bodies are `saved`'s.
    pub saved_views: Vec<NavView>,
    pub saved_panels: Vec<SavedPanel>,
    /// The views agents showed, whole, as the last event said: newest first, which one the panel shows,
    /// which is a page, and what changed in each. Shared, so a view that keeps them across unrelated
    /// events does not copy them.
    pub views: Option<Rc<ViewsPage>>,
    /// The saved views with their bodies, whole, as the last event said.
    pub saved: Option<Rc<SavedViewsPage>>,
    /// What the window alone remembers of the blocks of those views (the tab being read, a fold, a
    /// half-typed form), by block path. Kept apart from the pages above, which are replaced whenever
    /// an agent shows a view again, so a republish never resets it, nor does a reconnect, which empties the list before the daemon replays it.
    pub memory: memory::Shared,
    /// The review the session holds, while it is open: what the Git stage's
    /// Changes tab draws. Shared, so a view that keeps its rows across
    /// unrelated events does not copy them.
    pub review: Option<Rc<ReviewPage>>,
    /// The history the session holds, while it is open.
    pub history: Option<Rc<HistoryPage>>,
    /// The Checkouts stage's table, whole, as the last event said: `None` until the first, which the
    /// stage draws as loading bars. Shared, so the stage keeps its rows across unrelated events.
    pub checkouts: Option<Rc<Checkouts>>,
    /// The Code stage, whole, as the last event said and kept after the stage goes: the session never
    /// takes it back, and the page keeps what it last drew for the person coming back to it. `open`
    /// false is the stage not being there (four loading bars). Shared, so the stage keeps its rows across
    /// unrelated events.
    pub code: Option<Rc<CodePage>>,
    /// The browsers agents run, as the Browsers stage lists them, whole as the last event said.
    pub browsers: Rc<BrowsersPage>,
    /// Their latest pictures, decoded off the UI thread and shared by every place that draws one.
    pub pictures: Pictures,
    /// The files read in each checkout while this window has been open, for the start panel.
    pub opened: Opened,
    /// What only the window knows of the Git stage (`review::entry`): that Git
    /// was pressed with no checkout to open, and the tab pressed last. Set by
    /// the rail and the stage, as a resize is held by the work area.
    pub git: Entry,
    /// The rows of every side a pane is drawn on, and the selection across them. A screen is
    /// shared with the element drawing it, which holds the last one while the next is applied
    /// (copy on write).
    pub screens: Screens,
    /// The line along the bottom of the window, while one is showing. The
    /// view that draws it ends it after [`NOTICE_FOR`]; a new one replaces it.
    pub notice: Option<Notice>,
    /// The drags holding a resize: a pane waits for them all to end before
    /// reporting its size (SHELL-EDGE-1), as the page marks the body
    /// `resizing`. Held by whoever drags, read by the pane through
    /// [`Heard::resizing`].
    holding: Holders,
    /// How many notices have been heard, to tell one from the next with the same words.
    noticed: u64,
    /// The sides whose screen a frame changed since [`Heard::listen`] last said so.
    painted: Vec<Side>,
    /// The panes a picture was heard of since [`Heard::announce`] last set them decoding, once each.
    shots: Vec<u64>,
    /// Where a pane's decoded bytes come from, and the store behind it when no session is.
    source: Source,
    standalone: Option<Standalone>,
    /// The panes whose picture went when the stage was told, which their places are to hear of.
    blanked: Vec<u64>,
    /// The actions the session asked the window to run since [`Heard::listen`] last said so.
    ran: Vec<Action>,
    /// The editor's events heard since [`Heard::listen`] last said so.
    edited: Vec<EditEvent>,
    /// The loop that drains the channel; held so it runs as long as this does.
    listening: Option<Task<()>>,
}

impl Heard {
    /// Hears nothing until [`Heard::listen`] is given a channel.
    pub fn new() -> Self {
        let standalone = Standalone::default();
        Self::with(standalone.source(), Some(standalone))
    }

    /// A model whose pictures come from the session's bytes (`Source`), not from the shots it hears.
    pub fn fed_by(source: Source) -> Self {
        Self::with(source, None)
    }

    fn with(source: Source, standalone: Option<Standalone>) -> Self {
        Self {
            status: None,
            rail: Rail::default(),
            docks: DocksPage::default(),
            console: ConsolePage::default(),
            keymap: Vec::new(),
            commands: Vec::new(),
            palette: None,
            add_project: None,
            settings: None,
            extension_groups: Vec::new(),
            extension_commands: Vec::new(),
            extension_views: Vec::new(),
            extension_tables: Vec::new(),
            extensions_listed: false,
            extension_confirm: None,
            extension_dialog: None,
            saved_views: Vec::new(),
            saved_panels: Vec::new(),
            views: None,
            saved: None,
            memory: Memory::shared(),
            review: None,
            history: None,
            checkouts: None,
            code: None,
            browsers: Rc::default(),
            pictures: Pictures::default(),
            opened: Opened::default(),
            git: Entry::default(),
            screens: Screens::new(),
            notice: None,
            holding: Holders::default(),
            noticed: 0,
            painted: Vec::new(),
            shots: Vec::new(),
            source,
            standalone,
            blanked: Vec::new(),
            ran: Vec::new(),
            edited: Vec::new(),
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
                    heard.announce(cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
    }

    /// Says what a batch left for the views to be told apart from a redraw: the sides whose screen
    /// changed, and the actions the session asked the window to run. [`Heard::listen`] does it after
    /// each batch; a harness that hears events directly does it once it has built its window.
    pub fn announce(&mut self, cx: &mut Context<Self>) {
        for side in std::mem::take(&mut self.painted) {
            cx.emit(Painted(side));
        }
        for pane in std::mem::take(&mut self.blanked) {
            cx.emit(Pictured(pane));
        }
        for pane in std::mem::take(&mut self.shots) {
            decoding::begin(self, pane, cx);
        }
        for action in std::mem::take(&mut self.ran) {
            cx.emit(Ran(action));
        }
        for event in std::mem::take(&mut self.edited) {
            cx.emit(Edited(event));
        }
    }

    /// Takes in one event; whether it changed what is held.
    pub fn hear(&mut self, event: Event) -> bool {
        match event {
            Event::Status(status) => self.status = Some(status),
            Event::Rail(rail) => self.rail = *rail,
            Event::Docks(docks) => self.docks = *docks,
            Event::Console(console) => self.console = *console,
            Event::Keymap { groups, commands, .. } => {
                self.keymap = groups;
                self.commands = commands;
            }
            Event::Palette(page) => self.palette = page.open.then_some(*page),
            Event::AddProject(page) => self.add_project = page.open.then_some(*page),
            Event::Settings(page) => self.settings = Some(Rc::new(*page)),
            Event::Extensions(page) => {
                self.extension_groups = page.groups;
                self.extension_commands = page.commands;
                self.extension_views = page.views;
                self.extension_tables = page.tables;
                self.extensions_listed = page.listed;
                self.extension_confirm = page.confirm;
            }
            // Held only while it is open: `open` false is the session's word that none is.
            Event::ExtensionDialog(page) => self.extension_dialog = page.open.then_some(*page),
            Event::SavedViews(page) => {
                self.saved_views = nav::views_of(&page);
                self.saved_panels = page.panels.clone();
                self.saved = Some(Rc::new(*page));
            }
            Event::Views(page) => {
                self.views = Some(Rc::new(*page));
            }
            // Said once and held by no one: the window opens what it names (`Ran`).
            Event::Run { action } => {
                self.ran.push(action);
                return false;
            }
            // The editor's, said once and held by no one (`Edited`).
            Event::Edit(event) => {
                self.edited.push(event);
                return false;
            }
            // The newest replaces the one showing, even with the same words.
            Event::Notice { text } => {
                self.noticed += 1;
                self.notice = Some(Notice { text, serial: self.noticed });
            }
            Event::Browsers(page) => {
                for pane in self.pictures.page(&page) {
                    self.blanked.push(pane);
                    if let Some(held) = &self.standalone {
                        held.closed(pane);
                    }
                }
                self.browsers = Rc::new(*page);
            }
            // Decoded once the batch is over (`announce`), and only the newest of a pane's.
            Event::BrowserShot(shot) => {
                if let Some(held) = &self.standalone {
                    held.feed(&shot);
                }
                if !self.shots.contains(&shot.pane) {
                    self.shots.push(shot.pane);
                }
                return false;
            }
            Event::Checkouts(table) => self.checkouts = Some(Rc::new(*table)),
            Event::Code(page) => {
                self.opened.remember(&page);
                self.code = Some(Rc::new(*page));
            }
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
            // A frame for a side with no screen is dropped, as a page drops one (`connection/screens`).
            Event::Frame(frame) => {
                if let Some(side) = self.screens.apply(frame) {
                    if !self.painted.contains(&side) {
                        self.painted.push(side);
                    }
                }
                return false;
            }
            _ => return false,
        }
        true
    }

    /// Puts the notice away, if it is still the one `serial` named: a timer
    /// set for an earlier notice must not take down the one that replaced it.
    /// Whether it did.
    pub fn dismiss_notice(&mut self, serial: u64) -> bool {
        let same = self.notice.as_ref().is_some_and(|notice| notice.serial == serial);
        if same {
            self.notice = None;
        }
        same
    }

    /// Whether any drag holds a resize: a pane reports its size when none does.
    pub fn resizing(&self) -> bool {
        self.holding.any()
    }

    /// Holds a resize for `holder`; whether anything changed.
    pub fn hold_resize(&mut self, holder: Holder) -> bool {
        self.holding.hold(holder)
    }

    /// Lets `holder`'s hold go, leaving the others'; whether anything changed.
    pub fn release_resize(&mut self, holder: Holder) -> bool {
        self.holding.release(holder)
    }

    /// Whether a dock showing `panel` has its element on screen: the console
    /// is on the stage instead while it is a tab, fullscreen or hidden
    /// (`layout/docks.ts`, `element`), and the dock is closed then.
    pub fn on_screen_in_dock(&self, panel: &Panel) -> bool {
        *panel != Panel::Console || self.console.place == ConsolePlace::Docked
    }

    /// The screen a side draws.
    pub fn screen_of(&self, side: Side) -> &Arc<Screen> {
        self.screens.of(side)
    }

    /// Moves the selection to `marked`, or takes it away, and says to the side it left and the side it
    /// came to that their picture changed. Not a `notify`: the rail and the foot have nothing new to
    /// draw while a drag goes on.
    pub fn mark(&mut self, marked: Option<Marked>, cx: &mut Context<Self>) {
        for side in self.screens.mark(marked) {
            cx.emit(Painted(side));
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
            Side::Beside(n) => self.rail.row.beside.get(usize::from(n)).and_then(Option::as_ref).map(|shown| shown.pane),
            Side::View(_) => self.view_console(side).and_then(|console| console.pane),
            _ => self.rail.shown.as_ref().map(|shown| shown.pane),
        }
    }

    /// The console an extension view on screen draws on `side`, as its table says.
    pub fn view_console(&self, side: Side) -> Option<&ExtTableConsole> {
        self.extension_tables.iter().filter_map(|table| table.console.as_ref()).find(|console| console.side == Some(side))
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

impl Heard {
    /// The source of decoded picture bytes, for a decode that runs off the UI thread.
    pub fn source(&self) -> Source {
        self.source.clone()
    }
}

impl EventEmitter<Pictured> for Heard {}
impl EventEmitter<Ran> for Heard {}
impl EventEmitter<Edited> for Heard {}

impl Default for Heard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::memory::Draft;
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
    fn a_run_is_kept_for_the_window_to_open_and_changes_nothing_a_view_draws() {
        let mut heard = Heard::new();
        assert!(!heard.hear(Event::Run { action: Action::Shortcuts }));
        assert!(!heard.hear(Event::Run { action: Action::Palette }));
        assert_eq!(heard.ran, [Action::Shortcuts, Action::Palette]);
    }

    #[test]
    fn an_event_no_view_draws_yet_changes_nothing() {
        let mut heard = Heard::new();
        assert!(!heard.hear(Event::Chime));
    }

    #[test]
    fn a_notice_is_held_and_the_next_replaces_it_even_with_the_same_words() {
        let mut heard = Heard::new();
        assert!(heard.hear(Event::Notice { text: "no pane is on screen".into() }));
        let first = heard.notice.clone().expect("held");
        assert_eq!(first.text, "no pane is on screen");
        assert!(heard.hear(Event::Notice { text: "no pane is on screen".into() }));
        let second = heard.notice.clone().expect("held");
        assert_ne!(first.serial, second.serial, "the same words are a notice of their own");
        heard.hear(Event::Notice { text: "could not copy".into() });
        assert_eq!(heard.notice.as_ref().map(|n| n.text.as_str()), Some("could not copy"));
    }

    #[test]
    fn a_timer_for_an_earlier_notice_leaves_the_one_that_replaced_it() {
        let mut heard = Heard::new();
        heard.hear(Event::Notice { text: "one".into() });
        let earlier = heard.notice.as_ref().unwrap().serial;
        heard.hear(Event::Notice { text: "two".into() });
        assert!(!heard.dismiss_notice(earlier));
        assert_eq!(heard.notice.as_ref().map(|n| n.text.as_str()), Some("two"));
        let current = heard.notice.as_ref().unwrap().serial;
        assert!(heard.dismiss_notice(current));
        assert!(heard.notice.is_none());
        assert!(!heard.dismiss_notice(current), "nothing left to put away");
    }

    #[test]
    fn a_resize_is_held_until_every_holder_has_let_go() {
        let mut heard = Heard::new();
        assert!(!heard.resizing());
        heard.hold_resize(Holder::Rail);
        heard.hold_resize(Holder::Grid);
        heard.release_resize(Holder::Rail);
        assert!(heard.resizing(), "the grid's drag still holds it");
        heard.release_resize(Holder::Grid);
        assert!(!heard.resizing());
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
        assert_eq!(heard.screen_of(Side::Workspace).pane(), Some(1));
        assert_eq!(heard.painted, [Side::Workspace]);
    }

    #[test]
    fn a_frame_for_another_side_is_dropped() {
        use argus_desktop_core::connection::page::Side;
        let mut heard = Heard::new();
        for side in [Side::Beside(0), Side::View(0)] {
            heard.hear(frame(side));
        }
        assert_eq!(heard.screen_of(Side::Workspace).pane(), None);
        assert!(heard.painted.is_empty());
    }

    #[test]
    fn a_consoles_frames_are_held_for_their_own_side_and_say_which_painted() {
        use argus_desktop_core::connection::page::Side;
        let mut heard = Heard::new();
        heard.hear(frame(Side::First));
        heard.hear(frame(Side::Second));
        heard.hear(frame(Side::Second));
        assert_eq!(
            (heard.screen_of(Side::First).pane(), heard.screen_of(Side::Second).pane(), heard.screen_of(Side::Workspace).pane()),
            (Some(1), Some(1), None)
        );
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

    fn page_view(pane: u64, id: &str) -> argus_desktop_core::connection::page::PageView {
        argus_desktop_core::connection::page::PageView {
            pane,
            from: "claude".into(),
            at: 0,
            refreshing: false,
            pressed: Vec::new(),
            subject: None,
            view: argus_protocol::AgentView { id: id.into(), title: id.into(), ..Default::default() },
            diagrams: Vec::new(),
            changed: Vec::new(),
            unseen: false,
        }
    }

    fn views(shown: &[(u64, &str)]) -> Event {
        Event::Views(Box::new(ViewsPage { views: shown.iter().map(|(pane, id)| page_view(*pane, id)).collect(), ..ViewsPage::default() }))
    }

    #[test]
    fn the_views_agents_showed_are_held_whole_and_replaced_by_the_next_event() {
        let mut heard = Heard::new();
        assert!(heard.views.is_none());
        assert!(heard.hear(views(&[(1, "ci"), (2, "trim")])));
        assert_eq!(heard.views.as_ref().map(|page| page.views.len()), Some(2));
        assert!(heard.hear(views(&[(1, "ci")])));
        assert_eq!(heard.views.as_ref().map(|page| page.views.len()), Some(1), "whole each time, never merged");
    }

    #[test]
    fn what_the_window_remembers_of_a_view_survives_the_agent_showing_it_again() {
        let mut heard = Heard::new();
        heard.hear(views(&[(1, "ci")]));
        heard.memory.borrow_mut().choose_tab("1/ci.2", 3);
        heard.memory.borrow_mut().set_draft("1/ci.4|title", Draft::Text("half typed".into()));
        heard.hear(views(&[(1, "ci")]));
        assert_eq!(heard.memory.borrow().tab("1/ci.2", 5), 3, "a republish must not put the tab back");
        assert_eq!(heard.memory.borrow().draft("1/ci.4|title"), Some(&Draft::Text("half typed".into())));
    }

    #[test]
    fn what_the_window_remembers_of_a_view_survives_a_reconnect_that_empties_the_list() {
        let mut heard = Heard::new();
        heard.hear(views(&[(1, "ci")]));
        heard.memory.borrow_mut().set_draft("1/ci.4|title", Draft::Text("half typed".into()));
        heard.hear(views(&[]));
        heard.hear(views(&[(1, "ci")]));
        assert_eq!(heard.memory.borrow().draft("1/ci.4|title"), Some(&Draft::Text("half typed".into())));
    }

    #[test]
    fn the_saved_views_are_held_with_their_bodies_beside_the_sidebars_list_of_them() {
        let mut heard = Heard::new();
        assert!(heard.saved.is_none());
        let page = SavedViewsPage { can_press: true, ..SavedViewsPage::default() };
        assert!(heard.hear(Event::SavedViews(Box::new(page))));
        assert_eq!(heard.saved.as_ref().map(|page| page.can_press), Some(true));
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
