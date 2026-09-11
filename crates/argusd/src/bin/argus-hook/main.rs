//! The command Argus's managed agent hooks run, and the one an agent runs
//! itself to say what it is doing.
//!
//! ```text
//! argus-hook title "fixing the pty deadlock"
//! argus-hook status waiting "needs the staging database password"
//! argus-hook status needs-review "ready for review"
//! argus-hook status done "reviewed and complete"
//! argus-hook status working
//! argus-hook checkout                            # reports the current directory
//! argus-hook session <id>                        # records exact resume identity
//! argus-hook comments                            # reads durable review feedback
//! argus-hook feature                            # the feature this checkout is on
//! argus-hook feature list                       # every feature of the project
//! argus-hook feature open "decision scoping"    # opens one and works on it
//! argus-hook feature use decision-scoping       # works on one that already exists
//! argus-hook feature note "the board is per feature"   # adds to its document
//! argus-hook feature export                  # the feature as material to write up
//! argus-hook decisions                          # this feature's decision board
//! argus-hook decide "sqlite" --over "a file per feature" --because "both need migrations"
//! argus-hook decide "one row per note" --under 3  # hangs under decision 3
//! argus-hook decide "one row per note" --supersedes 7   # replaces decision 7
//! argus-hook say "text"                          # prints, calls nobody
//! argus-hook instructions                        # prints inherited startup context
//! argus-hook <url> <token> [--note-from-stdin] [--title-from-stdin]  # the installed hook form
//! ```
//!
//! The named forms read `ARGUS_HOOK_URL` and `ARGUS_HOOK_TOKEN` from the
//! environment, which every agent pane is handed. That is what makes status
//! harness-agnostic: a CLI that can run one command at some point in its
//! lifecycle needs nothing from Argus but these variables. The explicit form
//! is what Argus writes into a harness's own hook config, where there is no
//! guarantee the environment survives.
//!
//! It **always exits 0**, whatever happens. That is the entire reason it
//! exists instead of a `curl` invocation: a hook command that exits non-zero
//! is reported to the user as a failed turn. A daemon that has since exited —
//! or a port that now belongs to nobody — must degrade to "pane status stops
//! updating", never to an error on every prompt in that directory. `curl`
//! exits 7 on a refused connection, which is exactly what this avoids.
//!
//! Installed hooks write only the JSON the runner needs to let the turn
//! continue — Cursor wants `permission`, Claude wants `decision` — never a
//! human-readable message. Some agent CLIs inject a hook's stdout into the
//! model's context, so staying silent keeps Argus's bookkeeping out of the
//! conversation. The deliberate `say`, `instructions`, `comments`, `feature`,
//! `task`, `decisions`, and `decide` commands do return useful output.
//!
//! On Windows it is a GUI-subsystem binary. Not because it has a UI — it
//! has none — but because the agent CLI that runs it decides how it is
//! spawned, and we cannot ask that CLI to pass `CREATE_NO_WINDOW`. A
//! console-subsystem binary spawned from a process without a console gets
//! its own console *window*, which flashes on screen on every hook event.
//! Declaring the GUI subsystem means no console is ever allocated. Safe
//! precisely because this program reads and writes nothing on stdio it was
//! not handed.
//!
//! The commands live in modules by what they talk about: `board` for the
//! shared work an agent reads and writes, `installed` for the form a
//! harness's hook config runs, and `transport` for how either reaches the
//! daemon.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use argus_protocol::{
    Decision, DecisionBoard, DecisionWrite, Endpoint, FeatureAction, FeatureBoard, FeatureWrite,
    Report, ReviewComment, TaskAction, TaskList, TaskState, TaskWrite, INSTRUCTIONS_COMMAND,
    INSTRUCTIONS_VAR, NOTE_FLAG, OWNS_SESSION_FLAG, SESSION_HEADER, SESSION_KEY_FLAG, TITLE_FLAG,
    TOKEN_VAR, URL_VAR,
};

const TIMEOUT: Duration = Duration::from_secs(2);
const ARTIFACT_SCOPE_VAR: &str = "ARGUS_ARTIFACT_SCOPE";

mod board;
mod installed;
mod transport;

use board::*;
use installed::*;
use transport::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    dispatch(args.first().map(String::as_str), &rest);
}

type NamedHandler = fn(&[&str]);

const NAMED_HANDLERS: &[(&str, NamedHandler)] = &[
    ("say", say),
    (INSTRUCTIONS_COMMAND, instructions),
    ("title", title),
    ("status", status),
    ("checkout", checkout),
    ("session", session),
    ("comments", comments),
    ("feature", feature),
    ("task", task),
    ("decisions", decisions),
    ("decide", decide),
];

fn dispatch(command: Option<&str>, rest: &[&str]) {
    match command {
        // The installed-hook form uses an absolute URL and token because a
        // harness's hook config cannot count on inheriting the environment.
        Some(url) if url.starts_with("http://") => installed_hook(url, rest),
        Some(name) => {
            if let Some((_, handler)) = NAMED_HANDLERS.iter().find(|(key, _)| *key == name) {
                handler(rest);
            }
        }
        None => {}
    }
}

fn say(rest: &[&str]) {
    // Deliberately on stdout: this is context for the model.
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", rest.join(" "));
    let _ = out.flush();
}

fn instructions(_: &[&str]) {
    // Read in the helper, not in a shell command string: multiline context
    // and paths with shell metacharacters must remain data, never shell code.
    say(&[&env_instructions()]);
}

fn title(rest: &[&str]) {
    let text = rest.join(" ");
    if !text.trim().is_empty() {
        let _ = post(
            &endpoint_url(&env_url(), Endpoint::Title),
            &env_token(),
            &text,
        );
    }
}

fn status(rest: &[&str]) {
    // A state the pane API has no name for could only ever be refused at the
    // other end, so it is refused here instead of travelling.
    let Some(report) = rest.first().and_then(|s| Report::parse(s)) else {
        return;
    };
    // Anything after the state is the reason, so
    // `status waiting "needs a password"` reads the way you'd say it.
    let note = rest[1..].join(" ");
    let _ = post(
        &endpoint_url(&env_url(), Endpoint::Status(report)),
        &env_token(),
        &note,
    );
}

fn checkout(rest: &[&str]) {
    if let Some(path) = reported_checkout(rest) {
        let _ = post(
            &endpoint_url(&env_url(), Endpoint::Checkout),
            &env_token(),
            &path.to_string_lossy(),
        );
    }
}

fn session(rest: &[&str]) {
    let id = rest.join(" ");
    if !id.is_empty() {
        let _ = post(
            &endpoint_url(&env_url(), Endpoint::Session),
            &env_token(),
            &id,
        );
    }
}

fn reported_checkout(args: &[&str]) -> Option<std::path::PathBuf> {
    if args.is_empty() {
        std::env::current_dir().ok()
    } else {
        Some(std::path::PathBuf::from(args.join(" ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_the_conversation_it_came_from() {
        // What lets the daemon tell the pane's own agent from a CLI started
        // inside it, which inherits the same URL and token.
        let tagged = request(
            "/pane/1/status/idle",
            "127.0.0.1:4242",
            "tok",
            Some("s-1"),
            "",
        );
        assert!(tagged.contains("\r\nX-Argus-Session: s-1\r\n"), "{tagged}");
        let untagged = request("/pane/1/status/idle", "127.0.0.1:4242", "tok", None, "");
        assert!(!untagged.contains("X-Argus-Session"), "{untagged}");
        assert!(untagged.contains("\r\nContent-Length: 0\r\n"), "{untagged}");
    }

    #[test]
    fn every_header_starts_its_own_line_at_column_zero() {
        // An indented header line is a continuation of the one above it, so
        // a stray space here is invisible on the wire and silently costs the
        // daemon whichever header it swallowed: a session header it misses
        // files a child's report on its parent's row, and a Content-Length
        // it misses drops the note the report was carrying.
        let req = request("/pane/1/title", "127.0.0.1:4242", "tok", Some("s-1"), "hi");
        let (head, body) = req
            .split_once("\r\n\r\n")
            .expect("a blank line ends the headers");
        assert_eq!(body, "hi");
        for line in head.split("\r\n") {
            assert_eq!(line.trim_start(), line, "indented header line: {req:?}");
            assert!(!line.is_empty(), "blank header line: {req:?}");
        }
        assert!(head.contains("\r\nContent-Length: 2\r\n"), "{req:?}");
    }

    #[test]
    fn a_json_event_gives_up_the_message_a_human_would_read() {
        assert_eq!(
            note_from(
                r#"{"session_id":"x","message":"Claude needs your permission to run tests"}"#
            ),
            "Claude needs your permission to run tests"
        );
    }

    #[test]
    fn one_json_event_can_supply_a_note_and_session_id() {
        let raw = r#"{"session_id":"session-123","message":"waiting"}"#;
        assert_eq!(note_from(raw), "waiting");
        assert_eq!(
            json_string(raw, "session_id").as_deref(),
            Some("session-123")
        );
    }

    #[test]
    fn cursor_session_start_names_the_id_session_id() {
        // sessionStart's documented payload uses session_id; other events
        // put the same value on conversation_id. The helper asks for either.
        let start = r#"{"session_id":"conv-9","composer_mode":"agent"}"#;
        assert_eq!(
            json_string(start, "conversation_id").as_deref(),
            Some("conv-9")
        );
        let tool = r#"{"conversation_id":"conv-9","tool_name":"Shell"}"#;
        assert_eq!(json_string(tool, "session_id").as_deref(), Some("conv-9"));
    }

    #[test]
    fn hook_stdin_stops_at_one_json_object_without_waiting_for_eof() {
        struct JsonThenHang {
            data: &'static [u8],
        }
        impl Read for JsonThenHang {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.data.is_empty() {
                    panic!("hook stdin was read past the JSON object");
                }
                let n = self.data.len().min(buf.len());
                buf[..n].copy_from_slice(&self.data[..n]);
                self.data = &self.data[n..];
                Ok(n)
            }
        }
        let raw = read_hook_input(JsonThenHang {
            data: br#"{"conversation_id":"conv-9","tool_name":"Shell"}"#,
        });
        assert_eq!(
            json_string(&raw, "conversation_id").as_deref(),
            Some("conv-9")
        );
    }

    #[test]
    fn hook_stdin_plain_text_still_reads_to_eof() {
        assert_eq!(
            read_hook_input(std::io::Cursor::new("waiting on review\n")),
            "waiting on review\n"
        );
    }

    #[test]
    fn cursor_tool_hooks_allow_with_permission_and_claude_with_decision() {
        assert_eq!(
            hook_reply(
                Some(r#"{"tool_name":"Shell","conversation_id":"c"}"#),
                false,
                ""
            ),
            r#"{"permission":"allow"}"#
        );
        assert_eq!(
            hook_reply(Some(r#"{"toolCall":{"name":"Bash"}}"#), false, ""),
            r#"{"decision":"allow"}"#
        );
        assert_eq!(hook_reply(Some(r#"{"session_id":"c"}"#), false, ""), "{}");
    }

    #[test]
    fn a_checkout_wide_hook_url_rebases_to_the_process_pane() {
        assert_eq!(
            rebase_hook_url(
                "http://127.0.0.1:4242/pane/1/status/idle",
                "http://127.0.0.1:4242/pane/9"
            )
            .as_deref(),
            Some("http://127.0.0.1:4242/pane/9/status/idle")
        );
        assert!(rebase_hook_url(
            "http://127.0.0.1:4242/pane/1/status/idle",
            "http://127.0.0.1:9999/pane/9"
        )
        .is_none());
    }

    #[test]
    fn a_rebased_hook_uses_the_process_token_too() {
        assert_eq!(
            routed_hook(
                "http://127.0.0.1:4242/pane/1/status/idle",
                "configured-token",
                "http://127.0.0.1:4242/pane/9",
                "process-token"
            ),
            (
                "http://127.0.0.1:4242/pane/9/status/idle".to_string(),
                "process-token".to_string()
            )
        );
    }

    #[test]
    fn an_incomplete_or_foreign_process_pair_keeps_the_configured_pair() {
        let configured = "http://127.0.0.1:4242/pane/1/status/idle";
        assert_eq!(
            routed_hook(configured, "configured-token", "", "process-token"),
            (configured.to_string(), "configured-token".to_string())
        );
        assert_eq!(
            routed_hook(
                configured,
                "configured-token",
                "http://127.0.0.1:9999/pane/9",
                "process-token"
            ),
            (configured.to_string(), "configured-token".to_string())
        );
        assert_eq!(
            routed_hook(
                configured,
                "configured-token",
                "http://127.0.0.1:4242/pane/9",
                ""
            ),
            (configured.to_string(), "configured-token".to_string())
        );
    }

    #[test]
    fn plain_text_falls_back_to_its_first_real_line() {
        // A harness that hands its hooks text rather than JSON.
        assert_eq!(
            note_from("\n\n  waiting on review  \nmore"),
            "waiting on review"
        );
    }

    #[test]
    fn json_with_nothing_recognizable_shows_nothing() {
        // Better an empty note than a wall of serialized event under a row.
        assert_eq!(note_from(r#"{"session_id":"x","cwd":"/tmp"}"#), "");
        assert_eq!(note_from(""), "");
    }

    #[test]
    fn a_prompt_event_gives_up_the_text_the_daemon_should_name_the_row() {
        // Cursor beforeSubmitPrompt and Claude UserPromptSubmit both put
        // the user's text on `prompt`. That is what a column of "claude"
        // rows is missing: the task, without waiting for the model to
        // remember `argus-hook title`.
        assert_eq!(
            title_from(
                r#"{"conversation_id":"c","prompt":"fixing the pty deadlock","attachments":[]}"#
            ),
            "fixing the pty deadlock"
        );
        assert_eq!(
            title_from(r#"{"session_id":"s","prompt":"  review split view  "}"#),
            "review split view"
        );
    }

    #[test]
    fn a_tool_start_event_is_not_a_title() {
        // preToolUse stdin names a tool. Using that as the row would label
        // every working pane "Shell".
        assert_eq!(
            title_from(r#"{"conversation_id":"c","tool_name":"Shell","toolCall":{"name":"Bash"}}"#),
            ""
        );
        assert_eq!(title_from(r#"{"session_id":"x","cwd":"/tmp"}"#), "");
    }

    #[test]
    fn a_title_flag_reads_the_prompt_without_turning_it_into_a_note() {
        let raw = r#"{"conversation_id":"c","prompt":"fixing the pty deadlock"}"#;
        let (key, body, note, title) = installed_input(
            &["tok", TITLE_FLAG, SESSION_KEY_FLAG, "conversation_id"],
            std::io::Cursor::new(raw),
        );
        assert_eq!(key, Some("conversation_id"));
        assert_eq!(body.as_deref(), Some(raw));
        assert_eq!(note, "");
        assert_eq!(title, "fixing the pty deadlock");
    }

    #[test]
    fn without_the_title_flag_a_prompt_event_does_not_rename_the_row() {
        let (key, _, note, title) = installed_input(
            &["tok", SESSION_KEY_FLAG, "conversation_id"],
            std::io::Cursor::new(r#"{"conversation_id":"c","prompt":"secret task"}"#),
        );
        assert_eq!(key, Some("conversation_id"));
        assert_eq!(note, "");
        assert_eq!(title, "");
    }

    #[test]
    fn a_prompt_title_posts_to_the_pane_title_endpoint() {
        use std::io::BufRead as _;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let len = head
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            (head, String::from_utf8(body).unwrap())
        });

        post_title(
            &format!("http://{address}/pane/7/status/working"),
            "tok",
            "fixing the pty deadlock",
            Some("c"),
        );
        let (head, body) = server.join().unwrap();
        assert!(
            head.starts_with("POST /pane/7/title HTTP/1.1\r\n"),
            "{head}"
        );
        assert!(head.contains("\r\nX-Argus-Session: c\r\n"), "{head}");
        assert_eq!(body, "fixing the pty deadlock");
    }

    #[test]
    fn an_explicit_checkout_path_keeps_spaces() {
        assert_eq!(
            reported_checkout(&["C:\\Source\\my", "checkout"]),
            Some(std::path::PathBuf::from("C:\\Source\\my checkout"))
        );
    }

    #[test]
    fn checkout_without_a_path_reports_the_current_directory() {
        assert_eq!(reported_checkout(&[]), std::env::current_dir().ok());
    }

    #[test]
    fn comments_are_read_from_the_daemon_and_rendered_in_order() {
        let comments = vec![ReviewComment {
            id: 4,
            anchor: argus_protocol::ReviewAnchor {
                base: argus_protocol::ReviewBase::Staged,
                commit: None,
                path: "src/main.rs".to_string(),
                old_path: None,
                old_start: Some(9),
                old_end: Some(9),
                new_start: Some(10),
                new_end: Some(10),
                text: vec!["+changed".to_string()],
            },
            body: "fix this".to_string(),
        }];
        let (address, server) = serve_once(&serde_json::to_string(&comments).unwrap());

        let message = comments_message(&[], &format!("http://{address}/pane/4"), "secret");
        let head = server.join().unwrap();

        assert_eq!(message, "#4 [staged] src/main.rs:10 `+changed`: fix this");
        assert!(head.starts_with("POST /pane/4/comments HTTP/1.1\r\n"));
        assert!(head.contains("\r\nAuthorization: Bearer secret\r\n"));
        assert_eq!(
            comments_message(&["extra"], "", ""),
            "could not read comments: comments takes no arguments"
        );
    }

    #[test]
    fn the_decision_grammar_puts_the_choice_first_and_the_rest_behind_flags() {
        let write = parse_decide_args(&[
            "one",
            "row",
            "per",
            "note",
            "--over",
            "a table per note",
            "--because",
            "the key is durable",
            "--under",
            "3",
        ])
        .unwrap();
        assert_eq!(write.chose, "one row per note");
        assert_eq!(write.over.as_deref(), Some("a table per note"));
        assert_eq!(write.because.as_deref(), Some("the key is durable"));
        assert_eq!(write.under, Some(3));
        assert_eq!(write.supersedes, None);
    }

    #[test]
    fn a_decision_that_could_not_be_recorded_says_which_part_was_wrong() {
        for (bad, expected) in [
            (vec!["--over", "nothing chosen"], "what was chosen"),
            (vec!["x", "--under"], "needs something after it"),
            (vec!["x", "--under", "zero"], "wants a decision number"),
            (vec!["x", "--why", "no"], "is not one of"),
            (vec!["x", "--under", "1", "--supersedes", "2"], "not both"),
        ] {
            let message = decide_message(&bad, "", "");
            assert!(
                message.starts_with("could not record the decision:") && message.contains(expected),
                "{bad:?} gave {message:?}"
            );
        }
    }

    #[test]
    fn a_task_brief_is_sent_whole_and_printed_beneath_its_task() {
        let list = r#"{"project_name":"argus","feature":"task-briefs","tasks":[
            {"id":7,"feature":"task-briefs","title":"render the brief",
             "body":"Keep the title compact.\nVerify multiline output.","state":"doing",
             "claimed_by":"sess-1","external":null,"position":0,"at":1,"session":null}]}"#;
        let (address, server) = serve_once(list);

        let message = task_message(
            &[
                "brief",
                "7",
                "Keep the title compact.\nVerify multiline output.",
            ],
            &format!("http://{address}/pane/4"),
            "secret",
        );
        let _ = server.join();
        assert_eq!(
            message,
            "Tasks under task-briefs:\n\
             #7   doing render the brief — sess-1\n\
             \x20     Keep the title compact.\n\
             \x20     Verify multiline output."
        );
    }

    #[test]
    fn a_recorded_decision_reports_the_id_the_next_one_hangs_off() {
        let recorded = r#"{"id":7,"parent":3,"at":1,"session":null,"checkout":null,
             "chose":"one row per note","over":null,"because":null,"superseded_by":null}"#;
        let (address, server) = serve_once(recorded);

        let message = decide_message(
            &["one", "row", "per", "note", "--under", "3"],
            &format!("http://{address}/pane/4"),
            "secret",
        );

        assert_eq!(message, "recorded decision #7 under #3: one row per note");
        assert!(server
            .join()
            .unwrap()
            .starts_with("POST /pane/4/decide HTTP/1.1\r\n"));
    }

    #[test]
    fn a_feature_is_read_as_its_brief_and_then_its_reasoning() {
        let board = r#"{"project":null,"project_name":"argus","current":"notes-storage",
            "unfiled":2,
            "features":[{"slug":"notes-storage","title":"Notes storage","body":"keys outlive ids",
                         "origin_checkout":null,"origin_branch":"notes","at":1,"session":null}],
            "decisions":[
              {"id":1,"parent":null,"at":1,"session":null,"checkout":null,
               "feature":"notes-storage","chose":"one row per note","over":"a table per note",
               "because":null,"superseded_by":null}]}"#;
        let (address, server) = serve_once(board);

        let message = feature_message(&[], &format!("http://{address}/pane/4"), "t");
        let _ = server.join();

        assert_eq!(
            message,
            "Feature: Notes storage (notes-storage)\n\
             Started on notes.\n\
             \n\
             keys outlive ids\n\
             \n\
             Decided under it, newest last:\n\
             #1 one row per note\n\
             \x20     over: a table per note"
        );
    }

    #[test]
    fn a_checkout_on_no_feature_is_told_what_to_do_about_it() {
        let board = r#"{"project":null,"project_name":"argus","current":null,"unfiled":3,
            "features":[{"slug":"the-pty-deadlock","title":"The pty deadlock","body":"",
                         "origin_checkout":null,"origin_branch":null,"at":1,"session":null}],
            "decisions":[]}"#;
        let (address, server) = serve_once(board);

        let message = feature_message(&[], &format!("http://{address}/pane/4"), "t");
        let _ = server.join();

        assert!(
            message.starts_with("This checkout is not on a feature yet"),
            "{message}"
        );
        assert!(
            message.contains("the-pty-deadlock — The pty deadlock"),
            "{message}"
        );
        assert!(message.contains("3 older decision(s)"), "{message}");
    }

    #[test]
    fn a_board_is_drawn_as_the_tree_it_is() {
        let board = r#"{"project":null,"name":"argus","decisions":[
            {"id":1,"parent":null,"at":1,"session":null,"checkout":null,
             "chose":"sqlite","over":"a file per feature",
             "because":"both need migrations","superseded_by":null},
            {"id":2,"parent":1,"at":2,"session":null,"checkout":null,
             "chose":"one table per note","over":null,"because":null,
             "superseded_by":3},
            {"id":3,"parent":1,"at":3,"session":null,"checkout":null,
             "chose":"one row per note","over":null,"because":null,
             "superseded_by":null},
            {"id":4,"parent":2,"at":4,"session":null,"checkout":null,
             "chose":"store the body","over":null,"because":null,
             "superseded_by":null}]}"#;
        let (address, server) = serve_once(board);

        let message = decisions_message(&[], &format!("http://{address}/pane/4"), "t");
        let _ = server.join();

        assert_eq!(
            message,
            "Decisions on argus, newest last:\n\
             #1 sqlite\n\
             │     over: a file per feature\n\
             │     because: both need migrations\n\
             ├─ #2 one table per note  (superseded by #3)\n\
             │  └─ #4 store the body\n\
             └─ #3 one row per note"
        );
    }

    #[test]
    fn an_empty_board_says_so_rather_than_nothing() {
        let (address, server) = serve_once(r#"{"project":null,"name":"argus","decisions":[]}"#);
        let message = decisions_message(&[], &format!("http://{address}/pane/4"), "t");
        let _ = server.join();
        assert_eq!(message, "nothing decided under this feature yet");
    }

    /// Serves one canned response at a chosen status, for the endpoint that
    /// has something to say when it refuses.
    /// Serves one canned JSON response and hands back the request head, so
    /// a rendering test can also assert what went over the wire.
    fn serve_once(body: &str) -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
        serve_response(&format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ))
    }

    fn serve_response(response: &str) -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
        use std::io::BufRead as _;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response = response.to_string();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            reader.get_mut().write_all(response.as_bytes()).unwrap();
            head
        });
        (address, server)
    }

    fn exportable_board() -> FeatureBoard {
        let feature = argus_protocol::Feature {
            slug: "streaming-the-pty".into(),
            title: "streaming the pty".into(),
            body: "the reader thread owns the handle.".into(),
            origin_checkout: None,
            origin_branch: Some("pty-stream".into()),
            at: 0,
            session: None,
            state: Default::default(),
            checkouts: Vec::new(),
            tasks: Default::default(),
        };
        FeatureBoard {
            project: None,
            project_name: "argus".into(),
            features: vec![feature],
            current: Some("streaming-the-pty".into()),
            decisions: vec![Decision {
                id: 4,
                parent: None,
                at: 0,
                session: None,
                checkout: None,
                feature: Some("streaming-the-pty".into()),
                chose: "one reader thread".into(),
                over: Some("polling the handle".into()),
                because: Some("a poll cannot see a burst".into()),
                superseded_by: None,
            }],
            unfiled: 0,
        }
    }

    #[test]
    fn an_export_hands_over_the_whole_feature_and_asks_for_prose() {
        let message = format_export(&exportable_board());

        // The commission first: without it this is the board again, and
        // the point of the command is what the model does with it.
        assert!(message.starts_with("Write this feature up"), "{message}");
        assert!(message.contains("the road not taken"));
        // Then every part of the record a document would need.
        assert!(message.contains("Feature: streaming the pty (streaming-the-pty)"));
        assert!(message.contains("the reader thread owns the handle."));
        assert!(message.contains("#4 one reader thread"));
        assert!(message.contains("over: polling the handle"));
        assert!(message.contains("because: a poll cannot see a burst"));
    }

    #[test]
    fn a_checkout_on_no_feature_is_told_that_rather_than_asked_to_write() {
        let mut board = exportable_board();
        board.current = None;
        board.decisions.clear();

        let message = format_export(&board);

        assert!(
            message.starts_with("This checkout is not on a feature"),
            "{message}"
        );
        assert!(
            message.contains("streaming-the-pty"),
            "it still offers the ones that exist"
        );
    }
}
