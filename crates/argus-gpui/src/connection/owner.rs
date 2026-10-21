//! The state the daemon loop and the views' calls share, handed to
//! `connection::effects` with the GPUI window's outlets.
//!
//! The Session stays behind a mutex here, as the Tauri window keeps it: it is
//! `Send`, every change already happens in one `act`, and holding the lock
//! across a change, the messages it sends and the events it publishes is
//! what keeps two threads' changes in order. What GPUI's threading changes
//! is only the way out: the events cannot go to a page's channel, so they go
//! into one the UI thread drains (`connection::heard`), and no tokio thread
//! ever touches a view.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use argus_desktop_core::code::edit::EditEvent;
use argus_desktop_core::connection::daemon::SessionOwner;
use argus_desktop_core::connection::effects::{self, Outlets};
use argus_desktop_core::connection::page::{Call, Event};
use argus_desktop_core::session::Session;
use argus_desktop_core::settings::remembered::{self, Settings};
use argus_desktop_core::system::opener;
use argus_protocol::{ClientMsg, Hello, ServerMsg};
use tokio::sync::{mpsc, watch, Notify};

use crate::system::clipboard;

pub struct Owner {
    session: Mutex<Session>,
    /// The connection's way out; `None` between connections, when what a
    /// view asks of the daemon has nowhere to go.
    out: Mutex<Option<mpsc::UnboundedSender<ClientMsg>>>,
    /// Where the events go, a batch to a send, for the UI thread to drain.
    heard: mpsc::UnboundedSender<Vec<Event>>,
    reconnect: Notify,
    /// The host the daemon loop should be reaching, which it follows.
    host: watch::Sender<Option<String>>,
}

impl Owner {
    /// The owner, and the receiver the UI thread drains. What a fresh page
    /// would be sent, everything, is already waiting in it.
    pub fn new(settings: Settings) -> (Arc<Owner>, mpsc::UnboundedReceiver<Vec<Event>>) {
        let host = settings.host.clone();
        let (heard, listening) = mpsc::unbounded_channel();
        let owner = Owner {
            session: Mutex::new(Session::new(settings)),
            out: Mutex::new(None),
            heard,
            reconnect: Notify::new(),
            host: watch::channel(host).0,
        };
        {
            let mut session = lock(&owner.session);
            let everything = session.everything(now());
            owner.publish(everything);
        }
        (Arc::new(owner), listening)
    }

    /// Changes the session, sends what the change asks of the daemon, and
    /// tells the views what it changed.
    pub fn act(&self, change: impl FnOnce(&mut Session) -> Vec<ClientMsg>) {
        let mut session = lock(&self.session);
        effects::act(
            &mut session,
            now(),
            change,
            &mut Outlets {
                send: &mut |msgs| self.send(msgs),
                publish: &mut |events| self.publish(events),
                // System notifications are a later step's.
                alert: &mut |alert| tracing::debug!("alert not raised: {}", alert.title),
                open: &mut |url| opener::open(url),
                save: &mut |settings| remembered::save(settings),
                clipboard: &clipboard::System,
            },
        );
    }

    /// What a view asks, as the Tauri window's `call` command answers it:
    /// the calls that are the window's or the daemon loop's here, the rest
    /// the session's.
    pub fn call(self: &Arc<Self>, call: Call) {
        // The paste chord reads the clipboard here, unless the leader is
        // waiting and the key is the window's own.
        let chord = matches!(&call, Call::Key { key } if lock(&self.session).pastes(key));
        match call {
            _ if chord => self.paste_clipboard(),
            Call::PasteClipboard => self.paste_clipboard(),
            Call::Reconnect => self.ask_reconnect(),
            Call::SetHost { host } => self.set_host(host),
            Call::OpenUrl { url } => {
                if let Err(said) = opener::open(&url) {
                    self.act(|s| s.note(said));
                }
            }
            Call::Edit { gesture } => self.send(vec![gesture.message()]),
            // The window's own frame answers these natively.
            Call::Minimize | Call::ToggleMaximize | Call::CloseWindow | Call::DragWindow | Call::SnapLayouts => {
                tracing::debug!("a window call reached the owner");
            }
            call => self.act(|s| s.call(call)),
        }
    }

    /// Publishes the shown panes' changed rows, if any changed. The daemon
    /// loop's runtime calls it on a short interval; the views repaint when
    /// the frames arrive, not on that interval.
    pub fn tick(&self) {
        let mut session = lock(&self.session);
        let frames = session.take_frames();
        self.publish(frames);
    }

    /// Whether a window still drains what is published. Its receiver goes
    /// with the window's model, so a closed one is a window that is gone and
    /// whatever is still ticking for it has nobody to tell.
    pub fn listening(&self) -> bool {
        !self.heard.is_closed()
    }

    /// Takes the shown panes' changed rows on a short interval, as the Tauri
    /// window's tick does, and ends once the window stops listening: taking
    /// frames nobody draws would still empty the session's queue.
    pub async fn frames(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_millis(16));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        while self.listening() {
            tick.tick().await;
            self.tick();
        }
    }

    /// Read off the UI thread: the clipboard can name files, and each is
    /// read whole before it is sent.
    fn paste_clipboard(self: &Arc<Self>) {
        let owner = self.clone();
        std::thread::spawn(move || {
            for pasting in effects::pastings(&clipboard::System) {
                owner.act(|s| effects::paste(s, pasting));
            }
        });
    }

    /// Heard only while stopped, so a press at any other time cannot leave
    /// a reconnect waiting for the next stop.
    fn ask_reconnect(&self) {
        if lock(&self.session).is_stopped() {
            self.reconnect.notify_one();
        }
    }

    fn set_host(&self, host: Option<String>) {
        let host = host.map(|host| host.trim().to_owned()).filter(|host| !host.is_empty());
        self.act(|s| s.switch_host(host.clone()));
        *lock(&self.out) = None;
        self.host.send_replace(host);
    }

    fn send(&self, msgs: Vec<ClientMsg>) {
        let out = lock(&self.out);
        let Some(out) = out.as_ref() else {
            return;
        };
        for msg in msgs {
            let _ = out.send(msg);
        }
    }

    fn publish(&self, events: Vec<Event>) {
        if events.is_empty() {
            return;
        }
        // A closed receiver is a window that is gone.
        let _ = self.heard.send(events);
    }
}

impl SessionOwner for Owner {
    fn watch_host(&self) -> watch::Receiver<Option<String>> {
        self.host.subscribe()
    }

    fn act(&self, change: impl FnOnce(&mut Session) -> Vec<ClientMsg>) {
        Owner::act(self, change)
    }

    fn connected(
        &self,
        out: mpsc::UnboundedSender<ClientMsg>,
        daemon: Option<Hello>,
        alarm: Option<String>,
        opening: Vec<ServerMsg>,
    ) {
        *lock(&self.out) = Some(out);
        self.act(|s| s.connected(daemon, alarm, opening));
    }

    fn disconnected(&self) {
        *lock(&self.out) = None;
        self.tell_page(Event::Edit(EditEvent::Lost));
    }

    fn tell_page(&self, event: Event) {
        // Under the session's lock, so it falls between the changes
        // either side of it, as a page's channel would have it.
        let _session = lock(&self.session);
        self.publish(vec![event]);
    }

    async fn reconnect_asked(&self) {
        self.reconnect.notified().await;
    }
}

/// A poisoned lock still holds a usable session: a panic aborts the release
/// build, so this only matters to a test, which should go on.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::Connection;
    use std::time::Duration;

    fn drain(listening: &mut mpsc::UnboundedReceiver<Vec<Event>>) -> Vec<Event> {
        let mut heard = Vec::new();
        while let Ok(batch) = listening.try_recv() {
            heard.extend(batch);
        }
        heard
    }

    async fn waiting(owner: &Owner) -> bool {
        tokio::time::timeout(Duration::from_millis(50), owner.reconnect_asked())
            .await
            .is_ok()
    }

    #[test]
    fn a_new_owner_has_everything_a_fresh_page_would_hear_waiting() {
        let (_owner, mut listening) = Owner::new(Settings::default());
        let heard = drain(&mut listening);
        let status = heard.iter().find_map(|e| match e {
            Event::Status(status) => Some(status),
            _ => None,
        });
        assert_eq!(status.map(|s| s.connection), Some(Connection::Connecting));
        assert!(heard.iter().any(|e| matches!(e, Event::Rail(_))));
    }

    #[tokio::test]
    async fn a_reconnect_is_heard_only_while_stopped() {
        let (owner, mut listening) = Owner::new(Settings::default());
        drain(&mut listening);
        owner.call(Call::Reconnect);
        assert!(!waiting(&owner).await, "a reconnect asked while connecting was kept");
        owner.act(|s| {
            s.stopped();
            Vec::new()
        });
        owner.call(Call::Reconnect);
        assert!(waiting(&owner).await, "a reconnect asked while stopped was lost");
    }

    /// The tick belongs to the window: once the model that drains the
    /// channel is gone it must end, not go on taking frames until the
    /// process does.
    #[tokio::test]
    async fn the_frame_tick_ends_with_the_window_that_listened() {
        let (owner, listening) = Owner::new(Settings::default());
        let ticking = tokio::spawn(owner.clone().frames());
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(!ticking.is_finished(), "the tick stopped while the window still listened");
        assert!(owner.listening());
        drop(listening);
        assert!(!owner.listening());
        let ended = tokio::time::timeout(Duration::from_millis(500), ticking).await;
        assert!(ended.is_ok(), "the tick went on after the window's receiver was dropped");
    }
}
