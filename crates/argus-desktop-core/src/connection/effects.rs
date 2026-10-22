//! The order a session change's effects leave in, for every window that owns
//! a session.
//!
//! Written once so the Tauri window and the GPUI one cannot drift on it:
//! what the change asks of the daemon goes first, then the link or text the
//! session wants handed to the system (a refusal of either goes to the
//! daemon as a note, before the events are built, so the person hears it),
//! then the events, then the settings to keep, then the alerts. Where
//! messages, events and alerts end up is the owner's, passed in as
//! callbacks; the caller holds the session's lock across the call, which is
//! what keeps two threads' changes in order. What the paste key reads off
//! the clipboard is here too, for the same reason: the order is a rule. The
//! system calls themselves, the clipboard's included, are the owner's: this
//! module names no platform crate, only what it needs of one.

use std::path::PathBuf;

use argus_protocol::ClientMsg;

use crate::connection::page::Event;
use crate::session::Session;
use crate::settings::remembered::Settings;
use crate::system::notifications::Alert;

/// The system clipboard, which only an owner can reach: a window crate names
/// the platform one, so this core does not.
pub trait Clipboard {
    /// Puts `text` on the clipboard, or says why not.
    fn write(&self, text: &str) -> Result<(), String>;
    /// The clipboard's text, or `None` when there is none to read.
    fn read(&self) -> Option<String>;
    /// The clipboard's picture as a PNG, or `None` when it holds none. PNG
    /// because every harness's image path takes it, and a clipboard hands
    /// over raw pixels, so the encoding is the implementor's.
    fn read_image(&self) -> Option<Vec<u8>>;
    /// The files a file manager copied, which is no picture and no text: an
    /// image copied in Explorer is its path. Empty when there are none.
    fn read_files(&self) -> Vec<PathBuf>;
}

/// Where an owner sends what a change produced.
pub struct Outlets<'a> {
    /// Messages for the daemon, wherever the connection is.
    pub send: &'a mut dyn FnMut(Vec<ClientMsg>),
    /// Events for the views.
    pub publish: &'a mut dyn FnMut(Vec<Event>),
    /// An alert the system should raise.
    pub alert: &'a mut dyn FnMut(&Alert),
    /// Hands a link to the system's opener, or says why it would not.
    pub open: &'a mut dyn FnMut(&str) -> Result<(), String>,
    /// Keeps the settings a change asked to have kept.
    pub save: &'a mut dyn FnMut(&Settings),
    /// Where a copy is written.
    pub clipboard: &'a dyn Clipboard,
}

/// Changes `session`, and lets every effect of the change leave through
/// `outlets`. `now` is seconds since the Unix epoch.
pub fn act(
    session: &mut Session,
    now: u64,
    change: impl FnOnce(&mut Session) -> Vec<ClientMsg>,
    outlets: &mut Outlets,
) {
    let msgs = change(session);
    (outlets.send)(msgs);
    // Written here rather than by a page, which may write only inside the
    // gesture's own handler, and a key reaches Rust first.
    if let Some(url) = session.take_opening() {
        if let Err(said) = (outlets.open)(&url) {
            let msgs = session.note(said);
            (outlets.send)(msgs);
        }
    }
    if let Some(text) = session.take_copied() {
        let said = match outlets.clipboard.write(&text) {
            Ok(()) => format!("copied {text}"),
            Err(e) => format!("could not copy: {e}"),
        };
        let msgs = session.note(said);
        (outlets.send)(msgs);
    }
    let mut events = session.events(now);
    write_osc52(&mut events, outlets.clipboard);
    (outlets.publish)(events);
    if let Some(settings) = session.take_settings() {
        (outlets.save)(&settings);
    }
    for alert in session.take_alerts() {
        (outlets.alert)(&alert);
    }
}

/// Writes what a pane's child put on the clipboard (OSC 52) through the owner's clipboard, the one
/// writer every copy goes through, and takes the events out: a page that wrote it too would copy
/// twice, and through a grant the page is never given. A refusal says the same words as any other
/// copy's, after the events that were already heard.
fn write_osc52(events: &mut Vec<Event>, clipboard: &dyn Clipboard) {
    let mut refused = None;
    events.retain(|event| {
        let Event::Clipboard { text } = event else { return true };
        if let Err(why) = clipboard.write(text) {
            refused = Some(why);
        }
        false
    });
    if let Some(why) = refused {
        events.push(Event::Notice { text: format!("could not copy: {why}") });
    }
}

/// One thing the paste key puts in the pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pasting {
    /// A picture or a copied file, sent whole.
    File { name: String, bytes: Vec<u8> },
    Text(String),
    /// Why there is nothing to paste, or a file that could not be read.
    Note(String),
}

/// What the clipboard holds for the paste key, in the order the terminal
/// client tries it: a picture first, since a screenshot copied from some
/// apps carries a text form too and the picture is the point; then files a
/// file manager copied, each read whole; then text.
pub fn pastings(clipboard: &dyn Clipboard) -> Vec<Pasting> {
    if let Some(png) = clipboard.read_image() {
        return vec![Pasting::File { name: "clipboard.png".into(), bytes: png }];
    }
    let files = clipboard.read_files();
    if !files.is_empty() {
        return files
            .into_iter()
            .map(|path| {
                let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
                match std::fs::read(&path) {
                    Ok(bytes) => Pasting::File { name, bytes },
                    Err(e) => Pasting::Note(format!("could not read {name}: {e}")),
                }
            })
            .collect();
    }
    match clipboard.read() {
        Some(text) => vec![Pasting::Text(text)],
        None => vec![Pasting::Note("nothing on the clipboard to paste".into())],
    }
}

/// The change `pasting` makes to the session, for [`act`].
pub fn paste(session: &mut Session, pasting: Pasting) -> Vec<ClientMsg> {
    match pasting {
        Pasting::File { name, bytes } => session.paste_file(name, bytes),
        Pasting::Text(text) => session.paste(text),
        Pasting::Note(said) => session.note(said),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::page::Call;
    use crate::fixtures::{checkout, pane, project, repository};
    use argus_protocol::{Hello, PaneId, PaneKind, PaneStatus, ServerMsg};
    use std::cell::RefCell;

    /// Records what it is asked and holds what it is told to.
    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        text: Option<String>,
        image: Option<Vec<u8>>,
        files: Vec<PathBuf>,
        refuses_write: bool,
    }

    impl Clipboard for Fake {
        fn write(&self, text: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("write {text}"));
            if self.refuses_write { Err("locked".into()) } else { Ok(()) }
        }
        fn read(&self) -> Option<String> {
            self.calls.borrow_mut().push("read".into());
            self.text.clone()
        }
        fn read_image(&self) -> Option<Vec<u8>> {
            self.calls.borrow_mut().push("read_image".into());
            self.image.clone()
        }
        fn read_files(&self) -> Vec<PathBuf> {
            self.calls.borrow_mut().push("read_files".into());
            self.files.clone()
        }
    }

    fn noticed(events: &[Event]) -> Vec<&str> {
        events.iter().filter_map(|e| if let Event::Notice { text } = e { Some(text.as_str()) } else { None }).collect()
    }

    /// Runs one change that leaves every kind of effect, and returns the
    /// order they left in and the events that were published.
    fn everything_at_once(clipboard: &Fake, opening_refused: bool) -> (Vec<String>, Vec<Event>) {
        let order = RefCell::new(Vec::new());
        let heard = RefCell::new(Vec::new());
        let mut send = |msgs: Vec<ClientMsg>| order.borrow_mut().push(format!("send {}", msgs.len()));
        let mut publish = |events: Vec<Event>| {
            order.borrow_mut().push("publish".to_owned());
            heard.borrow_mut().extend(events);
        };
        let mut alert = |a: &Alert| order.borrow_mut().push(format!("alert {}", a.title));
        let mut open = |url: &str| {
            order.borrow_mut().push(format!("open {url}"));
            if opening_refused { Err("no opener".to_owned()) } else { Ok(()) }
        };
        let mut save = |settings: &Settings| order.borrow_mut().push(format!("save {:?}", settings.host));
        let mut session = Session::new(Settings::default());
        session.events(0);
        act(
            &mut session,
            0,
            |s| {
                // Switching host forgets the last daemon's alerts, so it goes first.
                s.switch_host(Some("devbox".into()));
                s.owing("https://example.test", "text", Alert { title: "claude".into(), body: "needs you".into() });
                vec![ClientMsg::Subscribe { pane: PaneId(1) }]
            },
            &mut Outlets { send: &mut send, publish: &mut publish, alert: &mut alert, open: &mut open, save: &mut save, clipboard },
        );
        (order.into_inner(), heard.into_inner())
    }

    #[test]
    fn effects_leave_as_messages_then_the_link_and_the_copy_then_events_settings_and_alerts() {
        let clipboard = Fake::default();
        let (order, heard) = everything_at_once(&clipboard, true);
        let expected = [
            "send 1",
            "open https://example.test",
            "send 0",
            "send 0",
            "publish",
            "save Some(\"devbox\")",
            "alert claude",
        ];
        assert_eq!(order, expected, "the daemon, the opener, a note on each, the events, the settings, the alerts");
        assert_eq!(*clipboard.calls.borrow(), ["write text"]);
        assert_eq!(noticed(&heard), ["no opener", "copied text"], "the notes are built into the events that follow");
    }

    #[test]
    fn a_refused_copy_says_so() {
        let clipboard = Fake { refuses_write: true, ..Fake::default() };
        let (_, heard) = everything_at_once(&clipboard, true);
        assert_eq!(noticed(&heard), ["no opener", "could not copy: locked"]);
    }

    #[test]
    fn a_link_that_opened_leaves_no_note() {
        let (_, heard) = everything_at_once(&Fake::default(), false);
        assert_eq!(noticed(&heard), ["copied text"]);
    }

    #[test]
    fn what_a_change_asks_of_the_daemon_leaves_before_the_events_it_made() {
        let order = RefCell::new(Vec::new());
        let mut send = |msgs: Vec<ClientMsg>| order.borrow_mut().push(format!("send {}", msgs.len()));
        let mut publish = |events: Vec<Event>| order.borrow_mut().push(format!("publish {}", events.len()));
        let mut alert = |_: &Alert| order.borrow_mut().push("alert".to_owned());
        let mut open = |_: &str| Ok(());
        let mut save = |_: &Settings| order.borrow_mut().push("save".to_owned());
        let mut session = Session::new(Settings::default());
        act(
            &mut session,
            0,
            |s| {
                s.stopped();
                vec![ClientMsg::Subscribe { pane: PaneId(1) }]
            },
            &mut Outlets {
                send: &mut send,
                publish: &mut publish,
                alert: &mut alert,
                open: &mut open,
                save: &mut save,
                clipboard: &Fake::default(),
            },
        );
        let order = order.into_inner();
        assert_eq!(order.len(), 2, "nothing is sent after the events, and no alert was owed: {order:?}");
        assert_eq!(order[0], "send 1");
        assert!(order[1].starts_with("publish ") && order[1] != "publish 0", "a stop is an event: {order:?}");
    }

    #[test]
    fn a_picture_is_pasted_before_files_and_files_before_text() {
        let everything = Fake {
            text: Some("words".into()),
            image: Some(vec![1, 2]),
            files: vec!["nowhere/a.txt".into()],
            ..Fake::default()
        };
        assert_eq!(pastings(&everything), [Pasting::File { name: "clipboard.png".into(), bytes: vec![1, 2] }]);
        assert_eq!(*everything.calls.borrow(), ["read_image"], "nothing else is read once a picture is");

        let files = Fake { text: Some("words".into()), files: vec!["nowhere/a.txt".into()], ..Fake::default() };
        let pasted = pastings(&files);
        assert!(matches!(pasted.as_slice(), [Pasting::Note(said)] if said.starts_with("could not read a.txt")), "{pasted:?}");
        assert_eq!(*files.calls.borrow(), ["read_image", "read_files"]);

        let text = Fake { text: Some("words".into()), ..Fake::default() };
        assert_eq!(pastings(&text), [Pasting::Text("words".into())]);
        assert_eq!(*text.calls.borrow(), ["read_image", "read_files", "read"]);
    }

    #[test]
    fn an_empty_clipboard_says_there_is_nothing_to_paste() {
        assert_eq!(pastings(&Fake::default()), [Pasting::Note("nothing on the clipboard to paste".into())]);
    }

    #[test]
    fn a_pasted_note_reaches_the_views_as_a_notice() {
        let mut session = Session::new(Settings::default());
        let msgs = paste(&mut session, Pasting::Note("nothing on the clipboard to paste".into()));
        assert!(msgs.is_empty());
        let heard = session.events(0);
        assert!(
            heard.iter().any(|e| matches!(e, Event::Notice { text } if text == "nothing on the clipboard to paste")),
            "{heard:?}"
        );
    }

    #[test]
    fn text_pasted_with_no_pane_on_screen_goes_nowhere() {
        let mut session = Session::new(Settings::default());
        assert!(paste(&mut session, Pasting::Text("hello".into())).is_empty());
        assert!(paste(&mut session, Pasting::File { name: "a.png".into(), bytes: vec![1] }).is_empty());
    }

    // SHELL-SEAM-11, SHELL-SYSTEM-8: the one clipboard writer takes a pane child's OSC 52 too.
    #[test]
    fn what_a_pane_child_put_on_the_clipboard_is_written_by_the_owner_and_not_heard_again() {
        let clipboard = Fake::default();
        let mut events = vec![
            Event::Notice { text: "kept".into() },
            Event::Clipboard { text: "from the child".into() },
            Event::Chime,
        ];
        write_osc52(&mut events, &clipboard);
        assert_eq!(*clipboard.calls.borrow(), ["write from the child"]);
        assert!(matches!(events.as_slice(), [Event::Notice { .. }, Event::Chime]), "{events:?}");
    }

    /// Runs `act` with a session that has heard `text` copied by the shown pane's child, and
    /// returns what `publish` was given.
    fn act_on_osc_52(clipboard: &Fake, text: &str) -> Vec<Event> {
        let heard = RefCell::new(Vec::new());
        let mut send = |_: Vec<ClientMsg>| {};
        let mut publish = |events: Vec<Event>| heard.borrow_mut().extend(events);
        let mut alert = |_: &Alert| {};
        let mut open = |_: &str| Ok(());
        let mut save = |_: &Settings| {};
        let mut session = Session::new(Settings::default());
        let tree = vec![project(1, "src", vec![repository(10, "argus", None, vec![checkout(
            100,
            "main",
            true,
            vec![pane(1, PaneKind::Shell, "zsh", PaneStatus::Idle)],
        )])])];
        session.connected(Some(Hello::this_build()), None, vec![ServerMsg::Tree(tree)]);
        session.call(Call::Select { pane: 1 });
        session.events(0);
        act(
            &mut session,
            0,
            |s| {
                s.receive(ServerMsg::Clipboard { pane: PaneId(1), text: text.into() });
                Vec::new()
            },
            &mut Outlets { send: &mut send, publish: &mut publish, alert: &mut alert, open: &mut open, save: &mut save, clipboard },
        );
        heard.into_inner()
    }

    // SHELL-SYSTEM-8, CROSS-30: `act` is what takes a pane child's OSC 52 to the owner's clipboard.
    #[test]
    fn act_writes_what_the_shown_pane_copied_and_publishes_no_clipboard_event() {
        let clipboard = Fake::default();
        let heard = act_on_osc_52(&clipboard, "from the child");
        assert_eq!(*clipboard.calls.borrow(), ["write from the child"]);
        assert!(!heard.iter().any(|e| matches!(e, Event::Clipboard { .. })), "{heard:?}");
    }

    #[test]
    fn act_says_so_when_the_clipboard_refuses_an_osc_52_write() {
        let clipboard = Fake { refuses_write: true, ..Fake::default() };
        let heard = act_on_osc_52(&clipboard, "from the child");
        assert_eq!(noticed(&heard), ["could not copy: locked"]);
        assert!(!heard.iter().any(|e| matches!(e, Event::Clipboard { .. })), "{heard:?}");
    }

    #[test]
    fn a_refused_osc_52_write_says_so_once_after_what_was_heard() {
        let clipboard = Fake { refuses_write: true, ..Fake::default() };
        let mut events = vec![
            Event::Clipboard { text: "a".into() },
            Event::Notice { text: "kept".into() },
            Event::Clipboard { text: "b".into() },
        ];
        write_osc52(&mut events, &clipboard);
        assert_eq!(noticed(&events), ["kept", "could not copy: locked"]);
        assert_eq!(events.len(), 2);
    }
}
