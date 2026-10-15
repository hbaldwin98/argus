//! What the page's calls, the daemon's messages and the frame tick share:
//! the session, the way out to the daemon, the way to the page, and the
//! app, which raises the system's notifications.
//!
//! Each change happens under the session's lock, and so do the messages it
//! sends and the events it publishes, so two changes on two threads reach
//! the daemon and the page in the order they were made.

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use argus_protocol::{ClientMsg, Hello, ServerMsg};
use tauri::ipc::Channel;
use tauri::AppHandle;
use tokio::sync::{mpsc, Notify};

use crate::notifications;
use crate::page::Event;
use crate::session::Session;
use crate::settings::{self, Settings};

pub struct Shared {
    session: Mutex<Session>,
    /// The connection's way out; `None` between connections, when what the
    /// page asks of the daemon has nowhere to go.
    out: Mutex<Option<mpsc::UnboundedSender<ClientMsg>>>,
    /// The page's channel, while a page is loaded.
    page: Mutex<Option<Channel<Event>>>,
    /// Set once the app is built: a tree that arrives before then raises
    /// nothing, and the first tree is quiet anyway.
    app: OnceLock<AppHandle>,
    reconnect: Notify,
}

impl Shared {
    pub fn new(settings: Settings) -> Shared {
        Shared {
            session: Mutex::new(Session::new(settings)),
            out: Mutex::new(None),
            page: Mutex::new(None),
            app: OnceLock::new(),
            reconnect: Notify::new(),
        }
    }

    /// Changes the session, sends what the change asks of the daemon, and
    /// tells the page what it changed.
    pub fn act(&self, change: impl FnOnce(&mut Session) -> Vec<ClientMsg>) {
        let mut session = lock(&self.session);
        let msgs = change(&mut session);
        self.send(msgs);
        let events = session.events(now());
        self.publish(events);
        if let Some(settings) = session.take_settings() {
            settings::save(&settings);
        }
        let alerts = session.take_alerts();
        if let Some(app) = self.app.get() {
            for alert in &alerts {
                notifications::show(app, alert);
            }
        }
    }

    pub fn set_app(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }

    /// Reads the session without changing it.
    pub fn read<R>(&self, look: impl FnOnce(&Session) -> R) -> R {
        look(&lock(&self.session))
    }

    /// A connection is up: what the page asks goes over `out` from now on.
    pub fn connected(
        &self,
        out: mpsc::UnboundedSender<ClientMsg>,
        daemon: Option<Hello>,
        alarm: Option<String>,
        opening: Vec<ServerMsg>,
    ) {
        *lock(&self.out) = Some(out);
        self.act(|s| s.connected(daemon, alarm, opening));
    }

    pub fn disconnected(&self) {
        *lock(&self.out) = None;
    }

    /// A page has loaded and holds nothing: it hears everything.
    pub fn attach_page(&self, page: Channel<Event>) {
        let mut session = lock(&self.session);
        *lock(&self.page) = Some(page);
        let events = session.everything(now());
        self.publish(events);
    }

    /// The page is reloading. A channel to a page that is gone keeps taking
    /// sends without complaint, so it is dropped here, not on an error.
    pub fn detach_page(&self) {
        *lock(&self.page) = None;
    }

    /// Sends the shown panes' changed rows, if any changed.
    pub fn tick(&self) {
        let mut session = lock(&self.session);
        let frames = session.take_frames();
        if !frames.is_empty() {
            self.publish(frames);
        }
    }

    /// The page asked to reconnect. Heard only while stopped, so a press at
    /// any other time cannot leave a reconnect waiting for the next stop.
    pub fn ask_reconnect(&self) {
        if lock(&self.session).is_stopped() {
            self.reconnect.notify_one();
        }
    }

    pub async fn reconnect_asked(&self) {
        self.reconnect.notified().await;
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
        let page = lock(&self.page);
        let Some(page) = page.as_ref() else {
            return;
        };
        for event in events {
            if let Err(err) = page.send(event) {
                tracing::debug!("the page did not take an event: {err}");
            }
        }
    }
}

/// A poisoned lock still holds a usable session: a panic aborts the
/// release build, so this only matters to a test, which should go on.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
