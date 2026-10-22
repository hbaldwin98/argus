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
use argus_desktop_core::connection::effects::{self, Clipboard as _, Outlets};
use argus_desktop_core::connection::page::{Call, Event};
use argus_desktop_core::session::Session;
use argus_desktop_core::settings::remembered::{self, Settings};
use argus_desktop_core::system::notifications::Alert;
use argus_desktop_core::system::opener;
use argus_protocol::{ClientMsg, Hello, ServerMsg};
use tokio::sync::{mpsc, watch, Notify};

use crate::system::{chime, clipboard};

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
    /// The alerts to raise, queued for the UI thread: the platform's notification centre is its,
    /// and a change is made on whichever thread asked for it.
    alerting: mpsc::UnboundedSender<Alert>,
    alerts: Mutex<Option<mpsc::UnboundedReceiver<Alert>>>,
    /// What plays the chime when the session sends one.
    chime: Box<dyn Fn() + Send + Sync>,
}

impl Owner {
    /// The owner, and the receiver the UI thread drains. What a fresh page
    /// would be sent, everything, is already waiting in it.
    pub fn new(settings: Settings) -> (Arc<Owner>, mpsc::UnboundedReceiver<Vec<Event>>) {
        Self::sounding(settings, chime::ring)
    }

    /// An owner whose chime is `chime`, for a test that must not make a sound.
    pub fn sounding(settings: Settings, chime: impl Fn() + Send + Sync + 'static) -> (Arc<Owner>, mpsc::UnboundedReceiver<Vec<Event>>) {
        let host = settings.host.clone();
        let (heard, listening) = mpsc::unbounded_channel();
        let (alerting, alerts) = mpsc::unbounded_channel();
        let owner = Owner {
            session: Mutex::new(Session::new(settings)),
            out: Mutex::new(None),
            heard,
            reconnect: Notify::new(),
            host: watch::channel(host).0,
            alerting,
            alerts: Mutex::new(Some(alerts)),
            chime: Box::new(chime),
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
                alert: &mut |alert| {
                    // A closed queue is a window that is gone.
                    let _ = self.alerting.send(alert.clone());
                },
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
            // Written here, so the window's views are never granted the clipboard (SHELL-SYSTEM-3).
            Call::CopyText { text } => {
                if let Err(why) = clipboard::System.write(&text) {
                    self.act(|s| s.note(format!("could not copy: {why}")));
                }
            }
            // The window's own frame answers these natively.
            Call::Minimize | Call::ToggleMaximize | Call::CloseWindow | Call::DragWindow | Call::SnapLayouts => {
                tracing::debug!("a window call reached the owner");
            }
            call => self.act(|s| s.call(call)),
        }
    }

    /// When a browser size the session held back may go, on the clock of the `BrowserViewport` calls.
    pub fn next_browser_viewport(&self) -> Option<Duration> {
        lock(&self.session).next_browser_viewport()
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
        // Heard here and not by a view: a sound is the machine's, and the window drops the event
        // as one nothing draws.
        if events.iter().any(|event| matches!(event, Event::Chime)) {
            (self.chime)();
        }
        // A closed receiver is a window that is gone.
        let _ = self.heard.send(events);
    }

    /// The queue of alerts to raise, for the one window that raises them: `None` once taken.
    pub fn alerts(&self) -> Option<mpsc::UnboundedReceiver<Alert>> {
        lock(&self.alerts).take()
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

    fn browser_shot(&self, frame: argus_protocol::BrowserFrame) {
        let event = lock(&self.session).browser_shot(frame);
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

#[cfg(test)]
mod sounds {
    //! A tree in which an agent comes to need the person: the chime and the notification it raises
    //! (SHELL-SYSTEM-1, SHELL-SYSTEM-2).

    use std::sync::atomic::{AtomicUsize, Ordering};

    use argus_protocol::{
        AgentTelemetry, CheckoutId, CheckoutInfo, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId, ProjectInfo, RepositoryId,
        RepositoryInfo,
    };

    use super::*;

    fn tree(status: PaneStatus) -> ServerMsg {
        let pane = PaneInfo {
            id: PaneId(7),
            kind: PaneKind::Agent,
            title: "Trim context".into(),
            status,
            note: None,
            template: None,
            children: Vec::new(),
            background: false,
            telemetry: AgentTelemetry::default(),
            since: None,
            live: false,
        };
        let checkout = CheckoutInfo { id: CheckoutId(100), name: "main".into(), path: "/src/main".into(), panes: vec![pane], git: None, primary: true };
        let repository = RepositoryInfo {
            id: RepositoryId(10),
            name: "argus".into(),
            checkouts: vec![checkout],
            branches: Vec::new(),
            default_branch: None,
            remote_branches: Vec::new(),
            unmerged: Default::default(),
            last_used: None,
        };
        ServerMsg::Tree(vec![ProjectInfo { id: ProjectId(1), name: "src".into(), root: None, repositories: vec![repository] }])
    }

    fn counted() -> (Arc<AtomicUsize>, impl Fn() + Send + Sync + 'static) {
        let rang = Arc::new(AtomicUsize::new(0));
        let counting = rang.clone();
        (rang, move || {
            counting.fetch_add(1, Ordering::SeqCst);
        })
    }

    #[test]
    fn an_agent_coming_to_need_the_person_rings_the_chime_once_and_queues_its_alert() {
        let (rang, chime) = counted();
        let (owner, _heard) = Owner::sounding(Settings::default(), chime);
        let mut alerts = owner.alerts().expect("the queue, once");
        owner.call(Call::Focus { focused: false });
        owner.act(|s| s.receive(tree(PaneStatus::Working)));
        assert_eq!(rang.load(Ordering::SeqCst), 0, "working is not worth a sound");
        assert!(alerts.try_recv().is_err());
        owner.act(|s| s.receive(tree(PaneStatus::Waiting)));
        assert_eq!(rang.load(Ordering::SeqCst), 1);
        let alert = alerts.try_recv().expect("an alert is queued for the UI thread");
        assert_eq!((alert.title.as_str(), alert.body.as_str()), ("Trim context", "Is waiting for you · argus / main"));
        assert!(alerts.try_recv().is_err(), "once");
    }

    #[test]
    fn a_muted_chime_still_notifies() {
        let (rang, chime) = counted();
        let (owner, _heard) = Owner::sounding(Settings::default(), chime);
        let mut alerts = owner.alerts().unwrap();
        owner.call(Call::Focus { focused: false });
        owner.act(|s| s.receive(tree(PaneStatus::Working)));
        owner.call(Call::SetChime { on: false });
        owner.act(|s| s.receive(tree(PaneStatus::Failed)));
        assert_eq!(rang.load(Ordering::SeqCst), 0);
        assert_eq!(alerts.try_recv().map(|a| a.body), Ok("Failed · argus / main".to_owned()));
    }

    #[test]
    fn the_alert_queue_is_for_one_window() {
        let (owner, _heard) = Owner::sounding(Settings::default(), || {});
        assert!(owner.alerts().is_some());
        assert!(owner.alerts().is_none());
    }
}
