//! The client's event loop.
//!
//! One `select!` over four sources — terminal events, the daemon's
//! messages, the paste burst's deadline, and the frame tick — feeding one
//! `App` and one renderer. Everything the loop needs but does not decide
//! lives beside it: `terminal` owns the screen, `wire` owns the socket,
//! and `redraw` decides which wake-up is worth a frame.

mod app;
mod backend;
mod clipboard;
mod dirpicker;
mod fuzzy;
mod grid;
mod herdr;
mod history;
mod launch;
mod notes;
mod paste;
mod profile;
mod pty_input;
mod redraw;
mod review;
mod settings;
mod terminal;
mod theme;
mod ui;
mod wire;

use argus_protocol::{ClientMsg, PaneId, ServerMsg};
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

use app::App;
use redraw::RedrawScheduler;
use terminal::{draw_frame, enter_terminal, leave_terminal, ring_bell, Term};
use wire::connection_channels;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let stream = launch::ensure_daemon_and_connect().await?;
    let connection = connection_channels(stream);
    let mut terminal = enter_terminal()?;
    let result = run(&mut terminal, connection).await;
    leave_terminal(&mut terminal)?;
    result
}

/// Keeps trying for a daemon in the background, and reports the connection
/// once it has one.
///
/// A task rather than an await in the loop: reconnecting can take a minute
/// if the daemon has to be started, and the client has to stay drawable and
/// quittable the whole time. It never gives up on its own — the person
/// watching an agent run has no better option than waiting, and quitting is
/// always still theirs.
fn start_reconnecting() -> mpsc::Receiver<Connection> {
    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
        loop {
            if let Ok(stream) = launch::ensure_daemon_and_connect().await {
                let _ = tx.send(connection_channels(stream)).await;
                return;
            }
            tokio::time::sleep(RECONNECT_ROUND_GAP).await;
        }
    });
    rx
}

/// The next connection a reconnect task produces, or never when none is
/// running. Guarded at the call site, but the future still needs a type
/// either way.
async fn next_connection(pending: &mut Option<mpsc::Receiver<Connection>>) -> Option<Connection> {
    match pending {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn run(terminal: &mut Term, connection: Connection) -> anyhow::Result<()> {
    let (in_tx, mut out_rx) = connection;
    let mut app = App::with_settings(in_tx, settings::load());
    // False from the moment the daemon's messages stop, which is what
    // disables that arm of the select: a closed receiver is ready
    // immediately and forever, and polling it would spin the loop.
    let mut connected = true;
    let mut pending_reconnect: Option<mpsc::Receiver<Connection>> = None;
    let mut events = EventStream::new();
    let mut herdr = herdr::HerdrReporter::from_env();
    let mut frames = tokio::time::interval(FRAME_INTERVAL);
    frames.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut redraw = RedrawScheduler::default();
    let mut burst = PasteBurst::default();
    let mut profile = Profile::from_env();
    // Keyed by pane id, not just dimensions — switching to a different pane
    // at the same on-screen size still needs its own Resize, since each
    // pane's pty starts at a hardcoded default until told otherwise.
    let mut last_sizes: std::collections::HashMap<PaneId, (u16, u16)> =
        std::collections::HashMap::new();

    draw_frame(terminal, &mut app)?;

    loop {
        let burst_due = burst.deadline();
        let flash_due = app.next_flash_deadline();
        tokio::select! {
            maybe_event = events.next() => {
                if !take_event(&mut app, &mut burst, &mut redraw, &mut profile, maybe_event) {
                    break;
                }
            }
            _ = sleep_until(burst_due), if burst_due.is_some() => {
                flush_burst(&mut app, &mut burst);
                redraw.input(std::time::Instant::now());
            }
            _ = sleep_until(flash_due), if flash_due.is_some() => {
                app.expire_state_flashes(std::time::Instant::now());
                redraw.changed();
                redraw.due();
            }
            conn = next_connection(&mut pending_reconnect), if pending_reconnect.is_some() => {
                pending_reconnect = None;
                if let Some((in_tx, rx)) = conn {
                    out_rx = rx;
                    connected = true;
                    app.reconnect(in_tx);
                    // Every pane is about to be drawn against a grid it has
                    // not been sized for; forgetting the old sizes is what
                    // makes the next frame claim them again.
                    last_sizes.clear();
                    app.report("reconnected to argusd");
                    redraw.changed();
                    redraw.due();
                }
            }
            // `None` is the daemon going away, and it is matched rather
            // than pattern-bound on purpose. A `Some(..) =` arm over a
            // closed receiver is simply disabled, which is how a dead
            // connection used to leave the client drawing a photograph of
            // the tree and posting keystrokes into nothing, with no way
            // back but a restart. `connected` then keeps the arm off, since
            // a closed receiver is ready forever and would spin the loop.
            msg = out_rx.recv(), if connected => {
                match msg {
                    Some(msg) => {
                        let now = std::time::Instant::now();
                        let mut awaited = false;
                        // Everything the daemon has already queued is
                        // applied before the frame that will show it. One
                        // message per turn of the loop meant a burst from
                        // several agents was drained a message per select,
                        // each one re-arming five futures, and the frame in
                        // between showed a screen that was already stale.
                        // The model is cheap to update; the frame is not.
                        let mut batch = Some(msg);
                        let mut drained = 0;
                        while let Some(msg) = batch.take() {
                            awaited |= take_server_msg(
                                &mut app, terminal, &mut profile, msg, now,
                            )?;
                            drained += 1;
                            if drained < MAX_DRAINED_MESSAGES {
                                batch = out_rx.try_recv().ok();
                            }
                        }
                        if awaited {
                            redraw.input(now);
                        } else {
                            redraw.changed();
                        }
                    }
                    // Everything on screen is now a photograph, so say so
                    // and start looking for another daemon rather than
                    // pretending the keys still go anywhere.
                    None => {
                        connected = false;
                        pending_reconnect = Some(start_reconnecting());
                        app.alert("lost argusd; reconnecting…");
                        redraw.changed();
                        redraw.due();
                    }
                }
            }
            _ = frames.tick(), if redraw.pending() => {
                redraw.due();
            }
        }

        if app.should_quit {
            break;
        }

        profile.flush_due();

        if redraw.take_frame(std::time::Instant::now()) {
            update_herdr(&mut herdr, &app);
            let began = std::time::Instant::now();
            let ui = draw_frame(terminal, &mut app)?;
            profile.record(|c| c.draw(began.elapsed(), ui));
            resize_live_panes(&mut app, &mut last_sizes);
        }
    }

    release_herdr(&mut herdr);

    Ok(())
}

/// How many of the daemon's messages one turn of the loop may apply before
/// it goes back to the select. A cap rather than the whole queue, so a
/// backlog cannot hold off a keystroke indefinitely.
const MAX_DRAINED_MESSAGES: usize = 512;

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
    let awaited = match &msg {
        ServerMsg::Damage { pane, .. } => {
            let pane = *pane;
            profile.record(|c| c.damage(pane, now));
            app.input_pane() == Some(pane)
        }
        _ => false,
    };
    profile.record(profile::Counters::server_msg);
    app.on_server_msg(msg);
    if app.take_bell() {
        ring_bell(terminal)?;
    }
    Ok(awaited)
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
