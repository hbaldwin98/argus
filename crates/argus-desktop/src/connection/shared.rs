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
use tokio::sync::{mpsc, watch, Notify};

use crate::system::clipboard;
use crate::system::toast;
use argus_desktop_core::code::edit::EditEvent;
use argus_desktop_core::connection::daemon::SessionOwner;
use argus_desktop_core::connection::effects::{self, Outlets};
use argus_desktop_core::connection::page::Event;
use argus_desktop_core::session::Session;
use argus_desktop_core::settings::remembered::{self, Settings};
use argus_desktop_core::system::opener;

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
    /// The host the daemon loop should be reaching, which it follows.
    host: watch::Sender<Option<String>>,
}

impl Shared {
    pub fn new(settings: Settings) -> Shared {
        let host = settings.host.clone();
        Shared {
            session: Mutex::new(Session::new(settings)),
            out: Mutex::new(None),
            page: Mutex::new(None),
            app: OnceLock::new(),
            reconnect: Notify::new(),
            host: watch::channel(host).0,
        }
    }

    /// Changes the session, sends what the change asks of the daemon, and
    /// tells the page what it changed.
    pub fn act(&self, change: impl FnOnce(&mut Session) -> Vec<ClientMsg>) {
        let mut session = lock(&self.session);
        let app = self.app.get();
        effects::act(
            &mut session,
            now(),
            change,
            &mut Outlets {
                send: &mut |msgs| self.send(msgs),
                publish: &mut |events| self.publish(events),
                alert: &mut |alert| {
                    if let Some(app) = app {
                        toast::show(app, alert);
                    }
                },
                open: &mut |url| opener::open(url),
                save: &mut |settings| remembered::save(settings),
                clipboard: &clipboard::System,
            },
        );
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
        self.tell_page(Event::Edit(EditEvent::Lost));
    }

    /// Puts the editor's ask to the daemon without the session: nothing in
    /// it changes, and every page it holds would be built again to say so.
    pub fn relay(&self, msg: ClientMsg) {
        self.send(vec![msg]);
    }

    /// Tells the page what the daemon said of a file its editor has open,
    /// past the session as `relay` is.
    pub fn tell_page(&self, event: Event) {
        self.publish(vec![event]);
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
    /// Reaches the daemon on `host`, or this machine's. The session forgets
    /// the last one at once; the connection loop drops its bridge and
    /// starts another.
    pub fn set_host(&self, host: Option<String>) {
        let host = host.map(|host| host.trim().to_owned()).filter(|host| !host.is_empty());
        self.act(|s| s.switch_host(host.clone()));
        *lock(&self.out) = None;
        self.host.send_replace(host);
    }

    /// The host the connection loop follows.
    pub fn watch_host(&self) -> watch::Receiver<Option<String>> {
        self.host.subscribe()
    }

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

/// What `connection::daemon`'s loop calls, each the method of the same
/// name above: this window's `Shared` is the session's owner.
impl SessionOwner for Shared {
    fn watch_host(&self) -> watch::Receiver<Option<String>> {
        Shared::watch_host(self)
    }

    fn act(&self, change: impl FnOnce(&mut Session) -> Vec<ClientMsg>) {
        Shared::act(self, change)
    }

    fn connected(
        &self,
        out: mpsc::UnboundedSender<ClientMsg>,
        daemon: Option<Hello>,
        alarm: Option<String>,
        opening: Vec<ServerMsg>,
    ) {
        Shared::connected(self, out, daemon, alarm, opening)
    }

    fn disconnected(&self) {
        Shared::disconnected(self)
    }

    fn browser_shot(&self, frame: argus_protocol::BrowserFrame) {
        let event = lock(&self.session).browser_shot(frame);
        self.publish(vec![event]);
    }

    fn tell_page(&self, event: Event) {
        Shared::tell_page(self, event)
    }

    async fn reconnect_asked(&self) {
        Shared::reconnect_asked(self).await
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
