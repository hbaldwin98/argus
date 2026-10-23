//! Panes driven against real child processes, and the emulator's
//! translation driven against an emulator fed directly.

// These tests start processes of their own to watch what a pane's job
// does to them, which is the test's doing, not a command the daemon starts.
#![allow(clippy::disallowed_methods)]

use super::*;

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
fn panes_do_not_inherit_the_claude_session_that_started_the_daemon() {
    let mut command = CommandBuilder::new("dummy");
    command.env("CLAUDE_CODE_CHILD_SESSION", "1");
    command.env("CLAUDECODE", "1");
    command.env("CLAUDE_CODE_USE_BEDROCK", "1");

    strip_claude_session(
        &mut command,
        ["CLAUDE_CODE_CHILD_SESSION".into(), "CLAUDECODE".into(), "CLAUDE_CODE_USE_BEDROCK".into()],
    );

    assert_eq!(command.get_env("CLAUDE_CODE_CHILD_SESSION"), None);
    assert_eq!(command.get_env("CLAUDECODE"), None);
    assert_eq!(
        command.get_env("CLAUDE_CODE_USE_BEDROCK"),
        Some(std::ffi::OsStr::new("1")),
        "configuration a person set stays"
    );
}

#[test]
fn panes_do_not_inherit_a_repository_named_by_the_environment() {
    let mut shell = CommandBuilder::new("dummy");
    shell.env("GIT_DIR", "/elsewhere/.git/worktrees/w");
    shell.env("GIT_WORK_TREE", "/elsewhere");
    shell.env("GIT_EDITOR", "nvim");

    strip_git_repository(&mut shell);

    assert_eq!(shell.get_env("GIT_DIR"), None);
    assert_eq!(shell.get_env("GIT_WORK_TREE"), None);
    assert_eq!(
        shell.get_env("GIT_EDITOR"),
        Some(std::ffi::OsStr::new("nvim")),
        "configuration a person set stays"
    );
}

#[test]
fn panes_do_not_inherit_the_hook_identity_of_the_pane_that_started_the_daemon() {
    // A daemon started from inside another Argus pane (a dev instance) has
    // that pane's hook variables; handed on, a shell here would report to
    // the other daemon as that pane, and read and write its board.
    let mut shell = CommandBuilder::new("dummy");
    shell.env("ARGUS_HOOK_URL", "http://127.0.0.1:9/pane/37");
    shell.env("ARGUS_HOOK_TOKEN", "outer");
    shell.env("ARGUS_PANE", "37");
    shell.env("ARGUS_HOOK", "/outer/argus-hook");
    shell.env("ARGUS_INSTRUCTIONS", "outer instructions");
    shell.env("ARGUS_INSTANCE", "dev");

    strip_argus_hook_identity(&mut shell);

    for key in ["ARGUS_HOOK_URL", "ARGUS_HOOK_TOKEN", "ARGUS_PANE", "ARGUS_HOOK", "ARGUS_INSTRUCTIONS"] {
        assert_eq!(shell.get_env(key), None, "{key} is the outer pane's");
    }
    assert_eq!(
        shell.get_env("ARGUS_INSTANCE"),
        Some(std::ffi::OsStr::new("dev")),
        "the instance is this daemon's own"
    );
}

#[test]
fn an_agent_keeps_the_hook_identity_it_is_given() {
    // The strip comes before a pane's own variables, so a daemon restarted
    // from inside a pane, whose inherited ARGUS_HOOK is the very helper it
    // hands its agents, still hands it on.
    let spawn = Spawn::Program {
        program: "dummy".into(),
        args: Vec::new(),
        env: vec![
            ("ARGUS_PANE".into(), "2".into()),
            ("ARGUS_HOOK".into(), "/this/argus-hook".into()),
        ],
        resource_policy: ResourcePolicy::Unrestricted,
    };
    let (command, _) = spawn.into_command();
    assert_eq!(command.get_env("ARGUS_PANE"), Some(std::ffi::OsStr::new("2")));
    assert_eq!(command.get_env("ARGUS_HOOK"), Some(std::ffi::OsStr::new("/this/argus-hook")));
}

#[cfg(windows)]
#[test]
fn windows_default_shell_is_explicitly_selected() {
    let (command, _) = Spawn::DefaultShell.into_command();

    assert!(
        !command.is_default_prog(),
        "portable-pty's default Windows program is always cmd.exe"
    );
}

#[test]
fn a_configured_shell_wins_over_windows_fallbacks() {
    let chosen = select_shell(
        Some(std::ffi::OsStr::new("my-shell")),
        &["pwsh", "powershell", "cmd.exe"],
        |candidate| candidate == std::ffi::OsStr::new("my-shell"),
    );

    assert_eq!(chosen, std::ffi::OsString::from("my-shell"));
}

#[test]
fn windows_shell_fallbacks_prefer_pwsh_then_powershell() {
    let chosen = select_shell(
        None,
        &["pwsh", "powershell", "cmd.exe"],
        |candidate| {
            candidate == std::ffi::OsStr::new("pwsh")
                || candidate == std::ffi::OsStr::new("powershell")
        },
    );

    assert_eq!(chosen, std::ffi::OsString::from("pwsh"));
}

#[test]
fn windows_shell_fallbacks_use_powershell_after_pwsh() {
    let chosen = select_shell(
        None,
        &["pwsh", "powershell", "cmd.exe"],
        |candidate| candidate == std::ffi::OsStr::new("powershell"),
    );

    assert_eq!(chosen, std::ffi::OsString::from("powershell"));
}

#[test]
fn windows_shell_selection_uses_cmd_when_no_preferred_shell_is_available() {
    let chosen = select_shell(
        Some(std::ffi::OsStr::new("missing-shell")),
        &["pwsh", "powershell", "cmd.exe"],
        |_| false,
    );

    assert_eq!(chosen, std::ffi::OsString::from("cmd.exe"));
}

#[test]
fn a_dumb_parent_terminal_is_replaced_for_a_pty_child() {
    let mut command = CommandBuilder::new("dummy");
    command.env("TERM", "dumb");

    set_pty_terminal(&mut command);

    assert_eq!(
        command.get_env("TERM"),
        Some(std::ffi::OsStr::new("xterm-256color"))
    );
}

#[test]
fn an_existing_terminal_type_is_preserved_for_a_pty_child() {
    let mut command = CommandBuilder::new("dummy");
    command.env("TERM", "screen-256color");

    set_pty_terminal(&mut command);

    assert_eq!(
        command.get_env("TERM"),
        Some(std::ffi::OsStr::new("screen-256color"))
    );
}

#[test]
fn a_childs_mouse_request_is_carried_on_the_snapshot() {
    let mut vt = Vt::new(24, 80, 0);
    assert_eq!(vt.mouse(), MouseTracking::default());

    vt.process(b"[?1002h[?1006h");
    assert_eq!(
        vt.mouse(),
        MouseTracking {
            mode: MouseMode::ButtonMotion,
            encoding: MouseEncoding::Sgr,
        }
    );

    vt.process(b"[?1002l");
    assert_eq!(vt.mouse().mode, MouseMode::None);
}

#[test]
fn a_childs_kitty_keyboard_flags_are_carried_and_popped() {
    let mut vt = Vt::new(24, 80, 0);
    assert_eq!(vt.keyboard(), KeyboardMode::default());

    // Flags 1 (disambiguate) then 9 (and report all keys), as an agent CLI
    // pushes on start-up.
    vt.process(b"\x1b[>1u");
    assert_eq!(vt.keyboard(), KeyboardMode(1));
    vt.process(b"\x1b[>9u");
    assert_eq!(vt.keyboard(), KeyboardMode(9));

    vt.process(b"\x1b[<u");
    assert_eq!(vt.keyboard(), KeyboardMode(1), "a pop reads back the flags beneath");
    vt.process(b"\x1b[<u");
    assert_eq!(vt.keyboard(), KeyboardMode::default());
}

#[test]
fn a_keyboard_flags_query_is_answered_with_the_flags_in_force() {
    let mut vt = Vt::new(24, 80, 0);
    vt.process(b"\x1b[>5u\x1b[?u");

    assert_eq!(vt.take_replies(), b"\x1b[?5u");
}

#[test]
fn a_keyboard_push_alone_is_broadcast_to_watchers() {
    let mut vt = Vt::new(4, 20, 0);
    let (tx, mut rx) = broadcast::channel(8);
    let mut sent = Sent::default();
    let mut live = vt.grid();
    send_frame(&mut vt, &mut live, &mut sent, &tx, PaneId(1));
    while rx.try_recv().is_ok() {}

    vt.process(b"\x1b[>1u");
    send_frame(&mut vt, &mut live, &mut sent, &tx, PaneId(1));

    match rx.try_recv() {
        Ok(ServerMsg::RowDamage { keyboard, runs, .. }) => {
            assert_eq!(keyboard, KeyboardMode(1));
            assert!(runs.is_empty(), "no cell changed");
        }
        other => panic!("expected a frame, got {other:?}"),
    }
}

#[test]
fn focus_is_reported_only_to_a_child_that_asked_for_it() {
    let mut vt = Vt::new(24, 80, 0);
    assert_eq!(focus_report(vt.focus_events(), true), None);

    vt.process(b"\x1b[?1004h");
    assert_eq!(focus_report(vt.focus_events(), true), Some(&b"\x1b[I"[..]));
    assert_eq!(focus_report(vt.focus_events(), false), Some(&b"\x1b[O"[..]));

    vt.process(b"\x1b[?1004l");
    assert_eq!(focus_report(vt.focus_events(), false), None);
}

#[test]
fn a_childs_osc_52_copy_is_held_for_the_client() {
    let mut vt = Vt::new(24, 80, 0);

    vt.process(b"\x1b]52;c;aGVsbG8=\x07");

    assert_eq!(vt.take_copies(), vec!["hello".to_string()]);
    assert!(vt.take_copies().is_empty(), "sent once");
}

#[test]
fn a_childs_osc_52_read_is_not_answered() {
    let mut vt = Vt::new(24, 80, 0);

    vt.process(b"\x1b]52;c;?\x07");

    assert!(vt.take_copies().is_empty());
    assert!(vt.take_replies().is_empty());
}

#[test]
fn a_cursor_position_request_is_answered_with_where_the_cursor_was_when_asked() {
    let mut vt = Vt::new(24, 80, 0);

    // Two requests in one read, the cursor moving between and after them.
    vt.process(b"\x1b[3;5H\x1b[6nabc\x1b[6n\x1b[10;1H");

    assert_eq!(vt.take_replies(), b"\x1b[3;5R\x1b[3;8R");
    assert!(vt.take_replies().is_empty(), "answered once");
}

#[test]
fn a_status_request_is_answered_as_itself_not_as_a_cursor_position() {
    let mut vt = Vt::new(24, 80, 0);

    // Device status, then an attribute that only looks like a query.
    vt.process(b"\x1b[5n\x1b[6m");

    assert_eq!(vt.take_replies(), b"\x1b[0n");
}

/// An emulator holding `lines` numbered lines on a 4-row screen, so the
/// live screen is the last four and everything before it is scrollback.
fn scrolled(lines: usize) -> Vt {
    let mut vt = Vt::new(4, 20, SCROLLBACK_LINES);
    for i in 1..=lines {
        vt.process(format!("line {i}\r\n").as_bytes());
    }
    vt
}

fn first_row(cells: &[Vec<Cell>]) -> String {
    cells[0].iter().map(|c| c.ch.as_str()).collect::<String>()
        .trim_end()
        .to_string()
}

#[test]
fn an_offset_reads_the_lines_that_scrolled_off_the_top() {
    let vt = scrolled(10);

    // 10 lines written, 4 rows visible, and the cursor sits on the row
    // after the last one: rows 8..11 are live, so 7 lines are behind.
    let live = vt.scrollback(0, None);
    assert_eq!((live.offset, live.depth), (0, 7));
    assert_eq!(first_row(&live.cells), "line 8");

    let back = vt.scrollback(3, None);
    assert_eq!(back.offset, 3);
    assert_eq!(first_row(&back.cells), "line 5");
}

#[test]
fn an_offset_past_the_top_stops_at_the_oldest_line_it_has() {
    let vt = scrolled(10);
    let top = vt.scrollback(999, None);
    assert_eq!(top.offset, top.depth, "clamped to the top rather than refused");
    assert_eq!(first_row(&top.cells), "line 1");
}

#[test]
fn reading_scrollback_leaves_the_live_screen_where_it_was() {
    // Moved, the pump would diff scrolled-back rows against live ones and
    // broadcast the difference to every other subscriber as damage.
    let vt = scrolled(10);
    vt.scrollback(5, None);
    assert_eq!(first_row(&vt.grid()), "line 8");
}

#[test]
fn the_alternate_screen_reports_no_scrollback_of_its_own() {
    // A full-screen child manages its own history, and the shell's
    // must not show through underneath it.
    let mut vt = scrolled(10);
    vt.process(b"[?1049h");
    let read = vt.scrollback(5, None);
    assert_eq!((read.offset, read.depth), (0, 0));
}

#[test]
fn a_line_keeps_its_number_as_more_output_pushes_it_back() {
    let mut vt = scrolled(10);
    let parked = vt.scrollback(3, None);
    assert_eq!(first_row(&parked.cells), "line 5");
    assert_eq!(parked.top, 4, "four lines above it: 1 to 4");

    for i in 11..=30 {
        vt.process(format!("line {i}\r\n").as_bytes());
    }
    let again = vt.scrollback(0, Some(parked.top));
    assert_eq!(first_row(&again.cells), "line 5", "still the line it was");
    assert_eq!(again.offset, 23, "now twenty more lines back");
    assert_eq!(again.top, parked.top);
}

#[test]
fn lines_are_counted_past_the_cap_and_an_evicted_one_reads_as_the_oldest() {
    // A history of five: the count has to go on past what is kept, or a
    // pane that has been printing for a while numbers nothing reliably.
    let mut vt = Vt::new(4, 20, 5);
    let mut all = String::new();
    for i in 1..=100 {
        all.push_str(&format!("line {i}\r\n"));
    }
    vt.process(all.as_bytes());
    let top = vt.scrollback(999, None);
    assert_eq!(top.depth, 5, "trimmed back to its cap");
    assert_eq!(first_row(&top.cells), "line 93");
    assert_eq!(top.top, 92, "every line that went up, not just the kept ones");

    let gone = vt.scrollback(0, Some(10));
    assert_eq!(first_row(&gone.cells), "line 93", "the oldest it still has");
}

#[test]
fn the_alternate_screen_leaves_the_count_where_it_was() {
    let mut vt = scrolled(10);
    let before = vt.scrollback(0, None).top;
    vt.process(b"\x1b[?1049h");
    for i in 0..40 {
        vt.process(format!("full-screen {i}\r\n").as_bytes());
    }
    vt.process(b"\x1b[?1049l");
    assert_eq!(vt.scrollback(0, None).top, before);
}

#[test]
fn rows_lost_to_a_resize_move_the_screen_top_and_keep_the_lines_numbered() {
    let mut vt = scrolled(10);
    let parked = vt.scrollback(2, None);
    vt.resize(2, 20);
    let again = vt.scrollback(0, Some(parked.top));
    assert_eq!(first_row(&again.cells), first_row(&parked.cells));
}

#[test]
fn an_alternate_screen_request_is_visible_on_the_snapshot() {
    let mut vt = Vt::new(24, 80, 0);
    assert!(!vt.alternate_screen());
    vt.process(b"[?1049h");
    assert!(vt.alternate_screen());
    vt.process(b"[?1049l");
    assert!(!vt.alternate_screen());
}

#[test]
fn alternate_scroll_alone_does_not_enable_mouse_reporting() {
    // Codex (and similar TUIs) send DECSET 1007 so a wheel becomes
    // cursor keys. That is not mouse tracking; treating it as such
    // would type `ESC [ < 65 ...` into the prompt.
    let mut vt = Vt::new(24, 80, 0);
    vt.process(b"[?1007h[?1049h");
    assert_eq!(vt.mouse(), MouseTracking::default());
    assert!(vt.alternate_screen());
}

/// The cursor shape `bytes` leave behind.
fn shape_after(chunks: &[&[u8]]) -> CursorShape {
    let mut vt = Vt::new(4, 20, 0);
    for chunk in chunks {
        vt.process(chunk);
    }
    vt.cursor().shape
}

#[test]
fn a_bar_cursor_request_is_picked_out_of_the_stream() {
    assert_eq!(shape_after(&[b"hello\x1b[6 qworld"]), CursorShape::SteadyBar);
}

#[test]
fn a_request_split_across_reads_still_lands() {
    // A read boundary falls wherever the kernel put it, so the halves
    // of a five-byte sequence routinely arrive in different chunks.
    let chunks: [&[u8]; 5] = [b"\x1b", b"[", b"5", b" ", b"q"];
    assert_eq!(shape_after(&chunks), CursorShape::BlinkingBar);
}

#[test]
fn a_zero_or_bare_parameter_hands_the_shape_back_to_the_host() {
    assert_eq!(shape_after(&[b"\x1b[2 q"]), CursorShape::SteadyBlock);
    assert_eq!(shape_after(&[b"\x1b[2 q\x1b[0 q"]), CursorShape::Default);
    assert_eq!(shape_after(&[b"\x1b[4 q\x1b[ q"]), CursorShape::Default);
    assert_eq!(shape_after(&[]), CursorShape::Default, "nothing asked, nothing imposed");
}

#[test]
fn other_escape_sequences_leave_the_shape_alone() {
    // Colours, cursor moves, private modes, and a `q` that is not the
    // final byte of a DECSCUSR: all common, none of them shape changes.
    assert_eq!(
        shape_after(&[b"\x1b[3 q", b"\x1b[31m\x1b[2;5H\x1b[?25l\x1b[?1049hq\x1b[10q"]),
        CursorShape::BlinkingUnderline
    );
}

#[test]
fn a_truncated_sequence_does_not_swallow_the_one_behind_it() {
    // An ESC always starts over, so an abandoned sequence cannot eat the
    // next request.
    assert_eq!(shape_after(&[b"\x1b[2\x1b[6 q"]), CursorShape::SteadyBar);
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

/// ARCHITECTURE.md, "Jobs cap neither memory nor processes": a cap ended
/// agents mid-task. What the agent's job still does is end everything it
/// holds, and keep it, since an agent's job does not let breakaway go.
#[cfg(windows)]
#[test]
fn an_agents_job_caps_nothing_and_ends_what_it_holds() {
    use windows_sys::Win32::System::JobObjects::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

    let flags = ProcessJob::agent().unwrap().limit_flags().unwrap();
    assert_eq!(flags, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, "{flags:#x}");
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
        let active = pane.job.active_processes().unwrap();
        pane.kill().unwrap();

        // The console server, cmd.exe, and the agent behind it.
        assert!(
            active >= 3,
            "only {active} entered the job on attempt {i}; the agent child escaped"
        );

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while pane.job.active_processes().unwrap() != 0 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "terminating the pane left a descendant running on attempt {i}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// Opens a pane of the kind `ARGUS_PANE_OWNER_TEST` names, prints the PIDs
/// in its job, and waits to be killed.
#[cfg(windows)]
#[tokio::test]
async fn pane_owner_helper() {
    let Some(kind) = std::env::var_os("ARGUS_PANE_OWNER_TEST") else {
        return;
    };
    let (spec, started) = if kind == "agent" {
        let ping = Spawn::Program {
            program: "ping".to_string(),
            args: ["-n", "120", "127.0.0.1"].map(String::from).to_vec(),
            env: Vec::new(),
            resource_policy: ResourcePolicy::Agent,
        };
        // The console server, cmd.exe and ping behind it.
        (ping, 3)
    } else {
        (Spawn::DefaultShell, 2)
    };
    let pane = PaneRuntime::spawn(PaneId(90), &std::env::temp_dir(), spec, |_| {}).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while pane.job.active_processes().unwrap() < started {
        assert!(tokio::time::Instant::now() < deadline, "the pane never started");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let pids: Vec<String> = pane
        .job
        .process_ids()
        .unwrap()
        .iter()
        .map(u32::to_string)
        .collect();
    println!("argus-pane-owner-pids {}", pids.join(" "));
    tokio::time::sleep(Duration::from_secs(120)).await;
}

/// HUN-201: a test binary or daemon killed with panes open left their
/// console servers spinning, orphaned, and their children with them.
#[cfg(windows)]
#[test]
fn a_panes_console_and_child_die_with_the_process_that_owns_them() {
    use std::io::BufRead;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::process::{Command, Stdio};
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        PROCESS_TERMINATE,
    };

    for kind in ["shell", "agent"] {
        let mut owner = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "pty::tests::pane_owner_helper", "--nocapture"])
            .env("ARGUS_PANE_OWNER_TEST", kind)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let said = std::io::BufReader::new(owner.stdout.take().unwrap())
            .lines()
            .map_while(Result::ok)
            .find_map(|line| line.strip_prefix("argus-pane-owner-pids ").map(str::to_string))
            .unwrap_or_else(|| panic!("the {kind} pane's owner never named its processes"));
        let in_job: Vec<u32> = said.split(' ').map(|pid| pid.parse().unwrap()).collect();
        let servers = super::console_host::servers_started_by(owner.id());
        assert!(!servers.is_empty(), "the {kind} pane's owner started no console server");
        assert!(
            servers.iter().all(|server| in_job.contains(server)),
            "the {kind} pane's console server {servers:?} is not in its job {in_job:?}"
        );

        // Held before the owner dies, so a PID handed on cannot pass for
        // one still running.
        let watched: Vec<(u32, OwnedHandle)> = in_job
            .iter()
            .map(|&pid| {
                let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
                assert!(!raw.is_null(), "{pid} in the {kind} pane's job is already gone");
                (pid, unsafe { OwnedHandle::from_raw_handle(raw as _) })
            })
            .collect();
        owner.kill().unwrap();
        owner.wait().unwrap();

        let survivors: Vec<u32> = watched
            .iter()
            .filter(|(_, process)| {
                let waited = unsafe { WaitForSingleObject(process.as_raw_handle() as _, 10_000) };
                if waited != WAIT_OBJECT_0 {
                    unsafe { TerminateProcess(process.as_raw_handle() as _, 1) };
                    return true;
                }
                false
            })
            .map(|(pid, _)| *pid)
            .collect();
        assert!(
            survivors.is_empty(),
            "{survivors:?} of the {kind} pane outlived the process that owned it"
        );
    }
}

/// `hold_test_process`'s promise: the test process and what it starts
/// outside any pane are in its job, so nothing a test starts outlives the
/// test binary even when the pane code under test is broken.
#[cfg(windows)]
#[test]
fn what_a_test_process_starts_is_held_in_its_job() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let held = job::hold_test_process().expect("the test process is held in a job");
    assert!(held.holds(unsafe { GetCurrentProcess() } as _).unwrap(), "the test process itself");

    let mut child = std::process::Command::new("cmd")
        .args(["/C", "ping -n 30 127.0.0.1 > NUL"])
        .spawn()
        .unwrap();
    let inside = held.holds(AsRawHandle::as_raw_handle(&child) as _);
    let _ = child.kill();
    let _ = child.wait();
    assert!(inside.unwrap(), "a process the test started, outside any pane");
}

/// The rest of `hold_test_process`'s promise, which no test can watch
/// happen from inside the process it is about: its job ends everything in
/// it when the test binary ends, and lets a process that asks to leave it
/// go. The first keeps a broken run from leaving shells behind; the second
/// is a shell job's rule, which `argus` relies on to start its daemon from a
/// pane, so code under test sees the job it would under the daemon.
#[cfg(windows)]
#[test]
fn the_test_process_job_ends_what_it_holds_and_lets_breakaway_go() {
    use windows_sys::Win32::System::JobObjects::{
        JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    let held = job::hold_test_process().expect("the test process is held in a job");
    let flags = held.limit_flags().unwrap();
    assert_ne!(flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, 0, "ends what it holds: {flags:#x}");
    assert_ne!(flags & JOB_OBJECT_LIMIT_BREAKAWAY_OK, 0, "lets breakaway go: {flags:#x}");
}

/// HUN-243: Git Bash's `bin\bash.exe` is a launcher for `usr\bin\bash.exe`,
/// which left the shell's job as it started and outlived the pane, until
/// MSYS ran out of consoles. Git is on CI's Windows runners.
#[cfg(windows)]
#[tokio::test]
async fn nothing_a_git_bash_pane_started_outlives_it() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        PROCESS_TERMINATE,
    };

    let launcher = std::path::Path::new(r"C:\Program Files\Git\bin\bash.exe");
    if !launcher.exists() {
        eprintln!("no Git Bash at {}; skipping", launcher.display());
        return;
    }
    let spec = Spawn::Program {
        program: launcher.to_string_lossy().into_owned(),
        // Interactive, as a shell pane's is. Arguments would need quoting
        // that `cmd /C` mangles once the program's path is quoted too.
        args: Vec::new(),
        env: Vec::new(),
        resource_policy: ResourcePolicy::Unrestricted,
    };
    let pane = PaneRuntime::spawn(PaneId(91), &std::env::temp_dir(), spec, |_| {}).unwrap();

    // The console server, cmd.exe, the launcher and the bash it starts.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while pane.job.active_processes().unwrap() < 4 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the launcher's bash never joined the pane's job: {:?}",
            pane.job.process_ids()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let watched: Vec<(u32, OwnedHandle)> = pane
        .job
        .process_ids()
        .unwrap()
        .into_iter()
        .filter_map(|pid| {
            let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
            (!raw.is_null()).then(|| (pid, unsafe { OwnedHandle::from_raw_handle(raw as _) }))
        })
        .collect();

    pane.kill().unwrap();

    let survivors: Vec<u32> = watched
        .iter()
        .filter(|(_, process)| {
            let waited = unsafe { WaitForSingleObject(process.as_raw_handle() as _, 10_000) };
            if waited != WAIT_OBJECT_0 {
                unsafe { TerminateProcess(process.as_raw_handle() as _, 1) };
                return true;
            }
            false
        })
        .map(|(pid, _)| *pid)
        .collect();
    assert!(survivors.is_empty(), "{survivors:?} outlived the Git Bash pane");
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

    let live = pane.scrollback(0, None);
    assert_eq!(live.offset, 0);
    assert!(live.depth > 0, "60 lines do not fit in 24 rows");
    assert!(
        !rows_of(&live.cells).iter().any(|r| r == "line1"),
        "the first line is off the live screen: {:?}",
        rows_of(&live.cells)
    );

    let back = pane.scrollback(live.depth, None);
    assert_eq!(back.offset, live.depth, "clamped to the oldest line it kept");
    assert!(
        rows_of(&back.cells).iter().any(|r| r == "line1"),
        "the first line is recoverable: {:?}",
        rows_of(&back.cells)
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
            ServerMsg::RowDamage { pane, runs, .. } => {
                assert_eq!(pane, PaneId(7), "damage must be tagged with its own pane");
                seen.push_str(&run_text(&runs));
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

fn run_text(runs: &[argus_protocol::CellRun]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
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

    let (_, _, grid, _, _, _, _, _rx) = pane.snapshot_and_subscribe();
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
            if let Ok(ServerMsg::RowDamage { runs, .. }) = rx.recv().await {
                text.push_str(&run_text(&runs));
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

#[cfg(unix)]
#[tokio::test]
async fn a_silent_child_leaves_its_pump_asleep() {
    // The pump used to wake every frame for the life of the pane just to
    // ask whether the child had exited; twelve idle panes cost the daemon
    // hundreds of wakeups a second doing nothing. A child that says
    // nothing should now cost nothing until it exits.
    let pane = PaneRuntime::spawn(
        PaneId(5),
        &std::env::temp_dir(),
        Spawn::Program {
            program: "sleep".to_string(),
            args: vec!["30".to_string()],
            env: Vec::new(),
            resource_policy: ResourcePolicy::Unrestricted,
        },
        |_| {},
    )
    .unwrap();
    let mut rx = pane.subscribe();

    tokio::time::sleep(Duration::from_millis(100)).await;
    let settled = pane.pump_wakes.load(std::sync::atomic::Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(400)).await;
    let later = pane.pump_wakes.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(later, settled, "an idle pump woke {} times", later - settled);

    // And the exit still arrives without anything polling for it.
    pane.kill().unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(ServerMsg::PaneClosed { .. }) = rx.recv().await {
                return;
            }
        }
    })
    .await
    .expect("the exit should reach the pump");
}

#[cfg(unix)]
#[tokio::test]
async fn a_child_that_asks_where_its_cursor_is_hears_back() {
    // The reply comes back as input, so the child reads it raw — a line
    // discipline would hold it for a newline it never gets — and prints it
    // in a form the grid can show.
    let pane = PaneRuntime::spawn(
        PaneId(6),
        &std::env::temp_dir(),
        Spawn::Program {
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "stty raw -echo; printf '\\033[3;5H\\033[6n'; \
                 reply=$(dd bs=1 count=6 2>/dev/null); stty sane; \
                 printf '\\033[1;1Hreply=%s' \"$(printf '%s' \"$reply\" | tr -d '\\033')\""
                    .to_string(),
            ],
            env: Vec::new(),
            resource_policy: ResourcePolicy::Unrestricted,
        },
        |_| {},
    )
    .unwrap();
    wait_for(&pane, |g| grid_contains(g, "reply=[3;5R")).await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_watcher_rebuilt_from_damage_alone_ends_on_the_screen() {
    // What the pump sends is diffed against what it believes the watcher
    // holds; if the two ever drift, a pane draws text that has left its
    // screen. Coloured lines scrolling in bursts exercise the emulator's
    // damage, the scroll detection and the runs together.
    let script = "for i in $(seq 1 600); do \
                  printf '\\033[3%dmline %d of the stream\\033[0m\\n' $((i % 7)) $i; \
                  [ $((i % 40)) -eq 0 ] && sleep 0.02; done; echo stream-done; sleep 5";
    let pane = PaneRuntime::spawn(
        PaneId(7),
        &std::env::temp_dir(),
        Spawn::Program {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            env: Vec::new(),
            resource_policy: ResourcePolicy::Unrestricted,
        },
        |_| {},
    )
    .unwrap();
    let (_, _, mut watcher, _, _, _, _, mut rx) = pane.snapshot_and_subscribe();
    let mut frames = 0;
    tokio::time::timeout(Duration::from_secs(20), async {
        while !grid_contains(&watcher, "stream-done") {
            if let Ok(ServerMsg::RowDamage { scroll, runs, .. }) = rx.recv().await {
                if let Some(scroll) = scroll {
                    scroll.apply(&mut watcher);
                }
                for run in &runs {
                    run.apply(&mut watcher);
                }
                frames += 1;
            }
        }
    })
    .await
    .expect("the stream should reach the watcher");
    let (_, _, screen, _, _, _) = pane.full_snapshot();
    assert!(frames > 1, "one frame proves nothing about the ones after it");
    assert_eq!(watcher, screen);
    let _ = pane.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn killing_a_pane_also_kills_a_detached_grandchild() {
    // A shell backgrounding a job (e.g. an agent's own subprocess) leaves a
    // grandchild that never setsid's away from the pane's session. Killing
    // only the direct child must not leave it running.
    let pid_file = std::env::temp_dir().join(format!("argus-grandchild-{}.pid", std::process::id()));
    let _ = std::fs::remove_file(&pid_file);

    let pane = PaneRuntime::spawn(
        PaneId(5),
        &std::env::temp_dir(),
        Spawn::DefaultShell,
        |_| {},
    )
    .unwrap();
    let input = pane.input();
    tokio::time::sleep(Duration::from_millis(300)).await;
    input
        .write(
            format!(
                "(sleep 30 & echo $! > {}) &\n",
                pid_file.display()
            )
            .as_bytes(),
        )
        .unwrap();

    let grandchild_pid: i32 = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(contents) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = contents.trim().parse() {
                    return pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("grandchild should announce its pid");

    pane.kill().unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let still_alive = unsafe { libc::kill(grandchild_pid, 0) == 0 };
    let _ = std::fs::remove_file(&pid_file);
    assert!(
        !still_alive,
        "grandchild pid {grandchild_pid} survived the pane kill"
    );
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
                m @ ServerMsg::PaneRows { .. } => return m,
                _ => continue,
            }
        }
    })
    .await
    .expect("a snapshot should follow a resize");

    let ServerMsg::PaneRows {
        pane: id,
        rows,
        cols,
        runs,
        ..
    } = msg
    else {
        unreachable!()
    };
    assert_eq!(id, PaneId(6));
    assert_eq!((rows, cols), (40, 120));
    assert_eq!(argus_protocol::grid_from_runs(rows, cols, &runs).len(), 40);
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
    let vt = Arc::new(StdMutex::new(Vt::new(24, 80, 0)));
    let (tx, _keep) = broadcast::channel(64);

    for trial in 0..1_000 {
        vt.lock().unwrap().process(b"[2J[H");
        let mut rx = tx.subscribe();

        let held = vt.lock().unwrap();
        std::thread::scope(|scope| {
            let publisher = {
                let (vt, tx) = (vt.clone(), tx.clone());
                scope.spawn(move || publish_snapshot(&vt, &tx, PaneId(61)))
            };
            // The pump, as far as this matters: it changes the screen and
            // announces the change without ever letting go of the emulator.
            let pump = {
                let (vt, tx) = (vt.clone(), tx.clone());
                scope.spawn(move || {
                    let mut vt = vt.lock().unwrap();
                    vt.process(b"ZZZ");
                    let _ = tx.send(ServerMsg::RowDamage {
                        pane: PaneId(61),
                        scroll: None,
                        runs: Vec::new(),
                        cursor: vt.cursor(),
                        mouse: vt.mouse(),
                        alternate_screen: false,
                        keyboard: Default::default(),
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
            .position(|m| matches!(m, ServerMsg::RowDamage { .. }));
        let snapshot = sent
            .iter()
            .enumerate()
            .find_map(|(i, m)| match m {
                ServerMsg::PaneRows {
                    rows, cols, runs, ..
                } => Some((i, argus_protocol::grid_from_runs(*rows, *cols, runs))),
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

// --- the emulator's screen, as the protocol's ---------------------------

#[test]
fn an_untouched_cell_becomes_a_blank() {
    let c = convert_cell(&Default::default());
    assert_eq!(c.ch, " ");
    assert_eq!(c.fg, Color::Default);
    assert_eq!(c.bg, Color::Default);
}

#[test]
fn colors_convert_across_every_form() {
    use alacritty_terminal::vte::ansi::{Color as TermColor, NamedColor, Rgb};

    assert_eq!(convert_color(TermColor::Named(NamedColor::Foreground)), Color::Default);
    assert_eq!(convert_color(TermColor::Named(NamedColor::Background)), Color::Default);
    assert_eq!(convert_color(TermColor::Named(NamedColor::Red)), Color::Idx(1));
    assert_eq!(convert_color(TermColor::Named(NamedColor::BrightBlue)), Color::Idx(12));
    assert_eq!(convert_color(TermColor::Named(NamedColor::DimRed)), Color::Idx(1));
    assert_eq!(convert_color(TermColor::Indexed(200)), Color::Idx(200));
    assert_eq!(
        convert_color(TermColor::Spec(Rgb { r: 1, g: 2, b: 3 })),
        Color::Rgb(1, 2, 3)
    );
}

#[test]
fn a_wide_character_is_one_cell_and_a_blank_behind_it() {
    let mut vt = Vt::new(1, 4, 0);
    vt.process("世x".as_bytes());
    let grid = vt.grid();
    assert_eq!(grid[0][0].ch, "世");
    assert_eq!(grid[0][1].ch, " ");
    assert_eq!(grid[0][2].ch, "x");
}

#[test]
fn only_the_lines_that_changed_are_read_again() {
    let mut vt = Vt::new(4, 10, 0);
    let mut live = Vec::new();
    vt.process(b"one\r\ntwo");
    vt.refresh(&mut live);
    assert_eq!(rows_of(&live)[..2], ["one", "two"]);

    // Stale the copy by hand: a line refresh skips must keep what it had.
    live[0][0].ch = "Z".into();
    vt.process(b"\x1b[2;1HTWO");
    vt.refresh(&mut live);
    assert_eq!(rows_of(&live)[..2], ["Zne", "TWO"]);
    assert_eq!(rows_of(&vt.grid())[0], "one", "the screen itself was never touched");
}

#[test]
fn a_refreshed_copy_of_another_size_is_rebuilt_whole() {
    let mut vt = Vt::new(4, 10, 0);
    let mut live = Vec::new();
    vt.process(b"hi");
    vt.refresh(&mut live);
    vt.resize(6, 12);
    vt.refresh(&mut live);
    assert_eq!((live.len(), live[0].len()), (6, 12));
    assert_eq!(live, vt.grid());
}

#[test]
fn a_synchronized_update_is_drawn_whole_or_once_it_is_overdue() {
    let mut vt = Vt::new(2, 10, 0);
    vt.process(b"\x1b[?2026hhalf");
    assert_eq!(rows_of(&vt.grid())[0], "", "held back until the update ends");
    assert!(vt.sync_deadline().is_some());
    vt.process(b"way\x1b[?2026l");
    assert_eq!(rows_of(&vt.grid())[0], "halfway");
    assert!(vt.sync_deadline().is_none());

    // One the child never ends is drawn when its deadline passes.
    vt.process(b"\x1b[?2026h\r\nstuck");
    let deadline = vt.sync_deadline().expect("an update is open");
    assert!(!vt.end_overdue_sync(), "not due yet");
    std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
    assert!(vt.end_overdue_sync());
    assert_eq!(rows_of(&vt.grid())[1], "stuck");
}

#[test]
fn a_parsed_screen_snapshots_to_a_full_rectangular_grid() {
    let mut vt = Vt::new(4, 10, 0);
    vt.process(b"hi");
    let grid = vt.grid();
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
    let mut vt = Vt::new(2, 4, 0);
    vt.process(b"\x1b[41m\x1b[K");
    let grid = vt.grid();
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
    let mut vt = Vt::new(1, 4, 0);
    vt.process("e\u{301}x".as_bytes());
    let grid = vt.grid();
    assert_eq!(grid[0][0].ch, "e\u{301}");
    assert_eq!(grid[0][1].ch, "x");
}

#[test]
fn sgr_attributes_survive_the_snapshot() {
    let mut vt = Vt::new(1, 10, 0);
    vt.process(b"\x1b[1;3;4;7;31mX\x1b[0m");
    let grid = vt.grid();
    let c = &grid[0][0];
    assert_eq!(c.ch, "X");
    assert!(c.bold && c.italic && c.underline && c.reverse, "{c:?}");
    assert_eq!(c.fg, Color::Idx(1), "red");
}

#[test]
fn cursor_movement_and_erase_are_honored_not_appended() {
    let mut vt = Vt::new(2, 10, 0);
    vt.process(b"abcdef\x1b[H\x1b[Kxy");
    assert_eq!(rows_of(&vt.grid())[0], "xy");
}

#[test]
fn cursor_position_and_visibility_survive_vt_parsing() {
    let mut vt = Vt::new(4, 10, 0);
    vt.process(b"\x1b[3;5H\x1b[?25l\x1b[6 q");
    assert_eq!(
        vt.cursor(),
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

/// The user's request (strip the paste markers out of pasted text before
/// wrapping it, as xterm and kitty do) and ARCHITECTURE.md's paste rule.
#[test]
fn a_pasted_end_marker_cannot_end_the_paste_early() {
    assert_eq!(
        paste_bytes(b"ls\x1b[201~rm -rf ~\n", true),
        b"\x1b[200~lsrm -rf ~\n\x1b[201~"
    );
    assert_eq!(
        paste_bytes(b"a\x1b[200~b\x1b[20\x1b[201~1~c", true),
        b"\x1b[200~abc\x1b[201~"
    );
}

/// ARCHITECTURE.md: the daemon wraps a paste, and so strips its markers, only
/// when the child has requested bracketed paste.
#[test]
fn an_unbracketed_paste_is_passed_through_untouched() {
    assert_eq!(paste_bytes(b"a\x1b[201~b", false), b"a\x1b[201~b");
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    /// Text built from pieces of the markers, so the generator reaches
    /// whole markers, split ones and ones that form only after another is
    /// removed, which uniform bytes almost never do.
    fn pasted_text() -> impl Strategy<Value = Vec<u8>> {
        let piece = prop_oneof![
            Just(b"\x1b[200~".to_vec()),
            Just(b"\x1b[201~".to_vec()),
            Just(b"\x1b[20".to_vec()),
            Just(b"0~".to_vec()),
            Just(b"1~".to_vec()),
            Just(b"\x1b".to_vec()),
            Just(b"[".to_vec()),
            proptest::collection::vec(any::<u8>(), 0..4),
        ];
        proptest::collection::vec(piece, 0..12).prop_map(|pieces| pieces.concat())
    }

    /// Text that comes close to a marker without holding one: escapes and
    /// marker fragments that never complete, so the property below is not
    /// spent discarding cases.
    fn near_miss_text() -> impl Strategy<Value = Vec<u8>> {
        let piece = prop_oneof![
            Just(b"\x1b[202~".to_vec()),
            Just(b"\x1b[200".to_vec()),
            Just(b"\x1b[2".to_vec()),
            Just(b"\x1b[".to_vec()),
            Just(b"0~".to_vec()),
            Just(b"1~".to_vec()),
            proptest::collection::vec(any::<u8>().prop_filter("no escape", |b| *b != 0x1b), 0..4),
        ];
        proptest::collection::vec(piece, 0..12)
            .prop_map(|pieces| pieces.concat())
            .prop_filter("holds a marker", |text| {
                !contains(text, PASTE_START) && !contains(text, PASTE_END)
            })
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|window| window == needle)
    }

    proptest! {
        /// The user's request (strip the paste markers out of pasted text
        /// before wrapping it, as xterm and kitty do) and ARCHITECTURE.md's rule
        /// that a paste reaches the child whole: between the delimiters the
        /// daemon adds, no marker of either kind survives.
        #[test]
        fn a_bracketed_paste_holds_no_marker_but_its_own(text in pasted_text()) {
            let pasted = paste_bytes(&text, true);
            prop_assert!(pasted.starts_with(PASTE_START));
            prop_assert!(pasted.ends_with(PASTE_END));
            let body = &pasted[PASTE_START.len()..pasted.len() - PASTE_END.len()];
            prop_assert!(!contains(body, PASTE_START));
            prop_assert!(!contains(body, PASTE_END));
        }

        /// The same request: only the markers are stripped, so text that
        /// holds none crosses byte for byte.
        #[test]
        fn text_without_markers_is_pasted_unchanged(text in near_miss_text()) {
            let pasted = paste_bytes(&text, true);
            prop_assert_eq!(&pasted[PASTE_START.len()..pasted.len() - PASTE_END.len()], &text[..]);
        }
    }
}

/// Shows what ConPTY itself sends around a resize to a child that sits on
/// the alternate screen, and that the pane stays there through it. ConPTY
/// forwards `?1049h` but swallows the mouse modes (`?1000`-`?1007`), so the
/// pane never learns of a child's mouse request; nothing here resets it.
#[cfg(windows)]
#[tokio::test]
async fn a_child_on_the_alternate_screen_stays_there_through_resizes() {
    let script = "$e=[char]27; [Console]::Out.Write(\"$e[?1049h$e[?1000h$e[?1002h$e[?1003h$e[?1006h$e[?1007h$e[?2004h$e[?2026h$e[?2026l$e[2J$e[Halt-marker\"); Start-Sleep 30";
    let pane = PaneRuntime::spawn(
        PaneId(77),
        &std::env::temp_dir(),
        Spawn::Program {
            program: "powershell.exe".to_string(),
            args: vec!["-NoProfile".to_string(), "-Command".to_string(), script.to_string()],
            env: Vec::new(),
            resource_policy: ResourcePolicy::Unrestricted,
        },
        |_| {},
    )
    .unwrap();
    wait_for(&pane, |g| grid_contains(g, "alt-marker")).await;
    let before = pane.vt.lock().unwrap().raw.len();
    for (rows, cols) in [(30, 100), (20, 70), (40, 120), (24, 80)] {
        pane.resize(rows, cols).unwrap();
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
    let vt = pane.vt.lock().unwrap();
    eprintln!("startup bytes: {:?}", String::from_utf8_lossy(&vt.raw[..before]));
    eprintln!("after resize bytes: {:?}", String::from_utf8_lossy(&vt.raw[before..]));
    let alternate = vt.alternate_screen();
    drop(vt);
    let _ = pane.kill();
    assert!(alternate, "the pane left the alternate screen");
}

#[test]
fn a_screen_switch_is_logged_with_the_request_that_made_it() {
    let mut chunk = vec![b'x'; 500];
    chunk.extend_from_slice(b"\x1b[?1049l");
    chunk.extend_from_slice(&[b'y'; 500]);
    let context = switch_context(&chunk);
    assert!(context.contains("\\x1b[?1049l"), "{context}");
    assert!(context.len() < 300, "only the bytes around it: {context}");
}

#[test]
fn a_switch_with_no_request_in_the_chunk_logs_its_tail() {
    let mut chunk = vec![b'x'; 500];
    chunk.extend_from_slice(b"end");
    assert!(switch_context(&chunk).ends_with("end"));
}

/// A real shell is at its prompt while nothing it started runs, and not
/// while a command does: what decides whether a `cd` may be typed into it.
#[tokio::test]
async fn a_shell_is_at_its_prompt_only_while_nothing_it_started_runs() {
    let pane = PaneRuntime::spawn(PaneId(91), &std::env::temp_dir(), Spawn::DefaultShell, |_| {})
        .unwrap();
    // Until its prompt is drawn the shell may still be starting, and its
    // startup scripts may run programs of their own.
    wait_for(&pane, |g| rows_of(g).iter().any(|row| !row.trim().is_empty())).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(matches!(pane.shell(), Some(ShellState::AtPrompt(_))), "{:?}", pane.shell());

    // Only the command's own output says it started: the typed line is
    // echoed before it runs.
    let (command, says) = if cfg!(windows) {
        (&b"ping -n 30 127.0.0.1\r"[..], "bytes")
    } else {
        (&b"printf 'argus-%s\\n' busy; sleep 30\r"[..], "argus-busy")
    };
    pane.input().write(command).unwrap();
    wait_for(&pane, |g| grid_contains(g, says)).await;
    assert_eq!(pane.shell(), Some(ShellState::Running));
    let _ = pane.kill();
}

/// The keys a move types reach the directory even with something left
/// half-typed at the prompt, which they clear first rather than run.
#[tokio::test]
async fn a_cd_typed_over_a_half_typed_line_moves_the_shell_and_runs_nothing_else() {
    let target = tempfile::Builder::new().prefix("argus-moved-").tempdir().unwrap();
    let pane = PaneRuntime::spawn(PaneId(92), &std::env::temp_dir(), Spawn::DefaultShell, |_| {})
        .unwrap();
    wait_for(&pane, |g| rows_of(g).iter().any(|row| !row.trim().is_empty())).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let Some(ShellState::AtPrompt(shell)) = pane.shell() else {
        panic!("the shell is not at its prompt: {:?}", pane.shell());
    };
    // Printed upper-cased, so only the output matches, not the typed line.
    let print = match shell {
        Shell::Posix | Shell::Fish => &b"echo \"$PWD\" | tr a-z A-Z\r"[..],
        Shell::Pwsh => &b"$PWD.Path.ToUpper()\r"[..],
        // cmd cannot upper-case its output.
        Shell::Cmd => return,
    };

    pane.input().write(b"echo argus-junk-").unwrap();
    pane.input().write(&shell.cd(target.path()).unwrap()).unwrap();
    pane.input().write(print).unwrap();

    wait_for(&pane, |g| grid_contains(g, "ARGUS-MOVED-")).await;
    let _ = pane.kill();
}
