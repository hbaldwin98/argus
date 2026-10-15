//! What the status bar says — the connection, what its last failed try
//! said, an alarm about the daemon it reached, whether the window chimes —
//! and whether the page has heard it since it changed. The daemon's
//! greeting is kept here too: what it can take is what the alarm is about.

use argus_protocol::Hello;

use crate::page::{Connection, Status};

pub struct StatusBar {
    connection: Connection,
    detail: Option<String>,
    alarm: Option<String>,
    daemon: Option<Hello>,
    chime: bool,
    /// The page has not heard the status since it changed.
    changed: bool,
    /// The chime was switched since the settings were saved.
    chime_changed: bool,
}

impl StatusBar {
    pub fn new(chime: bool) -> StatusBar {
        StatusBar {
            connection: Connection::Connecting,
            detail: None,
            alarm: None,
            daemon: None,
            chime,
            changed: true,
            chime_changed: false,
        }
    }

    /// Trying to reach the daemon; `detail` is what the last failed try said.
    pub fn connecting(&mut self, detail: Option<String>) {
        self.connection = Connection::Connecting;
        self.detail = detail;
        self.daemon = None;
        self.changed = true;
    }

    /// Someone stopped the daemon: nothing reconnects until the page asks.
    pub fn stopped(&mut self) {
        self.connection = Connection::Stopped;
        self.daemon = None;
        self.changed = true;
    }

    pub fn is_stopped(&self) -> bool {
        self.connection == Connection::Stopped
    }

    pub fn is_connected(&self) -> bool {
        self.connection == Connection::Connected
    }

    /// A daemon answered. `daemon` is its greeting, `None` from one that
    /// said nothing or predates the greeting; `alarm` says it speaks another
    /// protocol.
    pub fn connected(&mut self, daemon: Option<Hello>, alarm: Option<String>) {
        self.connection = Connection::Connected;
        self.detail = None;
        self.alarm = alarm.or_else(|| daemon.as_ref().and_then(another_build));
        self.daemon = daemon;
        self.changed = true;
    }

    /// A greeting that arrived after the window stopped waiting for it.
    pub fn greeted(&mut self, hello: Hello) {
        self.alarm = self.alarm.take().or_else(|| another_build(&hello));
        self.daemon = Some(hello);
        self.changed = true;
    }

    /// Whether the daemon said it takes `capability`; one that said nothing
    /// takes nothing new.
    pub fn daemon_can(&self, capability: &str) -> bool {
        self.daemon.as_ref().is_some_and(|d| d.can(capability))
    }

    pub fn chimes(&self) -> bool {
        self.chime
    }

    pub fn set_chime(&mut self, on: bool) {
        if self.chime != on {
            self.chime = on;
            self.chime_changed = true;
            self.changed = true;
        }
    }

    /// Whether the chime was switched since last asked, so it is saved.
    pub fn take_chime_changed(&mut self) -> bool {
        std::mem::take(&mut self.chime_changed)
    }

    /// The status, when the page has not heard it.
    pub fn take(&mut self) -> Option<Status> {
        std::mem::take(&mut self.changed).then(|| Status {
            connection: self.connection,
            detail: self.detail.clone(),
            alarm: self.alarm.clone(),
            chime: self.chime,
        })
    }

    /// A page that has just loaded holds nothing: it hears the status again.
    pub fn resend(&mut self) {
        self.changed = true;
    }
}

/// What to say of a daemon from another build than this window, as the
/// terminal client says it.
fn another_build(daemon: &Hello) -> Option<String> {
    let ours = env!("CARGO_PKG_VERSION");
    (daemon.version != ours).then(|| {
        format!(
            "argusd {}, window {ours}: run `argus server restart`",
            daemon.version
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::FOCUS;

    #[test]
    fn a_daemon_from_another_build_is_an_alarm_until_the_next_connection() {
        let mut status = StatusBar::new(true);
        status.connected(Some(Hello { version: "0.0.1".into(), ..Hello::this_build() }), None);
        assert!(status.take().unwrap().alarm.unwrap().contains("argus server restart"));

        status.connected(Some(Hello::this_build()), None);
        assert_eq!(status.take().unwrap().alarm, None);
    }

    #[test]
    fn a_late_greeting_from_another_build_raises_the_alarm() {
        let mut status = StatusBar::new(true);
        status.connected(None, None);
        assert!(!status.daemon_can(FOCUS), "a daemon that said nothing takes nothing new");
        status.greeted(Hello { version: "0.0.1".into(), ..Hello::this_build() });
        assert!(status.take().unwrap().alarm.is_some());
        assert!(status.daemon_can(FOCUS));
    }

    #[test]
    fn stopping_is_remembered_until_a_connection_succeeds() {
        let mut status = StatusBar::new(true);
        status.stopped();
        assert!(status.is_stopped());
        status.connecting(Some("refused".into()));
        assert!(!status.is_stopped());
        status.stopped();
        status.connected(None, None);
        assert!(!status.is_stopped() && status.is_connected());
    }

    #[test]
    fn the_page_hears_the_status_once_per_change() {
        let mut status = StatusBar::new(true);
        assert!(status.take().is_some(), "a new window has not been heard");
        assert!(status.take().is_none());
        status.set_chime(true);
        assert!(status.take().is_none(), "an unchanged chime is no change");
        status.resend();
        assert!(status.take().is_some(), "a reloaded page hears it again");
    }

    #[test]
    fn switching_the_chime_says_so_and_asks_to_be_saved_once() {
        let mut status = StatusBar::new(true);
        status.take();
        status.set_chime(false);
        assert!(!status.chimes());
        assert!(!status.take().unwrap().chime);
        assert!(status.take_chime_changed());
        assert!(!status.take_chime_changed(), "saved once");
    }
}
