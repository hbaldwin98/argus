//! The client's event loop.
//!
//! One `select!` over four sources — terminal events, the daemon's
//! messages, the paste burst's deadline, and the frame tick — feeding one
//! `App` and one renderer. Everything the loop needs but does not decide
//! lives beside it: `terminal` owns the screen, `wire` owns the socket,
//! and `redraw` decides which wake-up is worth a frame.

mod app;
mod backend;
mod brief;
mod bridge;
mod web;
mod diagram;
mod clipboard;
mod dirpicker;
mod dropped;
mod fuzzy;
mod grid;
mod herdr;
mod history;
mod hosts;
mod launch;
mod motion;
mod paste;
mod profile;
mod pty_input;
mod redraw;
mod remote;
mod review;
mod selection;
mod settings;
mod ssh_hosts;
mod terminal;
mod theme;
mod ui;
mod wire;

#[cfg(test)]
mod fixtures;

use argus_protocol::{ClientMsg, Hello, PaneId, ServerMsg};
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use paste::{Flush, PasteBurst, Step};
use profile::{Profile, Record};
use tokio::sync::mpsc;

const FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

/// How long a failed round of reconnection attempts waits before another.
/// `ensure_daemon_and_connect` already spends about half a minute of its
/// own retrying, so this is the gap between rounds, not between attempts.
const RECONNECT_ROUND_GAP: std::time::Duration = std::time::Duration::from_secs(2);

/// The two ends of a connection to the daemon.
type Connection = (mpsc::UnboundedSender<ClientMsg>, mpsc::Receiver<ServerMsg>);

use app::{App, HostRequest};
use hosts::{Host, HostEvent, Hosts};
use launch::Connected;
use redraw::RedrawScheduler;
use terminal::{draw_frame, enter_terminal, leave_terminal, ring_bell, Term};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let host = match parse_command(&std::env::args().skip(1).collect::<Vec<_>>())? {
        Command::ServerRestart => return launch::restart_daemon().await,
        Command::ServerStop => return launch::stop_daemon().await,
        Command::Init(dir) => return launch::init(dir).await,
        Command::Bridge => return bridge::run().await,
        Command::Web(command) => return web::run(command).await,
        Command::Tui => None,
        Command::Host(host) => Some(host),
    };
    let local = launch::connect().await?;
    // Before the terminal is taken, so a host ssh cannot reach is explained
    // on a plain line rather than behind a screen that is about to close.
    let remote = match host {
        Some(host) => {
            let connected = remote::connect(&host).await?;
            Some((host, connected))
        }
        None => None,
    };
    let mut terminal = enter_terminal()?;
    let result = run(&mut terminal, local, remote).await;
    leave_terminal(&mut terminal)?;
    result
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Tui,
    ServerRestart,
    ServerStop,
    /// Scan a directory (the working directory when none is given) into a
    /// project in the open workspace, and report what was found.
    Init(Option<String>),
    /// Carry this machine's daemon on stdin and stdout, for a client on
    /// another machine reaching it over ssh.
    Bridge,
    /// The client, with the daemon on this ssh host connected beside this
    /// machine's and on screen.
    Host(String),
    /// The phone client, served by a client of this machine's daemon.
    Web(web::WebCommand),
}

fn parse_command(args: &[String]) -> anyhow::Result<Command> {
    match args {
        [] => Ok(Command::Tui),
        [server, restart] if server == "server" && restart == "restart" => {
            Ok(Command::ServerRestart)
        }
        [server, stop] if server == "server" && stop == "stop" => Ok(Command::ServerStop),
        [init] if init == "init" => Ok(Command::Init(None)),
        [init, dir] if init == "init" => Ok(Command::Init(Some(dir.clone()))),
        [bridge] if bridge == "bridge" => Ok(Command::Bridge),
        [web, rest @ ..] if web == "web" => web::parse(rest).map(Command::Web),
        [flag, host] if flag == "--host" && !host.is_empty() => Ok(Command::Host(host.clone())),
        _ => Err(anyhow::anyhow!(
            "usage: argus [--host HOST | init [DIR] | server (restart | stop) | bridge | web]"
        )),
    }
}

/// Keeps trying for a daemon in the background, and reports the connection
/// once it has one.
///
/// A task rather than an await in the loop: reconnecting can take a minute
/// if the daemon has to be started, and the client has to stay drawable and
/// quittable the whole time. It never gives up on its own — the person
/// watching an agent run has no better option than waiting, and quitting is
/// always still theirs.
fn start_reconnecting(host: Option<String>) -> mpsc::Receiver<Connected> {
    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
        loop {
            let connected = match &host {
                None => launch::connect().await,
                Some(host) => remote::connect(host).await,
            };
            if let Ok(connected) = connected {
                let _ = tx.send(connected).await;
                return;
            }
            tokio::time::sleep(RECONNECT_ROUND_GAP).await;
        }
    });
    rx
}

async fn run(
    terminal: &mut Term,
    local: Connected,
    remote: Option<(String, Connected)>,
) -> anyhow::Result<()> {
    let mut events = EventStream::new();
    let mut herdr = herdr::HerdrReporter::from_env();
    let mut frames = tokio::time::interval(FRAME_INTERVAL);
    frames.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut redraw = RedrawScheduler::default();
    let mut burst = PasteBurst::default();
    let mut profile = Profile::from_env();

    let mut hosts = Hosts::new(attach(None, local, terminal, &mut profile)?);
    if let Some((name, connected)) = remote {
        let remote = attach(Some(name), connected, terminal, &mut profile)?;
        let index = hosts.push(remote);
        hosts.show(index);
    }
    // Hosts the picker asked for, connecting in the background so the
    // screen stays live while ssh works.
    let (connected_tx, mut connected_rx) =
        mpsc::unbounded_channel::<(String, anyhow::Result<Connected>)>();
    let mut connecting: std::collections::HashSet<String> = std::collections::HashSet::new();
    draw_frame(terminal, &mut hosts.on_screen().app)?;

    loop {
        let burst_due = burst.deadline();
        let motion_due = hosts.on_screen().app.next_motion_deadline();
        tokio::select! {
            maybe_event = events.next() => {
                let app = &mut hosts.on_screen().app;
                if !take_event(app, &mut burst, &mut redraw, &mut profile, maybe_event) {
                    break;
                }
            }
            _ = sleep_until(burst_due), if burst_due.is_some() => {
                flush_burst(&mut hosts.on_screen().app, &mut burst);
                redraw.input(std::time::Instant::now());
            }
            // The client has no free-running frame clock, so anything
            // animating has to ask for its own next frame. This is that
            // ask: a spinner whose glyph is due to turn, or focus still
            // travelling. When nothing moves there is no deadline and the
            // loop goes back to sleeping on events.
            _ = sleep_until(motion_due), if motion_due.is_some() => {
                redraw.changed();
                redraw.due();
            }
            event = hosts.next() => match event {
                HostEvent::Reconnected(index, connection) => {
                    let host = hosts.get(index);
                    host.pending_reconnect = None;
                    if let Some(Connected { channels: (in_tx, rx), daemon, opening }) = connection {
                        host.out_rx = rx;
                        host.connected = true;
                        host.app.reconnect(in_tx);
                        // Every pane is about to be drawn against a grid it
                        // has not been sized for; forgetting the old sizes is
                        // what makes the next frame claim them again.
                        host.last_sizes.clear();
                        host.app.report(format!("reconnected to {}", daemon_name(&host.app)));
                        take_opening(&mut host.app, terminal, &mut profile, daemon, opening)?;
                        hosts.share();
                        redraw.changed();
                        redraw.due();
                    }
                }
                HostEvent::Message(index, Some(msg)) => {
                    let on_screen = hosts.is_on_screen(index);
                    let host = hosts.get(index);
                    let now = std::time::Instant::now();
                    let mut awaited = false;
                    // Everything the daemon has already queued is applied
                    // before the frame that will show it. One message per
                    // turn of the loop meant a burst from several agents was
                    // drained a message per select, each one re-arming five
                    // futures, and the frame in between showed a screen that
                    // was already stale. The model is cheap to update; the
                    // frame is not.
                    let mut batch = Some(msg);
                    let mut drained = 0;
                    while let Some(msg) = batch.take() {
                        awaited |= take_server_msg(&mut host.app, terminal, &mut profile, msg, now)?;
                        drained += 1;
                        if drained < MAX_DRAINED_MESSAGES {
                            batch = host.out_rx.try_recv().ok();
                        }
                    }
                    if awaited && on_screen {
                        redraw.input(now);
                    } else {
                        redraw.changed();
                    }
                }
                // Everything that host shows is now a photograph, so say so
                // and start looking for its daemon again rather than
                // pretending the keys still go anywhere.
                HostEvent::Message(index, None) => {
                    let host = hosts.get(index);
                    host.connected = false;
                    host.pending_reconnect = Some(start_reconnecting(host.app.host.clone()));
                    host.app.alert(format!("lost {}; reconnecting…", daemon_name(&host.app)));
                    hosts.share();
                    redraw.changed();
                    redraw.due();
                }
            },
            Some((name, result)) = connected_rx.recv() => {
                connecting.remove(&name);
                match result {
                    Ok(connected) => {
                        // What the host that asked says when it is shown
                        // again, rather than the "connecting" it said then.
                        hosts.on_screen().app.report(format!("{name} connected"));
                        let host = attach(Some(name.clone()), connected, terminal, &mut profile)?;
                        let index = hosts.push(host);
                        hosts.show(index);
                        settings::remember_host(&name);
                    }
                    Err(error) => hosts.on_screen().app.alert(error.to_string()),
                }
                redraw.changed();
                redraw.due();
            }
            _ = frames.tick(), if redraw.pending() => {
                redraw.due();
            }
        }

        if let Some(request) = hosts.on_screen().app.host_request.take() {
            let place = match request {
                HostRequest::Show(place) => place,
                HostRequest::Connect(name) => Some(name),
            };
            match (hosts.find(&place), place) {
                (Some(index), _) => hosts.show(index),
                (None, Some(name)) if connecting.insert(name.clone()) => {
                    hosts.on_screen().app.report(format!("connecting to {name}…"));
                    let connected_tx = connected_tx.clone();
                    tokio::spawn(async move {
                        let result = remote::connect(&name).await;
                        let _ = connected_tx.send((name, result));
                    });
                }
                _ => {}
            }
            redraw.changed();
            redraw.due();
        }

        let host = hosts.on_screen();
        if host.app.should_quit {
            break;
        }

        profile.flush_due();

        if redraw.take_frame(std::time::Instant::now()) {
            update_herdr(&mut herdr, &host.app);
            let began = std::time::Instant::now();
            // Every animation in the frame reads this one instant, so two
            // spinners cannot land on different glyphs in the same frame.
            host.app.set_frame_now(began);
            let ui = draw_frame(terminal, &mut host.app)?;
            profile.record(|c| c.draw(began.elapsed(), ui));
            resize_live_panes(&mut host.app, &mut host.last_sizes);
        }
    }

    release_herdr(&mut herdr);

    Ok(())
}

/// How many of the daemon's messages one turn of the loop may apply before
/// it goes back to the select. A cap rather than the whole queue, so a
/// backlog cannot hold off a keystroke indefinitely.
const MAX_DRAINED_MESSAGES: usize = 512;

/// A host for a daemon just connected to: an app of its own, given what
/// the daemon sent ahead of answering the greeting, and then the answer.
fn attach(
    host: Option<String>,
    connected: Connected,
    terminal: &mut Term,
    profile: &mut Option<Profile>,
) -> anyhow::Result<Host> {
    let Connected {
        channels: (in_tx, out_rx),
        daemon,
        opening,
    } = connected;
    let mut app = App::with_settings(in_tx, settings::load());
    app.host = host;
    take_opening(&mut app, terminal, profile, daemon, opening)?;
    Ok(Host::new(app, out_rx))
}

/// How the status bar names an app's daemon.
fn daemon_name(app: &App) -> String {
    match &app.host {
        None => "argusd".to_string(),
        Some(host) => host.clone(),
    }
}

/// Hands the app what the daemon sent ahead of answering the greeting, and
/// then the answer itself.
fn take_opening(
    app: &mut App,
    terminal: &mut Term,
    profile: &mut Option<Profile>,
    daemon: Option<Hello>,
    opening: Vec<ServerMsg>,
) -> anyhow::Result<()> {
    let now = std::time::Instant::now();
    for msg in opening {
        take_server_msg(app, terminal, profile, msg, now)?;
    }
    app.greeted(daemon.as_ref());
    Ok(())
}

/// Applies one message to the model. `true` when it was the echo of a
/// keystroke — damage from the pane being typed into, which somebody is
/// waiting on. Everything else the daemon sends is background and can wait
/// for the tick.
fn take_server_msg(
    app: &mut App,
    terminal: &mut Term,
    profile: &mut Option<Profile>,
    msg: ServerMsg,
    now: std::time::Instant,
) -> anyhow::Result<bool> {
    let damaged = damaged_pane(&msg);
    if let Some(pane) = damaged {
        profile.record(|c| c.damage(pane, now));
    }
    // The pane being typed into redrawing is a keystroke's echo, which is
    // presented at once rather than on the next tick.
    let awaited = damaged.is_some() && damaged == app.input_pane();
    profile.record(profile::Counters::server_msg);
    app.on_server_msg(msg);
    if app.take_bell() {
        ring_bell(terminal)?;
    }
    Ok(awaited)
}

/// The pane a message redraws part of, in either form damage comes in.
fn damaged_pane(msg: &ServerMsg) -> Option<PaneId> {
    match msg {
        ServerMsg::Damage { pane, .. } | ServerMsg::RowDamage { pane, .. } => Some(*pane),
        _ => None,
    }
}

/// arm is guarded, but the future still needs a type either way.
async fn sleep_until(deadline: Option<std::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
        None => std::future::pending().await,
    }
}

fn flush_burst(app: &mut App, burst: &mut PasteBurst) {
    match burst.take(app.accepts_paste()) {
        Some(Flush::Paste(text)) => app.on_paste(text),
        Some(Flush::Keys(keys)) => {
            for key in keys {
                handle_key_event(app, key);
            }
        }
        None => {}
    }
}

/// One event, with the bookkeeping the loop wants around it. `false` when
/// the event stream has ended and the client should stop.
///
/// Only a keystroke presents on the spot: mouse motion arrives hundreds of
/// times a second and nobody is waiting on its echo, so it rides the next
/// tick.
fn take_event(
    app: &mut App,
    burst: &mut PasteBurst,
    redraw: &mut RedrawScheduler,
    profile: &mut Option<Profile>,
    event: Option<Result<Event, std::io::Error>>,
) -> bool {
    // A release is not a keypress, and counting it made the rate in the
    // profile twice what was actually typed.
    let key = matches!(
        &event,
        Some(Ok(Event::Key(k))) if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat)
    );
    // A pointer crossing the terminal changes nothing on screen, and a
    // frame for each was costing more than everything else the loop does.
    let idle = matches!(&event, Some(Ok(Event::Mouse(m))) if app.mouse_is_idle(m));
    if !handle_terminal_event(app, burst, event) {
        return false;
    }
    if idle {
        return true;
    }
    if key {
        let pane = app.input_pane();
        profile.record(|c| c.key(pane, std::time::Instant::now()));
        redraw.input(std::time::Instant::now());
    } else {
        redraw.changed();
    }
    true
}

fn handle_terminal_event(
    app: &mut App,
    burst: &mut PasteBurst,
    event: Option<Result<Event, std::io::Error>>,
) -> bool {
    match event {
        Some(Ok(Event::Key(key))) => match burst.push(key, std::time::Instant::now()) {
            Step::Dispatch(key) => handle_key_event(app, key),
            Step::Buffered | Step::Drop => {}
            Step::FlushThen(key) => {
                flush_burst(app, burst);
                handle_key_event(app, key);
            }
        },
        Some(Ok(Event::Mouse(event))) => {
            flush_burst(app, burst);
            app.on_mouse(event);
        }
        Some(Ok(Event::Paste(text))) => {
            flush_burst(app, burst);
            app.on_paste(text);
        }
        Some(Ok(_)) => {}
        Some(Err(_)) | None => {
            flush_burst(app, burst);
            return false;
        }
    }
    true
}

fn handle_key_event(app: &mut App, key: crossterm::event::KeyEvent) {
    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        app.on_key(key);
    }
}

fn update_herdr(reporter: &mut Option<herdr::HerdrReporter>, app: &App) {
    if let Some(reporter) = reporter {
        reporter.update(&app.tree, &app.open_workspace);
    }
}

fn release_herdr(reporter: &mut Option<herdr::HerdrReporter>) {
    if let Some(reporter) = reporter {
        reporter.release();
    }
}

fn resize_live_panes(
    app: &mut App,
    last_sizes: &mut std::collections::HashMap<PaneId, (u16, u16)>,
) {
    // Every pane on screen is sized from where it is actually drawn, so a
    // floating editor and the column behind it can differ.
    let live = app.live_panes();
    for (pane, area) in &live {
        let size = (area.height, area.width);
        if size.0 == 0 || size.1 == 0 {
            continue;
        }
        if last_sizes.get(pane) != Some(&size) {
            last_sizes.insert(*pane, size);
            app.resize_pane(*pane, size.0, size.1);
        }
    }
    forget_offscreen(last_sizes, &live);
}

/// Drops the remembered size of every pane that is no longer on screen.
///
/// Leaving the screen unsubscribes, and the daemon reads that as this
/// client no longer constraining the pane's size — so the pane may well be
/// a different size by the time it comes back. Forgetting it here is what
/// makes the next frame that draws it claim its size again.
fn forget_offscreen(
    last_sizes: &mut std::collections::HashMap<PaneId, (u16, u16)>,
    live: &[(PaneId, ratatui::layout::Rect)],
) {
    last_sizes.retain(|pane, _| live.iter().any(|(id, _)| id == pane));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Focus, Prompt};
    use std::io;

    #[test]
    fn no_arguments_open_the_tui() {
        assert!(matches!(parse_command(&[]), Ok(Command::Tui)));
    }

    #[test]
    fn server_restart_is_the_daemon_control_command() {
        let args = ["server".to_string(), "restart".to_string()];
        assert!(matches!(parse_command(&args), Ok(Command::ServerRestart)));
    }

    #[test]
    fn server_stop_is_the_daemon_stop_command() {
        let args = ["server".to_string(), "stop".to_string()];
        assert!(matches!(parse_command(&args), Ok(Command::ServerStop)));
    }

    #[test]
    fn damage_in_either_form_names_the_pane_it_redraws() {
        // Missing the newer form meant a keystroke's echo waited for the
        // next tick instead of being presented at once.
        let per_cell = ServerMsg::Damage {
            pane: PaneId(3),
            spans: Vec::new(),
            cursor: Default::default(),
            mouse: Default::default(),
            alternate_screen: false,
        };
        let runs = ServerMsg::RowDamage {
            pane: PaneId(4),
            scroll: None,
            runs: Vec::new(),
            cursor: Default::default(),
            mouse: Default::default(),
            alternate_screen: false,
        };
        assert_eq!(damaged_pane(&per_cell), Some(PaneId(3)));
        assert_eq!(damaged_pane(&runs), Some(PaneId(4)));
        assert_eq!(damaged_pane(&ServerMsg::Tree(Vec::new())), None);
    }

    #[test]
    fn a_host_is_named_after_the_flag() {
        assert_eq!(
            parse_command(&["--host".to_string(), "devbox".to_string()]).unwrap(),
            Command::Host("devbox".to_string())
        );
        assert!(parse_command(&["--host".to_string()]).is_err());
        assert!(parse_command(&["--host".to_string(), String::new()]).is_err());
    }

    #[test]
    fn bridge_is_the_command_ssh_runs_on_the_far_side() {
        assert_eq!(
            parse_command(&["bridge".to_string()]).unwrap(),
            Command::Bridge
        );
    }

    #[test]
    fn init_scans_the_working_directory_or_the_one_named() {
        assert_eq!(
            parse_command(&["init".to_string()]).unwrap(),
            Command::Init(None)
        );
        assert_eq!(
            parse_command(&["init".to_string(), "~/src".to_string()]).unwrap(),
            Command::Init(Some("~/src".to_string()))
        );
    }

    #[test]
    fn init_reports_what_the_scan_found() {
        let project = crate::fixtures::project(
            1,
            "src",
            vec![crate::fixtures::repository(
                2,
                "orion",
                vec![
                    crate::fixtures::checkout(10, "main", true, vec![]),
                    crate::fixtures::checkout(11, "wt", false, vec![]),
                ],
            )],
        );
        let text = launch::init_summary(&project, "/home/me/src");
        assert!(
            text.starts_with("added src (/home/me/src): 1 repository, 2 checkouts\n"),
            "{text}"
        );
        assert!(text.contains("  orion  2 checkouts\n"), "{text}");
    }

    #[test]
    fn unsupported_arguments_show_usage() {
        let args = ["server".to_string()];
        assert!(parse_command(&args).is_err());
    }

    #[test]
    fn a_pane_that_left_the_screen_claims_its_size_again_when_it_returns() {
        let mut last_sizes =
            std::collections::HashMap::from([(PaneId(1), (30u16, 80u16)), (PaneId(2), (30, 80))]);
        let still_shown = [(PaneId(1), ratatui::layout::Rect::new(0, 0, 80, 30))];

        forget_offscreen(&mut last_sizes, &still_shown);

        assert_eq!(
            last_sizes.keys().copied().collect::<Vec<_>>(),
            vec![PaneId(1)]
        );
    }
    #[test]
    fn paste_events_are_dispatched_to_the_app() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(tx);
        app.prompt = Some(Prompt::EditorCommand {
            input: String::new(),
        });

        assert!(handle_terminal_event(
            &mut app,
            &mut PasteBurst::default(),
            Some(Ok(Event::Paste("pasted".to_string())))
        ));
        assert!(matches!(
            app.prompt,
            Some(Prompt::EditorCommand { ref input }) if input == "pasted"
        ));
    }
    #[test]
    fn a_repeated_null_key_event_keeps_the_leader_chord_pending() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(tx);
        app.focus = Focus::PaneContent;
        let leader = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Null,
            crossterm::event::KeyModifiers::NONE,
        );

        assert!(handle_terminal_event(
            &mut app,
            &mut PasteBurst::default(),
            Some(Ok(Event::Key(leader)))
        ));
        assert!(handle_terminal_event(
            &mut app,
            &mut PasteBurst::default(),
            Some(Ok(Event::Key(crossterm::event::KeyEvent {
                kind: crossterm::event::KeyEventKind::Repeat,
                ..leader
            })))
        ));
        assert!(handle_terminal_event(
            &mut app,
            &mut PasteBurst::default(),
            Some(Ok(Event::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('f'),
                crossterm::event::KeyModifiers::NONE,
            ))))
        ));

        assert!(
            app.pane_fullscreen,
            "a repeated NUL cancelled the leader chord"
        );
    }
    #[test]
    fn a_closed_or_failed_event_stream_stops_the_client() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(tx);

        assert!(!handle_terminal_event(
            &mut app,
            &mut PasteBurst::default(),
            None
        ));
        assert!(!handle_terminal_event(
            &mut app,
            &mut PasteBurst::default(),
            Some(Err(io::Error::other("event stream failed")))
        ));
    }
}
