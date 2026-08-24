mod app;
mod fuzzy;
mod grid;
mod herdr;
mod keys;
mod launch;
mod mouse;
mod review;
mod settings;
mod theme;
mod ui;

use std::collections::HashSet;
use std::io;

use argus_protocol::{read_msg, write_msg, ClientMsg, PaneId, ServerMsg};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::io::split;
use tokio::sync::mpsc;

const FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);
const SERVER_QUEUE_MESSAGES: usize = 256;

use app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let stream = launch::ensure_daemon_and_connect().await?;
    let (in_tx, mut out_rx) = connection_channels(stream);
    let mut terminal = enter_terminal()?;
    let result = run(&mut terminal, in_tx, &mut out_rx).await;
    leave_terminal(&mut terminal)?;
    result
}

fn enter_terminal() -> anyhow::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn leave_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
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
    let mut seen = HashSet::new();
    batch.reverse();
    batch.retain(|msg| match msg {
        ClientMsg::Subscribe { pane } | ClientMsg::Unsubscribe { pane } => seen.insert(*pane),
        _ => true,
    });
    batch.reverse();
    batch.retain(|msg| match msg {
        ClientMsg::Subscribe { pane } => subscribed.insert(*pane),
        ClientMsg::Unsubscribe { pane } => subscribed.remove(pane),
        _ => true,
    });
    batch
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    in_tx: mpsc::UnboundedSender<ClientMsg>,
    out_rx: &mut mpsc::Receiver<ServerMsg>,
) -> anyhow::Result<()> {
    let mut app = App::with_settings(in_tx, settings::load());
    let mut events = EventStream::new();
    let mut herdr = herdr::HerdrReporter::from_env();
    let mut frames = tokio::time::interval(FRAME_INTERVAL);
    frames.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = false;
    // Keyed by pane id, not just dimensions — switching to a different pane
    // at the same on-screen size still needs its own Resize, since each
    // pane's pty starts at a hardcoded default until told otherwise.
    let mut last_sizes: std::collections::HashMap<PaneId, (u16, u16)> =
        std::collections::HashMap::new();

    terminal.draw(|f| ui::render(f, &mut app))?;

    loop {
        tokio::select! {
            maybe_event = events.next() => {
                if !handle_terminal_event(&mut app, maybe_event) {
                    break;
                }
            }
            Some(msg) = out_rx.recv() => {
                app.on_server_msg(msg);
                dirty = true;
                continue;
            }
            _ = frames.tick(), if dirty => {
                dirty = false;
            }
        }

        if app.should_quit {
            break;
        }

        update_herdr(&mut herdr, &app);

        terminal.draw(|f| ui::render(f, &mut app))?;
        resize_live_panes(&mut app, &mut last_sizes);
    }

    release_herdr(&mut herdr);

    Ok(())
}

fn handle_terminal_event(app: &mut App, event: Option<Result<Event, std::io::Error>>) -> bool {
    match event {
        Some(Ok(Event::Key(key))) => handle_key_event(app, key),
        Some(Ok(Event::Mouse(event))) => app.on_mouse(event),
        Some(Ok(_)) => {}
        Some(Err(_)) | None => return false,
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
        reporter.update(&app.tree);
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
            .flat_map(|project| &project.checkouts)
            .flat_map(|checkout| &checkout.panes)
            .any(|candidate| candidate.id == *pane)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn rapid_pane_swaps_do_not_reach_the_daemon_when_selection_returns_to_start() {
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
}
