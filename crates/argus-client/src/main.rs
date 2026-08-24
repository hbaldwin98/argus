mod app;
mod fuzzy;
mod grid;
mod keys;
mod launch;
mod mouse;
mod review;
mod settings;
mod theme;
mod ui;

use std::io;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use futures::StreamExt;
use argus_protocol::{read_msg, write_msg, ClientMsg, PaneId, ServerMsg};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::io::split;
use tokio::sync::mpsc;

use app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let stream = launch::ensure_daemon_and_connect().await?;
    let (mut rd, wr) = split(stream);

    let (in_tx, mut in_rx) = mpsc::unbounded_channel::<ClientMsg>();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMsg>();

    tokio::spawn(async move {
        let mut wr = wr;
        while let Some(msg) = in_rx.recv().await {
            if write_msg(&mut wr, &msg).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Ok(msg) = read_msg::<_, ServerMsg>(&mut rd).await {
            if out_tx.send(msg).is_err() {
                break;
            }
        }
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, in_tx, &mut out_rx).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    in_tx: mpsc::UnboundedSender<ClientMsg>,
    out_rx: &mut mpsc::UnboundedReceiver<ServerMsg>,
) -> anyhow::Result<()> {
    let mut app = App::with_settings(in_tx, settings::load());
    let mut events = EventStream::new();
    // Keyed by pane id, not just dimensions — switching to a different pane
    // at the same on-screen size still needs its own Resize, since each
    // pane's pty starts at a hardcoded default until told otherwise.
    let mut last_pane_area: Option<(PaneId, u16, u16)> = None;

    terminal.draw(|f| ui::render(f, &mut app))?;

    loop {
        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                            app.on_key(key);
                        }
                    }
                    Some(Ok(Event::Mouse(ev))) => {
                        app.on_mouse(ev);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
            Some(msg) = out_rx.recv() => {
                app.on_server_msg(msg);
            }
        }

        if app.should_quit {
            break;
        }

        terminal.draw(|f| ui::render(f, &mut app))?;

        if let Some(pane) = app.subscribed {
            let area = app.live_area();
            let key = (pane, area.height, area.width);
            if last_pane_area != Some(key) && area.height > 0 && area.width > 0 {
                last_pane_area = Some(key);
                app.resize_pane(area.height, area.width);
            }
        } else {
            last_pane_area = None;
        }
    }

    Ok(())
}
