//! What the window has heard from its session: the latest of each kind of
//! event a view draws, held in one model every view observes.
//!
//! The events arrive in batches over the channel `connection::owner` fills,
//! drained here on the UI thread. A kind is held only once a view draws it,
//! so a later surface adds its own field and arm; until then its events are
//! dropped on arrival, which is safe because every surface but the terminal
//! is sent whole each time it changes.
//!
//! The terminal is the exception: its frames arrive many times a second and
//! only the pane on screen draws them, so they do not notify the views that
//! draw the rest. They are applied to the held screen, and a batch that
//! changed it emits [`Painted`], which the pane's view alone listens for.

use std::sync::Arc;

use argus_desktop_core::connection::page::{Connection, Event, Rail, Side, Status};
use gpui_kit::{Context, EventEmitter, Task};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::panes::screen::Screen;

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

/// Emitted by [`Heard`] when a batch of events changed the Workspace pane's
/// screen.
pub struct Painted;

pub struct Heard {
    pub status: Option<Status>,
    pub rail: Rail,
    /// The Workspace pane's rows. Shared with the element drawing them, which
    /// holds the last one while the next is applied (copy on write).
    pub screen: Arc<Screen>,
    /// A frame changed `screen` since [`Heard::listen`] last said so.
    painted: bool,
    /// The loop that drains the channel; held so it runs as long as this does.
    listening: Option<Task<()>>,
}

impl Heard {
    /// Hears nothing until [`Heard::listen`] is given a channel.
    pub fn new() -> Self {
        Self {
            status: None,
            rail: Rail::default(),
            screen: Arc::new(Screen::new()),
            painted: false,
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
                    if std::mem::take(&mut heard.painted) {
                        cx.emit(Painted);
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
            // Only the Workspace's side is drawn so far; the console's and
            // the splits' frames are dropped, as a page drops one for a side
            // it has no screen for.
            Event::Frame(frame) if frame.side == Side::Workspace => {
                Arc::make_mut(&mut self.screen).apply(frame);
                self.painted = true;
                return false;
            }
            _ => return false,
        }
        true
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
        assert!(heard.painted);
    }

    #[test]
    fn a_frame_for_another_side_is_dropped() {
        use argus_desktop_core::connection::page::Side;
        let mut heard = Heard::new();
        for side in [Side::First, Side::Second, Side::Beside(0), Side::View(0)] {
            heard.hear(frame(side));
        }
        assert_eq!(heard.screen.pane(), None);
        assert!(!heard.painted);
    }
}
