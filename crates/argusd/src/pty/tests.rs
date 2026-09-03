//! Panes driven against real child processes, and the vt100
//! translation driven against a parser fed directly.

use super::*;
use argus_protocol::CellSpan;

#[test]
fn nested_processes_do_not_inherit_the_outer_herdr_pane() {
    let mut command = CommandBuilder::new("dummy");
    command.env("HERDR_ENV", "1");
    command.env("HERDR_PANE_ID", "w1:p1");
    command.env("ARGUS_PANE", "7");

    strip_herdr_context(&mut command, ["HERDR_ENV".into(), "HERDR_PANE_ID".into()]);

    assert_eq!(command.get_env("HERDR_ENV"), None);
    assert_eq!(command.get_env("HERDR_PANE_ID"), None);
    assert_eq!(
        command.get_env("ARGUS_PANE"),
        Some(std::ffi::OsStr::new("7"))
    );
}

#[test]
fn a_childs_mouse_request_is_carried_on_the_snapshot() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    assert_eq!(snapshot_mouse(&parser), MouseTracking::default());

    parser.process(b"[?1002h[?1006h");
    assert_eq!(
        snapshot_mouse(&parser),
        MouseTracking {
            mode: MouseMode::ButtonMotion,
            encoding: MouseEncoding::Sgr,
        }
    );

    parser.process(b"[?1002l");
    assert_eq!(snapshot_mouse(&parser).mode, MouseMode::None);
}

/// A parser holding `lines` numbered lines on a 4-row screen, so the
/// live screen is the last four and everything before it is scrollback.
fn scrolled(lines: usize) -> vt100::Parser {
    let mut parser = vt100::Parser::new(4, 20, SCROLLBACK_LINES);
    for i in 1..=lines {
        parser.process(format!("line {i}\r\n").as_bytes());
    }
    parser
}

fn first_row(cells: &[Vec<Cell>]) -> String {
    cells[0].iter().map(|c| c.ch.as_str()).collect::<String>()
        .trim_end()
        .to_string()
}

#[test]
fn an_offset_reads_the_lines_that_scrolled_off_the_top() {
    let mut parser = scrolled(10);

    // 10 lines written, 4 rows visible, and the cursor sits on the row
    // after the last one: rows 8..11 are live, so 7 lines are behind.
    let (live, offset, depth) = read_scrollback(&mut parser, 0);
    assert_eq!((offset, depth), (0, 7));
    assert_eq!(first_row(&live), "line 8");

    let (back, offset, _) = read_scrollback(&mut parser, 3);
    assert_eq!(offset, 3);
    assert_eq!(first_row(&back), "line 5");
}

#[test]
fn an_offset_past_the_top_stops_at_the_oldest_line_it_has() {
    let mut parser = scrolled(10);
    let (cells, offset, depth) = read_scrollback(&mut parser, 999);
    assert_eq!(offset, depth, "clamped to the top rather than refused");
    assert_eq!(first_row(&cells), "line 1");
}

#[test]
fn reading_scrollback_leaves_the_parser_on_the_live_screen() {
    // The offset is parser-global. Left set, the pump would diff
    // scrolled-back rows against live ones and broadcast the difference
    // to every other subscriber as damage.
    let mut parser = scrolled(10);
    read_scrollback(&mut parser, 5);
    assert_eq!(parser.screen().scrollback(), 0);
    assert_eq!(first_row(&snapshot_grid(&parser)), "line 8");
}

#[test]
fn the_alternate_screen_reports_no_scrollback_of_its_own() {
    // A full-screen child manages its own history, and the shell's
    // must not show through underneath it.
    let mut parser = scrolled(10);
    parser.process(b"[?1049h");
    let (_, offset, depth) = read_scrollback(&mut parser, 5);
    assert_eq!((offset, depth), (0, 0));
}

#[test]
fn an_alternate_screen_request_is_visible_on_the_snapshot() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    assert!(!parser.screen().alternate_screen());
    parser.process(b"[?1049h");
    assert!(parser.screen().alternate_screen());
    parser.process(b"[?1049l");
    assert!(!parser.screen().alternate_screen());
}

#[test]
fn alternate_scroll_alone_does_not_enable_mouse_reporting() {
    // Codex (and similar TUIs) send DECSET 1007 so a wheel becomes
    // cursor keys. That is not mouse tracking; treating it as such
    // would type `ESC [ < 65 ...` into the prompt.
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(b"[?1007h[?1049h");
    assert_eq!(snapshot_mouse(&parser), MouseTracking::default());
    assert!(parser.screen().alternate_screen());
}

#[test]
fn a_bar_cursor_request_is_picked_out_of_the_stream() {
    let mut scan = CursorShapeScanner::default();
    scan.feed(b"hello[6 qworld");
    assert_eq!(scan.shape(), CursorShape::SteadyBar);
}

#[test]
fn a_request_split_across_reads_still_lands() {
    // A read boundary falls wherever the kernel put it, so the halves
    // of a five-byte sequence routinely arrive in different chunks.
    let mut scan = CursorShapeScanner::default();
    for chunk in [&b""[..], b"[", b"5", b" ", b"q"] {
        scan.feed(chunk);
    }
    assert_eq!(scan.shape(), CursorShape::BlinkingBar);
}

#[test]
fn a_zero_or_bare_parameter_hands_the_shape_back_to_the_host() {
    let mut scan = CursorShapeScanner::default();
    scan.feed(b"[2 q");
    assert_eq!(scan.shape(), CursorShape::SteadyBlock);

    scan.feed(b"[0 q");
    assert_eq!(scan.shape(), CursorShape::Default);

    scan.feed(b"[4 q");
    scan.feed(b"[ q");
    assert_eq!(scan.shape(), CursorShape::Default);
}

#[test]
fn other_escape_sequences_leave_the_shape_alone() {
    let mut scan = CursorShapeScanner::default();
    scan.feed(b"[3 q");
    assert_eq!(scan.shape(), CursorShape::BlinkingUnderline);

    // Colours, cursor moves, private modes, and a `q` that is not the
    // final byte of a DECSCUSR: all common, none of them shape changes.
    scan.feed(b"[31m[2;5H[?25l[?1049hq[10q");
    assert_eq!(scan.shape(), CursorShape::BlinkingUnderline);
}

#[test]
fn a_truncated_sequence_does_not_swallow_the_one_behind_it() {
    // An ESC always restarts the machine, so an abandoned sequence
    // cannot eat the next request.
    let mut scan = CursorShapeScanner::default();
    scan.feed(b"[2[6 q");
    assert_eq!(scan.shape(), CursorShape::SteadyBar);
}

/// Flattens a grid to one string per row, trailing blanks trimmed.
fn rows_of(grid: &[Vec<Cell>]) -> Vec<String> {
    grid.iter()
        .map(|r| {
            r.iter()
                .map(|c| c.ch.as_str())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

fn grid_contains(grid: &[Vec<Cell>], needle: &str) -> bool {
    rows_of(grid).iter().any(|r| r.contains(needle))
}

fn echo(text: &str) -> Spawn {
    Spawn::Program {
        program: "echo".to_string(),
        args: vec![text.to_string()],
        env: Vec::new(),
        resource_policy: ResourcePolicy::Unrestricted,
    }
}

#[cfg(windows)]
#[test]
fn job_memory_limit_helper() {
    if std::env::var_os("ARGUS_JOB_MEMORY_TEST").is_none() {
        return;
    }

    // Give the parent time to assign this process to the job before it
    // starts committing memory. The total allocation stays host-safe if
    // containment is broken; success then makes the parent test fail.
    std::thread::sleep(Duration::from_millis(500));
    let mut allocations = Vec::new();
    for _ in 0..8 {
        allocations.push(vec![0xa5; 32 * 1024 * 1024]);
    }
    std::hint::black_box(allocations);
}

#[cfg(windows)]
#[test]
fn a_job_stops_a_process_that_exceeds_its_memory_limit() {
    use std::os::windows::io::AsRawHandle;
    use std::process::{Command, Stdio};

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pty::tests::job_memory_limit_helper",
            "--nocapture",
        ])
        .env("ARGUS_JOB_MEMORY_TEST", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let job = ProcessJob::new(JobLimits {
        memory_bytes: 128 * 1024 * 1024,
        active_processes: 4,
    })
    .unwrap();
    job.assign(AsRawHandle::as_raw_handle(&child)).unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            job.terminate().unwrap();
            let _ = child.wait();
            panic!("memory-limited process did not exit");
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    assert!(
        !status.success(),
        "a process that committed 256 MiB escaped a 128 MiB job limit"
    );
}

#[cfg(windows)]
#[test]
fn job_descendant_helper() {
    if std::env::var_os("ARGUS_JOB_DESCENDANT_TEST").is_none() {
        return;
    }

    println!("argus-job-descendant-ready");
    std::thread::sleep(Duration::from_secs(20));
}

#[cfg(windows)]
#[tokio::test]
async fn agent_policy_contains_the_program_behind_the_cmd_wrapper() {
    let program = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for i in 0..20 {
        let pane = PaneRuntime::spawn(
            PaneId(32 + i),
            &std::env::temp_dir(),
            Spawn::Program {
                program: program.clone(),
                args: vec![
                    "--exact".to_string(),
                    "pty::tests::job_descendant_helper".to_string(),
                    "--nocapture".to_string(),
                ],
                env: vec![("ARGUS_JOB_DESCENDANT_TEST".to_string(), "1".to_string())],
                resource_policy: ResourcePolicy::Agent,
            },
            |_| {},
        )
        .unwrap();

        wait_for(&pane, |g| grid_contains(g, "argus-job-descendant-ready")).await;
        let active = pane._job.as_ref().unwrap().active_processes().unwrap();
        pane.kill().unwrap();

        assert!(
            active >= 2,
            "only cmd.exe entered the job on attempt {i}; its agent child escaped"
        );

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while pane._job.as_ref().unwrap().active_processes().unwrap() != 0 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "terminating the pane left a descendant running on attempt {i}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// Polls the pane's own grid until `pred` holds or the deadline passes.
/// Polling the grid rather than sleeping a fixed time keeps the test both
/// fast and non-flaky: process startup latency varies wildly.
async fn wait_for(pane: &PaneRuntime, pred: impl Fn(&[Vec<Cell>]) -> bool) -> Vec<Vec<Cell>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let (_, _, grid, _, _, _) = pane.full_snapshot();
        if pred(&grid) {
            return grid;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("timed out; last screen was:\n{}", rows_of(&grid).join("\n"));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_programs_output_lands_on_the_panes_grid() {
    let pane = PaneRuntime::spawn(
        PaneId(1),
        &std::env::temp_dir(),
        echo("argus-marker"),
        |_| {},
    )
    .unwrap();
    wait_for(&pane, |g| grid_contains(g, "argus-marker")).await;
}

#[tokio::test]
async fn a_short_lived_commands_output_is_not_lost_to_its_own_exit() {
    // Regression: the pump used to break out of its loop as soon as
    // `try_wait` reported the child gone, discarding output still in
    // flight from the reader thread. A command fast enough to exit
    // within a frame or two would leave a permanently blank pane.
    // Repeated because the race only lost the output sometimes.
    for i in 0..10 {
        let marker = format!("fast-exit-{i}");
        let pane =
            PaneRuntime::spawn(PaneId(20 + i), &std::env::temp_dir(), echo(&marker), |_| {})
                .unwrap();
        wait_for(&pane, |g| grid_contains(g, &marker)).await;
    }
}

/// A child that prints `count` numbered lines and exits, so more output
/// goes past than the 24-row pty can hold.
fn counter(count: usize) -> Spawn {
    #[cfg(windows)]
    let (program, args) = (
        "cmd".to_string(),
        vec![
            "/C".to_string(),
            format!("for /l %i in (1,1,{count}) do @echo line%i"),
        ],
    );
    #[cfg(not(windows))]
    let (program, args) = (
        "sh".to_string(),
        vec![
            "-c".to_string(),
            format!("i=1; while [ $i -le {count} ]; do echo line$i; i=$((i+1)); done"),
        ],
    );
    Spawn::Program {
        program,
        args,
        env: Vec::new(),
        resource_policy: ResourcePolicy::Unrestricted,
    }
}

#[tokio::test]
async fn a_childs_earlier_output_is_still_readable_after_it_scrolls_off() {
    // The whole point of the feature, through the real pipeline: reader
    // thread, pump, parser, and the scrollback read on top of them.
    let pane =
        PaneRuntime::spawn(PaneId(40), &std::env::temp_dir(), counter(60), |_| {}).unwrap();
    wait_for(&pane, |g| grid_contains(g, "line60")).await;

    let (live, offset, depth) = pane.scrollback(0);
    assert_eq!(offset, 0);
    assert!(depth > 0, "60 lines do not fit in 24 rows");
    assert!(
        !rows_of(&live).iter().any(|r| r == "line1"),
        "the first line is off the live screen: {:?}",
        rows_of(&live)
    );

    let (back, offset, _) = pane.scrollback(depth);
    assert_eq!(offset, depth, "clamped to the oldest line it kept");
    assert!(
        rows_of(&back).iter().any(|r| r == "line1"),
        "the first line is recoverable: {:?}",
        rows_of(&back)
    );
}

#[tokio::test]
async fn output_is_broadcast_as_damage_to_subscribers() {
    let pane = PaneRuntime::spawn(
        PaneId(7),
        &std::env::temp_dir(),
        echo("damage-marker"),
        |_| {},
    )
    .unwrap();
    let mut rx = pane.subscribe();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut seen = String::new();
    loop {
        let left = deadline - tokio::time::Instant::now();
        let msg = tokio::time::timeout(left, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("no damage carrying the marker; saw {seen:?}"))
            .unwrap();
        match msg {
            ServerMsg::Damage { pane, spans, .. } => {
                assert_eq!(pane, PaneId(7), "damage must be tagged with its own pane");
                seen.push_str(&span_text(&spans));
                if seen.contains("damage-marker") {
                    return;
                }
            }
            ServerMsg::PaneClosed { .. } => {
                panic!("exited before the marker appeared; saw {seen:?}")
            }
            _ => {}
        }
    }
}

fn span_text(spans: &[CellSpan]) -> String {
    spans
        .iter()
        .flat_map(|s| s.cells.iter().map(|c| c.ch.as_str()))
        .collect()
}

#[tokio::test]
async fn typed_input_reaches_the_child_and_its_output_comes_back() {
    // The M1 spine end-to-end, without a terminal: keystrokes in, screen
    // out. This is the loop that otherwise needs a real TUI to check.
    let pane = PaneRuntime::spawn(
        PaneId(2),
        &std::env::temp_dir(),
        Spawn::DefaultShell,
        |_| {},
    )
    .unwrap();
    // Let the shell draw its prompt before typing at it.
    tokio::time::sleep(Duration::from_millis(500)).await;
    pane.input().write(b"echo argus-typed\r").unwrap();
    wait_for(&pane, |g| grid_contains(g, "argus-typed")).await;
    let _ = pane.kill();
}

#[tokio::test]
async fn a_pane_nobody_is_watching_still_keeps_its_screen() {
    // The pump skips snapshotting and diffing a grid while there are no
    // subscribers — that work is what a background agent's output
    // charges the pane you are typing into. It must not skip feeding
    // the parser: the screen a later subscriber is handed comes from
    // there, and a pane that ran unwatched would otherwise come back
    // blank.
    let pane = PaneRuntime::spawn(
        PaneId(30),
        &std::env::temp_dir(),
        echo("unwatched-marker"),
        |_| {},
    )
    .unwrap();
    wait_for(&pane, |g| grid_contains(g, "unwatched-marker")).await;

    let (_, _, grid, _, _, _, _rx) = pane.snapshot_and_subscribe();
    assert!(
        grid_contains(&grid, "unwatched-marker"),
        "output produced with nobody watching was lost:
{}",
        rows_of(&grid).join(
            "
"
        )
    );
}

#[tokio::test]
async fn damage_picks_back_up_after_a_stretch_with_nobody_watching() {
    // Going unwatched drops the grid the pump diffs against, so the
    // first frame after someone subscribes has to be a full repaint.
    // Diffing against the stale one would silently drop every cell that
    // happens to match it.
    let pane = PaneRuntime::spawn(
        PaneId(31),
        &std::env::temp_dir(),
        Spawn::DefaultShell,
        |_| {},
    )
    .unwrap();
    // Long enough for the shell to draw a prompt with nobody watching.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut rx = pane.subscribe();
    pane.input().write(b"echo late-marker\r").unwrap();

    let seen = tokio::time::timeout(Duration::from_secs(20), async {
        let mut text = String::new();
        loop {
            if let Ok(ServerMsg::Damage { spans, .. }) = rx.recv().await {
                text.push_str(&span_text(&spans));
                if text.contains("late-marker") {
                    return text;
                }
            }
        }
    })
    .await;
    let _ = pane.kill();
    seen.expect("damage should resume once someone is watching again");
}

#[tokio::test]
async fn a_child_exit_fires_on_exit_once_and_announces_pane_closed() {
    let seen: Arc<StdMutex<Vec<Option<i32>>>> = Arc::new(StdMutex::new(Vec::new()));
    let sink = seen.clone();
    let pane = PaneRuntime::spawn(PaneId(3), &std::env::temp_dir(), echo("bye"), move |code| {
        sink.lock().unwrap().push(code);
    })
    .unwrap();
    let mut rx = pane.subscribe();

    let closed = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(ServerMsg::PaneClosed { pane, code }) = rx.recv().await {
                return (pane, code);
            }
        }
    })
    .await
    .expect("the pane should have announced its exit");

    assert_eq!(closed.0, PaneId(3));
    assert_eq!(closed.1, Some(0), "a successful echo exits clean");
    // The pump breaks out of its loop on exit, so the callback can only
    // ever run once — a second call would double-mark the tree.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(*seen.lock().unwrap(), vec![Some(0)]);
}

#[tokio::test]
async fn killing_a_pane_makes_it_exit() {
    let pane = PaneRuntime::spawn(
        PaneId(4),
        &std::env::temp_dir(),
        Spawn::DefaultShell,
        |_| {},
    )
    .unwrap();
    let mut rx = pane.subscribe();
    tokio::time::sleep(Duration::from_millis(300)).await;
    pane.kill().unwrap();

    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(ServerMsg::PaneClosed { .. }) = rx.recv().await {
                return;
            }
        }
    })
    .await
    .expect("kill should end the pane");
}

#[tokio::test]
async fn a_pane_starts_at_the_default_size_and_resize_changes_it() {
    let pane = PaneRuntime::spawn(
        PaneId(5),
        &std::env::temp_dir(),
        Spawn::DefaultShell,
        |_| {},
    )
    .unwrap();
    let (rows, cols, grid, _, _, _) = pane.full_snapshot();
    assert_eq!((rows, cols), (DEFAULT_ROWS, DEFAULT_COLS));
    assert_eq!(grid.len(), DEFAULT_ROWS as usize);
    assert_eq!(grid[0].len(), DEFAULT_COLS as usize);

    pane.resize(40, 120).unwrap();
    let (rows, cols, grid, _, _, _) = pane.full_snapshot();
    assert_eq!((rows, cols), (40, 120));
    assert_eq!(
        grid.len(),
        40,
        "the grid must actually grow, not just the pty"
    );
    assert_eq!(grid[0].len(), 120);
    let _ = pane.kill();
}

#[tokio::test]
async fn resize_pushes_a_full_snapshot_so_new_area_is_not_left_blank() {
    // Incremental Damage can't grow a subscriber's cached grid, so a
    // resize has to re-send the whole screen at the new size.
    let pane = PaneRuntime::spawn(
        PaneId(6),
        &std::env::temp_dir(),
        Spawn::DefaultShell,
        |_| {},
    )
    .unwrap();
    let mut rx = pane.subscribe();
    pane.resize(40, 120).unwrap();
    pane.broadcast_snapshot(PaneId(6));

    let msg = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await.unwrap() {
                m @ ServerMsg::PaneSnapshot { .. } => return m,
                _ => continue,
            }
        }
    })
    .await
    .expect("a snapshot should follow a resize");

    let ServerMsg::PaneSnapshot {
        pane: id,
        rows,
        cols,
        cells,
        ..
    } = msg
    else {
        unreachable!()
    };
    assert_eq!(id, PaneId(6));
    assert_eq!((rows, cols), (40, 120));
    assert_eq!(cells.len(), 40);
    let _ = pane.kill();
}

#[test]
fn a_resize_snapshot_is_never_older_than_the_damage_ahead_of_it() {
    // Regression: the snapshot used to be captured with the parser lock and
    // then published without it. The pump holds that lock across producing a
    // frame *and* publishing it, so a Damage newer than the snapshot could
    // reach the subscriber first. The subscriber applied those spans to a
    // grid still at the old size — where out-of-range cells are dropped —
    // and then replaced everything with the older snapshot, while the pump's
    // `prev` had already moved past them. The cells were never sent again,
    // and the pane went on drawing text that had left the screen.
    //
    // Stated as the property that inversion breaks: whatever order the two
    // land in, a snapshot published *after* a Damage must already contain
    // what that Damage carried. Only holding the lock across both halves of
    // the publish can promise that.
    //
    // No pty here. These are the three handles the publish works over, and
    // driving them directly is what makes a trial cheap enough to repeat
    // until the interleaving that used to break shows up.
    let parser = Arc::new(StdMutex::new(vt100::Parser::new(24, 80, 0)));
    let shape = Arc::new(StdMutex::new(CursorShapeScanner::default()));
    let (tx, _keep) = broadcast::channel(64);

    for trial in 0..1_000 {
        parser.lock().unwrap().process(b"[2J[H");
        let mut rx = tx.subscribe();

        let held = parser.lock().unwrap();
        std::thread::scope(|scope| {
            let publisher = {
                let (parser, shape, tx) = (parser.clone(), shape.clone(), tx.clone());
                scope.spawn(move || publish_snapshot(&parser, &shape, &tx, PaneId(61)))
            };
            // The pump, as far as this matters: it changes the screen and
            // announces the change without ever letting go of the parser.
            let pump = {
                let (parser, tx) = (parser.clone(), tx.clone());
                scope.spawn(move || {
                    let mut parser = parser.lock().unwrap();
                    parser.process(b"ZZZ");
                    let _ = tx.send(ServerMsg::Damage {
                        pane: PaneId(61),
                        spans: Vec::new(),
                        cursor: snapshot_cursor(&parser, CursorShape::Default),
                        mouse: snapshot_mouse(&parser),
                        alternate_screen: false,
                    });
                })
            };
            // Both are now contending for it, so releasing starts the race.
            drop(held);
            publisher.join().unwrap();
            pump.join().unwrap();
        });

        let sent: Vec<ServerMsg> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let damage_at = sent
            .iter()
            .position(|m| matches!(m, ServerMsg::Damage { .. }));
        let snapshot = sent
            .iter()
            .enumerate()
            .find_map(|(i, m)| match m {
                ServerMsg::PaneSnapshot { cells, .. } => Some((i, cells)),
                _ => None,
            })
            .expect("the snapshot should be published");

        if damage_at < Some(snapshot.0) {
            let top: String = snapshot.1[0].iter().map(|c| c.ch.as_str()).collect();
            assert!(
                top.starts_with("ZZZ"),
                "trial {trial}: a snapshot published behind a Damage did not contain it, so the pane keeps drawing text that has left the screen"
            );
        }
    }
}

#[tokio::test]
async fn a_pane_runs_in_the_checkouts_directory() {
    let dir = tempfile::tempdir().unwrap();
    // The randomly-named leaf is the marker: the child can only print it
    // if it inherited the checkout's directory as its cwd.
    let leaf = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let pane = PaneRuntime::spawn(
        PaneId(8),
        dir.path(),
        Spawn::Program {
            program: if cfg!(windows) {
                "cd".to_string()
            } else {
                "pwd".to_string()
            },
            args: Vec::new(),
            env: Vec::new(),
            resource_policy: ResourcePolicy::Unrestricted,
        },
        |_| {},
    )
    .unwrap();
    wait_for(&pane, |g| grid_contains(g, &leaf)).await;
}

#[tokio::test]
async fn a_nonexistent_program_does_not_take_the_daemon_down() {
    // Windows routes through `cmd /C`, which itself starts fine and then
    // reports the failure; unix fails at spawn. Either way the daemon
    // must survive — the pane just ends up dead.
    let r = PaneRuntime::spawn(
        PaneId(9),
        &std::env::temp_dir(),
        Spawn::Program {
            program: "argus-definitely-not-a-real-program".to_string(),
            args: Vec::new(),
            env: Vec::new(),
            resource_policy: ResourcePolicy::Unrestricted,
        },
        |_| {},
    );
    if let Ok(pane) = r {
        let mut rx = pane.subscribe();
        let closed = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Ok(ServerMsg::PaneClosed { code, .. }) = rx.recv().await {
                    return code;
                }
            }
        })
        .await
        .expect("a bad program should end the pane, not hang it");
        assert_ne!(closed, Some(0), "a missing program is not a success");
    }
}

// --- pure vt100 conversion ---------------------------------------------

#[test]
fn an_absent_vt100_cell_becomes_a_blank() {
    let c = cell_from_vt100(None);
    assert_eq!(c.ch, " ");
    assert_eq!(c.fg, Color::Default);
}

#[test]
fn colors_convert_across_all_three_forms() {
    assert_eq!(convert_color(vt100::Color::Default), Color::Default);
    assert_eq!(convert_color(vt100::Color::Idx(9)), Color::Idx(9));
    assert_eq!(
        convert_color(vt100::Color::Rgb(1, 2, 3)),
        Color::Rgb(1, 2, 3)
    );
}

#[test]
fn a_parsed_screen_snapshots_to_a_full_rectangular_grid() {
    let mut parser = vt100::Parser::new(4, 10, 0);
    parser.process(b"hi");
    let grid = snapshot_grid(&parser);
    assert_eq!(grid.len(), 4);
    assert!(
        grid.iter().all(|r| r.len() == 10),
        "every row is full width"
    );
    assert_eq!(rows_of(&grid)[0], "hi");
    assert_eq!(
        rows_of(&grid)[1],
        "",
        "untouched rows are blank, not missing"
    );
}

#[test]
fn a_cell_with_no_contents_keeps_the_colours_it_was_cleared_to() {
    // Erasing to end of line under a background colour is how a TUI
    // paints a bar: the cells hold no character at all but do hold that
    // colour. The snapshot skips `contents()` for exactly these cells,
    // so it must still read their attributes — reading them as default
    // cells would knock the colour out of every bar on screen.
    let mut parser = vt100::Parser::new(2, 4, 0);
    parser.process(b"\x1b[41m\x1b[K");
    let grid = snapshot_grid(&parser);
    assert_eq!(grid[0][0].ch, " ", "still drawn as a blank");
    assert_eq!(
        grid[0][3].bg,
        Color::Idx(1),
        "the cleared-to background is lost"
    );
    assert_eq!(
        grid[1][0].bg,
        Color::Default,
        "an untouched row stays default"
    );
}

#[test]
fn a_grapheme_wider_than_one_char_survives_the_snapshot() {
    // A cell holds a character plus any combining marks, so the
    // snapshot has to carry more than one `char` — and more than one
    // byte — through to the client.
    let mut parser = vt100::Parser::new(1, 4, 0);
    parser.process("e\u{301}x".as_bytes());
    let grid = snapshot_grid(&parser);
    assert_eq!(grid[0][0].ch, "e\u{301}");
    assert_eq!(grid[0][1].ch, "x");
}

#[test]
fn sgr_attributes_survive_the_snapshot() {
    let mut parser = vt100::Parser::new(1, 10, 0);
    parser.process(b"\x1b[1;3;4;7;31mX\x1b[0m");
    let grid = snapshot_grid(&parser);
    let c = &grid[0][0];
    assert_eq!(c.ch, "X");
    assert!(c.bold && c.italic && c.underline && c.reverse, "{c:?}");
    assert_eq!(c.fg, Color::Idx(1), "red");
}

#[test]
fn cursor_movement_and_erase_are_honored_not_appended() {
    let mut parser = vt100::Parser::new(2, 10, 0);
    parser.process(b"abcdef\x1b[H\x1b[Kxy");
    assert_eq!(rows_of(&snapshot_grid(&parser))[0], "xy");
}

#[test]
fn cursor_position_and_visibility_survive_vt_parsing() {
    let mut parser = vt100::Parser::new(4, 10, 0);
    parser.process(b"\x1b[3;5H\x1b[?25l");
    assert_eq!(
        snapshot_cursor(&parser, CursorShape::SteadyBar),
        Cursor {
            row: 2,
            col: 4,
            visible: false,
            shape: CursorShape::SteadyBar,
        }
    );
}

#[test]
fn paste_is_delimited_only_when_the_child_requested_it() {
    assert_eq!(paste_bytes(b"one\ntwo", false), b"one\ntwo");
    assert_eq!(
        paste_bytes(b"one\ntwo", true),
        b"\x1b[200~one\ntwo\x1b[201~"
    );
}
