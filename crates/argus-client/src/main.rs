mod app;
mod backend;
mod dirpicker;
mod fuzzy;
mod grid;
mod herdr;
mod keys;
mod launch;
mod mouse;
mod paste;
mod profile;
mod review;
mod settings;
mod theme;
mod ui;

use std::collections::HashSet;
use std::io;

use argus_protocol::{read_msg, write_msg, ClientMsg, PaneId, ServerMsg};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    EventStream, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use paste::{Flush, PasteBurst, Step};
use profile::{Profile, Record};
use ratatui::Terminal;
use tokio::io::split;
use tokio::sync::mpsc;

const FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);
const SERVER_QUEUE_MESSAGES: usize = 256;

/// Decides which of the loop's wake-ups is worth a frame.
///
/// Damage from the daemon is coalesced onto `FRAME_INTERVAL`: an agent
/// painting a spinner would otherwise redraw the whole screen for every
/// chunk it produces. Input is not. A keystroke is already one event and
/// has nothing to coalesce with, and the person who pressed the key is
/// waiting on the echo, so it presents on the spot.
#[derive(Default)]
struct RedrawScheduler {
    dirty: bool,
    present: bool,
}

impl RedrawScheduler {
    /// Something changed, but nobody is waiting on it — the next frame.
    fn changed(&mut self) {
        self.dirty = true;
    }

    /// Something changed and someone is waiting on it — this frame.
    fn input(&mut self) {
        self.dirty = true;
        self.present = true;
    }

    /// A frame has come due for whatever was already dirty.
    fn due(&mut self) {
        self.present = true;
    }

    fn pending(&self) -> bool {
        self.dirty
    }

    fn take_frame(&mut self) -> bool {
        if !(self.dirty && self.present) {
            return false;
        }
        self.dirty = false;
        self.present = false;
        true
    }
}

use app::App;
use backend::TermBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let stream = launch::ensure_daemon_and_connect().await?;
    let (in_tx, mut out_rx) = connection_channels(stream);
    let mut terminal = enter_terminal()?;
    let result = run(&mut terminal, in_tx, &mut out_rx).await;
    leave_terminal(&mut terminal)?;
    result
}

/// One frame is written as hundreds of small writes. `io::Stdout` is a
/// line writer with a 1 KiB buffer, and terminal output carries almost no
/// newlines, so unbuffered each full frame became a long run of tiny
/// console writes — the fixed ~6ms a frame cost that showed up in the
/// profile whether or not anything had changed. Buffered, a frame is one
/// write, at `end_frame`.
const FRAME_BUFFER: usize = 1 << 20;

type Term = Terminal<TermBackend<io::BufWriter<io::Stdout>>>;

fn enter_terminal() -> anyhow::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    Ok(Terminal::new(TermBackend::new(io::BufWriter::with_capacity(
        FRAME_BUFFER,
        stdout,
    )))?)
}

/// One frame, presented all at once.
///
/// The draw itself is a diff, a cursor move, and a visibility change,
/// written as several syscalls; wrapping them in a synchronized update is
/// what stops the terminal presenting a half-drawn frame with the cursor
/// already moved. The shape goes inside the wrapper for the same reason.
fn draw_frame(terminal: &mut Term, app: &mut App) -> anyhow::Result<std::time::Duration> {
    let mut ui_took = std::time::Duration::ZERO;
    terminal.backend_mut().begin_frame()?;
    terminal.draw(|f| {
        let began = std::time::Instant::now();
        ui::render(f, app);
        ui_took = began.elapsed();
    })?;
    let shape = app
        .layout
        .cursor
        .map_or(argus_protocol::CursorShape::Default, |c| c.shape);
    terminal.backend_mut().set_cursor_shape(shape)?;
    terminal.backend_mut().end_frame()?;
    Ok(ui_took)
}

fn leave_terminal(terminal: &mut Term) -> anyhow::Result<()> {
    // Ahead of everything else: if the last frame died partway through, the
    // terminal is still inside a synchronized update and would show none of
    // what follows.
    terminal.backend_mut().abandon_frame();
    disable_raw_mode()?;
    // The cursor shape belongs to whatever the user runs next, not to the
    // last pane that happened to be focused here.
    let _ = terminal.backend_mut().set_cursor_shape(argus_protocol::CursorShape::Default);
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn connection_channels<S>(
    stream: S,
) -> (mpsc::UnboundedSender<ClientMsg>, mpsc::Receiver<ServerMsg>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut rd, wr) = split(stream);
    let (in_tx, in_rx) = mpsc::unbounded_channel::<ClientMsg>();
    let (out_tx, out_rx) = mpsc::channel::<ServerMsg>(SERVER_QUEUE_MESSAGES);

    tokio::spawn(client_writer(wr, in_rx));
    tokio::spawn(async move {
        while let Ok(msg) = read_msg::<_, ServerMsg>(&mut rd).await {
            if out_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    (in_tx, out_rx)
}

async fn client_writer<W>(mut wr: W, mut rx: mpsc::UnboundedReceiver<ClientMsg>)
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut subscribed = HashSet::new();
    while let Some(first) = rx.recv().await {
        let subscription_change = is_subscription_change(&first);
        let mut batch = vec![first];
        if subscription_change {
            // Selection can change many times inside one rendered frame. Let
            // those changes settle before asking the daemon for full grids.
            tokio::time::sleep(FRAME_INTERVAL).await;
            batch.extend(std::iter::from_fn(|| rx.try_recv().ok()));
        }
        for msg in compact_subscriptions(batch, &mut subscribed) {
            if write_msg(&mut wr, &msg).await.is_err() {
                return;
            }
        }
    }
}

fn is_subscription_change(msg: &ClientMsg) -> bool {
    matches!(
        msg,
        ClientMsg::Subscribe { .. } | ClientMsg::Unsubscribe { .. }
    )
}

fn compact_subscriptions(
    mut batch: Vec<ClientMsg>,
    subscribed: &mut HashSet<PaneId>,
) -> Vec<ClientMsg> {
    // Letting a pane go drops the client's cached grid for it, and only the
    // snapshot a Subscribe brings back can rebuild one: incremental damage
    // has no rows to land on. So a pane this batch let go of and then asked
    // for again still needs its Subscribe on the wire, even though the
    // daemon never stopped streaming it and the message looks redundant —
    // dropping it leaves the column permanently blank.
    let regrid: HashSet<PaneId> = batch
        .iter()
        .filter_map(|msg| match msg {
            ClientMsg::Unsubscribe { pane } => Some(*pane),
            _ => None,
        })
        .collect();

    let mut seen = HashSet::new();
    batch.reverse();
    batch.retain(|msg| match msg {
        ClientMsg::Subscribe { pane } | ClientMsg::Unsubscribe { pane } => seen.insert(*pane),
        _ => true,
    });
    batch.reverse();
    batch.retain(|msg| match msg {
        ClientMsg::Subscribe { pane } => subscribed.insert(*pane) || regrid.contains(pane),
        ClientMsg::Unsubscribe { pane } => subscribed.remove(pane),
        _ => true,
    });
    batch
}

async fn run(
    terminal: &mut Term,
    in_tx: mpsc::UnboundedSender<ClientMsg>,
    out_rx: &mut mpsc::Receiver<ServerMsg>,
) -> anyhow::Result<()> {
    let mut app = App::with_settings(in_tx, settings::load());
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
        tokio::select! {
            maybe_event = events.next() => {
                // Only a keystroke has somebody waiting on its echo. Mouse
                // motion arrives hundreds of times a second and used to
                // present a frame each — the profile's quiet seconds with a
                // hundred frames and no keys in them were the mouse moving.
                // A release is not a keypress, and counting it made the
                // rate in the profile twice what was actually typed.
                let key = matches!(
                    &maybe_event,
                    Some(Ok(Event::Key(k)))
                        if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat)
                );
                if !handle_terminal_event(&mut app, &mut burst, maybe_event) {
                    break;
                }
                if key {
                    let pane = app.input_pane();
                    profile.record(|c| c.key(pane, std::time::Instant::now()));
                    redraw.input();
                } else {
                    redraw.changed();
                }
            }
            _ = sleep_until(burst_due), if burst_due.is_some() => {
                flush_burst(&mut app, &mut burst);
                redraw.input();
            }
            Some(msg) = out_rx.recv() => {
                if let ServerMsg::Damage { pane, .. } = &msg {
                    let pane = *pane;
                    profile.record(|c| c.damage(pane, std::time::Instant::now()));
                }
                profile.record(profile::Counters::server_msg);
                app.on_server_msg(msg);
                redraw.changed();
            }
            _ = frames.tick(), if redraw.pending() => {
                redraw.due();
            }
        }

        if app.should_quit {
            break;
        }

        profile.flush_due();

        if redraw.take_frame() {
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

/// Sleeps until `deadline`, or forever when there is none — the select
/// arm is guarded, but the future still needs a type either way.
async fn sleep_until(deadline: Option<std::time::Instant>) {
    match deadline {
        Some(deadline) => {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await
        }
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
    last_sizes.retain(|pane, _| {
        app.tree
            .iter()
            .flat_map(|project| &project.repositories)
            .flat_map(|repository| &repository.checkouts)
            .flat_map(|checkout| &checkout.panes)
            .any(|candidate| candidate.id == *pane)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Prompt;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn rapid_pane_swaps_cost_one_message_however_many_panes_were_crossed() {
        // Every selection the user passes through drops one grid and asks
        // for another, so a held key can queue hundreds of subscription
        // changes inside a single frame. Only the settled selection is
        // worth a full grid — but it is worth exactly one, because the
        // client threw its own copy away on the way past.
        let (client, mut daemon) = tokio::io::duplex(1024 * 1024);
        let (tx, _server_rx) = connection_channels(client);
        let pane_a = PaneId(1);
        let pane_b = PaneId(2);

        tx.send(ClientMsg::Subscribe { pane: pane_a }).unwrap();
        assert!(matches!(
            read_msg::<_, ClientMsg>(&mut daemon).await.unwrap(),
            ClientMsg::Subscribe { pane } if pane == pane_a
        ));

        for _ in 0..100 {
            tx.send(ClientMsg::Unsubscribe { pane: pane_a }).unwrap();
            tx.send(ClientMsg::Subscribe { pane: pane_b }).unwrap();
            tx.send(ClientMsg::Unsubscribe { pane: pane_b }).unwrap();
            tx.send(ClientMsg::Subscribe { pane: pane_a }).unwrap();
        }

        let settled = timeout(
            Duration::from_secs(5),
            read_msg::<_, ClientMsg>(&mut daemon),
        )
        .await
        .expect("the settled selection must be re-sent, or its column stays blank")
        .unwrap();
        assert!(
            matches!(settled, ClientMsg::Subscribe { pane } if pane == pane_a),
            "{settled:?}"
        );
        assert!(
            timeout(
                Duration::from_millis(50),
                read_msg::<_, ClientMsg>(&mut daemon)
            )
            .await
            .is_err(),
            "intermediate pane selections were written to the daemon"
        );
    }

    #[test]
    fn a_pane_let_go_of_and_taken_back_in_one_batch_is_still_re_subscribed() {
        // Regression: compaction used to see that the daemon was already
        // streaming pane A and drop the Subscribe as redundant. It was not
        // redundant — the app had dropped A's grid on the way out, and with
        // no snapshot to replace it every later damage span landed on a
        // grid with no rows. The pane rendered blank until something else
        // forced a resize.
        let pane_a = PaneId(1);
        let pane_b = PaneId(2);
        let mut subscribed = HashSet::from([pane_a]);
        let batch = vec![
            ClientMsg::Unsubscribe { pane: pane_a },
            ClientMsg::Subscribe { pane: pane_b },
            ClientMsg::Unsubscribe { pane: pane_b },
            ClientMsg::Subscribe { pane: pane_a },
        ];

        let compacted = compact_subscriptions(batch, &mut subscribed);

        assert!(
            matches!(compacted.as_slice(), [ClientMsg::Subscribe { pane }] if *pane == pane_a),
            "{compacted:?}"
        );
        assert_eq!(subscribed, HashSet::from([pane_a]));
    }

    #[test]
    fn subscription_compaction_keeps_the_final_selection() {
        let pane_a = PaneId(1);
        let pane_b = PaneId(2);
        let mut subscribed = HashSet::from([pane_a]);
        let batch = vec![
            ClientMsg::Unsubscribe { pane: pane_a },
            ClientMsg::Subscribe { pane: pane_b },
            ClientMsg::Unsubscribe { pane: pane_b },
            ClientMsg::Subscribe { pane: pane_a },
            ClientMsg::Unsubscribe { pane: pane_a },
            ClientMsg::Subscribe { pane: pane_b },
        ];

        let compacted = compact_subscriptions(batch, &mut subscribed);

        assert!(matches!(
            compacted.as_slice(),
            [
                ClientMsg::Unsubscribe { pane: first },
                ClientMsg::Subscribe { pane: second }
            ] if *first == pane_a && *second == pane_b
        ));
        assert_eq!(subscribed, HashSet::from([pane_b]));
    }

    #[test]
    fn many_damage_messages_request_one_draw_on_the_next_frame() {
        let mut redraw = RedrawScheduler::default();
        for _ in 0..10_000 {
            redraw.changed();
        }

        assert!(!redraw.take_frame(), "damage drew before its frame came due");
        redraw.due();
        assert!(redraw.take_frame());
        assert!(!redraw.take_frame());
    }

    #[test]
    fn a_keystroke_draws_without_waiting_for_the_next_frame() {
        // Regression: input used to be folded into the same 16ms grid as
        // damage, so every character typed into a prompt or a pane waited
        // out the rest of a frame it had no part in starting.
        let mut redraw = RedrawScheduler::default();
        redraw.input();

        assert!(redraw.take_frame());
        assert!(!redraw.take_frame());
    }

    #[test]
    fn a_keystroke_carries_pending_damage_with_it() {
        // The frame it presents is the whole screen, so damage that was
        // waiting for the next tick is already on it and must not ask for
        // a second frame of its own.
        let mut redraw = RedrawScheduler::default();
        redraw.changed();
        redraw.input();

        assert!(redraw.take_frame());
        redraw.due();
        assert!(!redraw.take_frame());
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
    fn a_closed_or_failed_event_stream_stops_the_client() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(tx);

        assert!(!handle_terminal_event(&mut app, &mut PasteBurst::default(), None));
        assert!(!handle_terminal_event(
            &mut app,
            &mut PasteBurst::default(),
            Some(Err(io::Error::other("event stream failed")))
        ));
    }
}
