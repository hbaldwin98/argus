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
//! argus-hook decide "which store?" --open --option sqlite --option "a file"  # asks, decides nothing
//! argus-hook decisions resolve 9 "sqlite" --because "both need migrations"   # the person's answer
//! argus-hook decisions show 9                        # one decision as a document
//! argus-hook decisions rewrite 9 --stdin             # that document, corrected
//! argus-hook task                                    # reads the task tree
//! argus-hook task add "bound the queue" --under 12  # adds a subtask
//! argus-hook task criterion add 12 "a full queue refuses"   # what done means
//! argus-hook task criterion check 12 1               # checks the first one off
//! argus-hook task note 12 "bounded; refusal next"    # where the work stands
//! argus-hook diagram                                 # sequence diagrams on this feature
//! argus-hook diagram add "open overlay" --stdin      # Mermaid source on stdin
//! argus-hook diagram rewrite 3 "open overlay" --stdin  # replaces #3's source
//! argus-hook diagram drop 3                          # removes diagram #3
//! argus-hook tracker-map                             # how this repo maps to its trackers
//! argus-hook tracker-map draft --stdin               # proposes one; the person approves it
//! argus-hook reconcile                               # the reconcile the person asked for
//! argus-hook reconcile propose --stdin               # its lines as JSON, all unticked
//! argus-hook reconcile report --failed 1 "403"       # the ticked lines are carried out
//! argus-hook view show --stdin                       # shows the person a view, as JSON on stdin
//! argus-hook view patch eng-1 'blocks[2]' --stdin       # replaces one block, as JSON on stdin
//! argus-hook view close eng-1                       # takes it down
//! argus-hook telemetry --model gpt-5 --context 42000 --window 272000 --cost 0.12 --tool shell
//! argus-hook transcript ~/.herdr/s1.jsonl user "tidy the notes"   # a neutral transcript line
//! argus-hook transcript ~/.herdr/s1.jsonl tool shell "npm run lint" --failed
//! argus-hook context                           # comments, and the feature by name
//! argus-hook context "preface"                  # the preface, then the same
//! argus-hook say "text"                          # prints, calls nobody
//! argus-hook instructions                        # inherited instructions, then context
//! argus-hook inbox <session-id>                  # the pane's inbox, one item a line
//! argus-hook agent "review the parser"           # another agent, in a pane of its own here
//! argus-hook agent --worktree fix-resize --stdin # in a new worktree, its brief on stdin
//! argus-hook agent --in ~/src/other "port this"  # in a checkout of another repository
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
//! conversation. The deliberate `say`, `instructions`, `context`, `comments`, `feature`,
//! `task`, `diagram`, `decisions`, and `decide` commands do return useful output.
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
//! The commands live in modules by what they talk about: `comments` for
//! the review comments addressed to the pane, `board` for the feature board
//! an agent reads and writes — features, tasks, diagrams, decisions, the
//! tracker map and a feature's reconcile with its tickets — `installed`
//! for the form a harness's hook config runs, `transport` for how either
//! reaches the daemon, `view` for what an agent shows the person, `launch`
//! for the agents an agent starts, and `transcript` for the one file
//! written without it.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use argus_protocol::hook;
use argus_protocol::{
    is_slug, parse_pane_url, requested_scope, Consequence, Decision, DecisionBoard,
    DecisionChange, DecisionState, DecisionText, DecisionWrite, DiagramAction,
    DiagramList, DiagramWrite, Endpoint, FeatureAction, FeatureBoard, FeatureWrite, LinkWrite,
    Report, TrackerMap,
    ReviewComment, TaskAction, TaskList, TaskPlace, TaskState, TaskWrite, ARTIFACT_SCOPE_VAR,
    CONTEXT_COMMAND, INSTRUCTIONS_COMMAND, INSTRUCTIONS_VAR, NOTE_FLAG, OWNS_SESSION_FLAG,
    BACKGROUND_HEADER, DIRECTORY_HEADER, SESSION_HEADER, SESSION_KEY_FLAG, TITLE_FLAG, TOKEN_VAR, URL_VAR,
};

const TIMEOUT: Duration = Duration::from_secs(2);

mod board;
mod comments;
mod context;
mod inbox;
mod installed;
mod launch;
mod telemetry;
mod transcript;
mod transport;
mod view;

use installed::*;
use telemetry::*;
use transport::*;
use view::view;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str);
    let rest = args
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let rest = match command {
        Some(board) if BOARD_COMMANDS.contains(&board) => match named_feature(board, &rest) {
            Ok((named, rest)) => {
                if let Some(slug) = named {
                    name_feature(slug);
                }
                rest
            }
            Err(message) => {
                println!("{message}");
                return;
            }
        },
        _ => rest,
    };
    dispatch(command, &rest);
}

/// The commands that read or write a feature board, which may name the
/// feature they are about rather than take the one the checkout is on.
const BOARD_COMMANDS: &[&str] = &["feature", "task", "diagram", "decisions", "decide", "reconcile"];

type NamedHandler = fn(&[&str]);

const NAMED_HANDLERS: &[(&str, NamedHandler)] = &[
    ("say", say),
    (INSTRUCTIONS_COMMAND, instructions),
    (CONTEXT_COMMAND, context::context),
    ("title", title),
    ("status", status),
    ("checkout", checkout),
    ("session", session),
    ("comments", comments::comments),
    ("feature", board::features::feature),
    ("task", board::tasks::task),
    ("diagram", board::diagrams::diagram),
    ("tracker-map", board::tracker_map::tracker_map),
    ("reconcile", board::reconcile::reconcile),
    ("view", view),
    ("decisions", board::decisions::decisions),
    ("decide", board::decisions::decide),
    ("telemetry", telemetry),
    ("transcript", transcript::transcript),
    ("inbox", inbox::inbox),
    ("agent", launch::agent),
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
    context::context(&[&env_instructions()]);
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
    // Said aloud either way: a move that silently failed left agents
    // believing their row had followed them.
    let Some(path) = reported_checkout(rest) else { return };
    match call::<hook::call::Checkout>(&env_url(), &env_token(), &path.to_string_lossy().into_owned()) {
        Ok(()) => println!("This pane now belongs to {}.", path.display()),
        Err(refusal) => println!("{}", refusal.said("Not moved")),
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
            false,
            None,
            "",
        );
        assert!(tagged.contains("\r\nX-Argus-Session: s-1\r\n"), "{tagged}");
        let untagged =
            request("/pane/1/status/idle", "127.0.0.1:4242", "tok", None, false, None, "");
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
        let req = request(
            "/pane/1/title",
            "127.0.0.1:4242",
            "tok",
            Some("s-1"),
            true,
            Some("C:\\work\\repo"),
            "hi",
        );
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
        // Claude Code's and Codex's tool events defer to the user's rules.
        assert_eq!(
            hook_reply(
                Some(r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","session_id":"s"}"#),
                false,
                ""
            ),
            "{}"
        );
    }

    #[test]
    fn telemetry_flags_become_a_partial_report() {
        let report = parse_flags(&[
            "--model",
            "gpt-5",
            "--context",
            "42_000",
            "--window",
            "272000",
            "--cost",
            "$0.12",
            "--tool",
            "shell",
        ]);
        assert_eq!(report.model.as_deref(), Some("gpt-5"));
        assert_eq!(report.context_tokens, Some(42_000));
        assert_eq!(report.context_window, Some(272_000));
        assert_eq!(report.cost_usd, Some(0.12));
        assert_eq!(report.tool.as_deref(), Some("shell"));
        assert_eq!(parse_flags(&["--tool-done"]).tool.as_deref(), Some(""));
    }

    #[test]
    fn a_tool_event_names_the_tool_and_its_end_clears_it() {
        let start = from_hook(
            r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","model":"claude-opus-5"}"#,
        );
        assert_eq!(start.tool.as_deref(), Some("Bash"));
        assert_eq!(start.model.as_deref(), Some("claude-opus-5"));
        let end = from_hook(r#"{"hook_event_name":"PostToolUse","tool_name":"Bash"}"#);
        assert_eq!(end.tool.as_deref(), Some(""));
        let cursor = from_hook(
            r#"{"hook_event_name":"beforeShellExecution","command":"ls","model":{"id":"gpt-5"}}"#,
        );
        assert_eq!(cursor.tool.as_deref(), Some("shell"));
        assert_eq!(cursor.model.as_deref(), Some("gpt-5"));
        assert!(from_hook(r#"{"session_id":"s"}"#).is_empty());
    }

    #[test]
    fn claude_transcripts_give_the_model_and_the_latest_context() {
        let lines = [
            r#"{"type":"user","message":{"content":"hi"}}"#,
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","usage":{"input_tokens":10,"output_tokens":5}}}"#,
            r#"{"type":"assistant","message":{"model":"claude-opus-5","usage":{"input_tokens":3,"cache_creation_input_tokens":1000,"cache_read_input_tokens":40000,"output_tokens":200}}}"#,
            "not json",
        ];
        let report = transcript_lines(lines.into_iter());
        assert_eq!(report.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(report.context_tokens, Some(41_203));
    }

    #[test]
    fn codex_transcripts_give_the_window_and_cumulative_tokens() {
        let lines = [
            r#"{"type":"turn_context","payload":{"model":"gpt-5-codex"}}"#,
            r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#,
            r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":90000,"output_tokens":4000},"last_token_usage":{"total_tokens":30500},"model_context_window":272000}}}"#,
        ];
        let report = transcript_lines(lines.into_iter());
        assert_eq!(report.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(report.context_tokens, Some(30_500));
        assert_eq!(report.context_window, Some(272_000));
        assert_eq!(report.input_tokens, Some(90_000));
        assert_eq!(report.output_tokens, Some(4_000));
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
    fn an_agent_inside_the_session_is_not_the_panes_own_voice() {
        let own = r#"{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"Bash"}"#;
        let subagent = r#"{"session_id":"s","agent_id":"a1","agent_type":"Explore","hook_event_name":"PreToolUse"}"#;
        let workflow = r#"{"session_id":"s","agent_id":"a2","agent_type":"workflow-subagent","hook_event_name":"SubagentStop"}"#;
        assert!(!from_an_inner_agent(own));
        assert!(from_an_inner_agent(subagent));
        assert!(from_an_inner_agent(workflow));
    }

    #[test]
    fn a_turn_ending_with_agents_in_flight_says_what_it_left_running() {
        // Recorded from Claude Code 2.1.287: Stop lists the background work
        // still in flight, and the wake-up turn's Stop lists none.
        let stop = r#"{"session_id":"s","hook_event_name":"Stop","background_tasks":[
            {"id":"b1","type":"shell","status":"running","description":"npm run dev"},
            {"id":"w1","type":"workflow","status":"completed","name":"old"},
            {"id":"w2","type":"workflow","status":"running","name":"probe"}]}"#;
        assert!(leaves_an_agent_running(stop));
        // A background shell can outlive every turn the session takes.
        let shell = r#"{"hook_event_name":"Stop","background_tasks":[
            {"id":"b1","type":"shell","status":"running"},
            {"id":"w1","type":"workflow","status":"completed"}]}"#;
        assert!(!leaves_an_agent_running(shell));
        assert!(!leaves_an_agent_running(r#"{"hook_event_name":"Stop"}"#));
        // An Agent call run in the background is listed as a subagent.
        let subagent = r#"{"hook_event_name":"Stop","background_tasks":[
            {"id":"a1","type":"subagent","status":"running","agent_type":"Explore"}]}"#;
        assert!(leaves_an_agent_running(subagent));

        let said =
            request("/pane/1/status/idle", "127.0.0.1:4242", "tok", Some("s"), true, None, "");
        assert!(said.contains(&format!("\r\n{BACKGROUND_HEADER}: agents\r\n")), "{said}");
        let unsaid =
            request("/pane/1/status/idle", "127.0.0.1:4242", "tok", Some("s"), false, None, "");
        assert!(!unsaid.contains(BACKGROUND_HEADER), "{unsaid}");
    }

    #[test]
    fn a_report_says_which_directory_the_conversation_is_in() {
        // Claude Code's Stop names the session's directory, which follows
        // it into a worktree; the daemon moves the row from that.
        let stop = r#"{"session_id":"s","hook_event_name":"Stop",
            "cwd":"C:\\work\\repo\\.argus\\worktrees\\feat"}"#;
        let directory = json_string(stop, "cwd").unwrap();
        assert_eq!(directory, "C:\\work\\repo\\.argus\\worktrees\\feat");
        let said = request(
            "/pane/1/status/idle",
            "127.0.0.1:4242",
            "tok",
            Some("s"),
            false,
            Some(&directory),
            "",
        );
        assert!(said.contains(&format!("\r\n{DIRECTORY_HEADER}: {directory}\r\n")), "{said}");
    }

    #[test]
    fn a_task_notification_is_not_a_title() {
        let wake = r#"{"session_id":"s","hook_event_name":"UserPromptSubmit",
            "prompt":"<task-notification>\n<task-id>wk1</task-id>\n<status>completed</status>"}"#;
        assert_eq!(title_from(wake), "");
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
}
