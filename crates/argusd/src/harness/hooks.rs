//! The environment and command lines a managed hook is built from: the
//! URL that names a pane, the token that authorizes it, and the helper
//! invocation a harness ends up running.

use super::*;

use argus_protocol::{
    HELPER_VAR, INSTRUCTIONS_VAR, NOTE_FLAG, OWNS_SESSION_FLAG, PANE_VAR, SESSION_KEY_FLAG,
    TITLE_FLAG, TOKEN_VAR, URL_VAR,
};

/// Environment handed to every agent pane, whatever its harness.
///
/// The universal floor: a harness with no settings file Argus understands
/// can still report, and an agent that has been told these exist can rename
/// its own pane from inside a turn.
pub fn env(pane: PaneId, port: u16, token: &str) -> Vec<(String, String)> {
    vec![
        (URL_VAR.into(), pane_url(pane, port)),
        (TOKEN_VAR.into(), token.to_string()),
        (PANE_VAR.into(), pane.0.to_string()),
        (HELPER_VAR.into(), helper_path()),
        (INSTRUCTIONS_VAR.into(), instructions()),
    ]
}

/// The base every endpoint for this pane hangs off.
pub(super) fn pane_url(pane: PaneId, port: u16) -> String {
    format!("http://127.0.0.1:{port}/pane/{}", pane.0)
}

pub(super) fn event_target_url(pane: PaneId, port: u16, event: &Event) -> String {
    let base = pane_url(pane, port);
    if event.claim_only {
        format!("{base}/session")
    } else {
        format!("{base}/status/{}", event.reports.as_str())
    }
}

pub(super) fn event_env_url(event: &Event, windows: bool) -> String {
    let base = if windows {
        "%ARGUS_HOOK_URL%"
    } else {
        "$ARGUS_HOOK_URL"
    };
    if event.claim_only {
        format!("{base}/session")
    } else {
        format!("{base}/status/{}", event.reports.as_str())
    }
}

/// What an agent is told about Argus, once, at the start of its session.
///
/// Kept to what is actionable. An agent that reads this should come away
/// knowing it can name its own row, which is the whole point: a column of
/// four panes all called "claude" tells you nothing about which one is
/// worth looking at.
pub fn instructions() -> String {
    let hook = helper_path();
    format!(
        "You are running inside Argus, which shows this session as one pane in a list.\n\
         The pane is currently named after the agent, which is not useful when several \
         are running. Rename it to whatever you are actually working on, as a short \
         noun phrase of a few words, by running:\n\
         \n\
         \x20 {hook} title \"fixing the pty deadlock\"\n\
         \n\
         Do that as soon as you know what the task is, and again whenever you move on \
         to something clearly different.\n\
         \n\
         Other agents may be running in the same checkout. Never run `git switch` or \
         `git checkout` in the checkout you were started in, because that changes the \
         branch for every agent sharing it. If you need another branch, create a new \
         linked worktree and branch there with `git worktree add <new-path> -b \
         <new-branch>`. Do all subsequent work from that new path.\n\
         \n\
         Review comments are durable and scoped to this checkout. Read the newest comments with:\n\
         \n\
         \x20 {hook} comments\n\
         \n\
         The human keeps notes on this checkout and the project above it, and lines \
         marked `- [!]` in them are standing instructions meant for you. Read them \
         before you start, and again whenever the task changes shape:\n\
         \n\
         \x20 {hook} context\n\
         \n\
         After you start working in another checkout, run this from that checkout so the \
         pane moves under it in Argus:\n\
         \n\
         \x20 {hook} checkout\n\
         \n\
         If you get blocked and need the human, say so in one line so they can see \
         why from the pane list without opening it:\n\
         \n\
         \x20 {hook} status waiting \"needs the staging database password\"\n\
         \x20 {hook} status failed \"cargo test is failing on a dependency I can't fix\"\n\
         \n\
         When your changes are ready for the human to inspect, report `needs-review`. \
         After they are reviewed and the task is complete, report `done`:\n\
         \n\
         \x20 {hook} status needs-review \"ready for review\"\n\
         \x20 {hook} status done \"reviewed and complete\"\n\
         \n\
         Report `working` again when you resume work. These write nothing and cost \
         nothing. Do not mention having run them."
    )
}

/// The helper that actually posts to the daemon (`src/bin/argus-hook.rs`),
/// resolved next to the running daemon rather than trusted to `PATH` —
/// nothing installs these binaries system-wide. Falls back to the bare name
/// if the daemon's own path can't be read.
pub fn helper_path() -> String {
    let exe = if cfg!(windows) {
        "argus-hook.exe"
    } else {
        "argus-hook"
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe)))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| exe.to_string())
}

/// Installed hook form for harnesses whose config is a single shell string
/// but whose runner does not inherit the pane environment (Cursor). Same
/// argv as Claude's command-plus-args shape, joined for the shell.
pub(super) fn baked_command_line(helper: &str, pane: PaneId, port: u16, token: &str, event: &Event) -> String {
    let mut parts = vec![
        helper.to_string(),
        event_target_url(pane, port, event),
        token.to_string(),
    ];
    push_event_flags(&mut parts, event);
    quote_command_parts(parts)
}

/// A stable command-string hook. Codex persists trust against the handler's
/// content hash, so the checkout-wide file must not contain ephemeral pane,
/// port, token, or executable-path values. Every spawned pane receives these
/// variables, and the helper's installed form still extracts hook stdin.
pub(super) fn env_command_line(event: &Event, windows: bool) -> String {
    let (helper, url, token) = if windows {
        (
            "%ARGUS_HOOK%".to_string(),
            event_env_url(event, true),
            "%ARGUS_HOOK_TOKEN%".to_string(),
        )
    } else {
        (
            "$ARGUS_HOOK".to_string(),
            event_env_url(event, false),
            "$ARGUS_HOOK_TOKEN".to_string(),
        )
    };
    let mut parts = vec![helper, url, token];
    push_event_flags(&mut parts, event);
    quote_command_parts(parts)
}

pub(super) fn quote_command_parts(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .map(|part| format!("\"{}\"", part.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn push_event_flags(parts: &mut Vec<String>, event: &Event) {
    if event.note_from_stdin {
        parts.push(NOTE_FLAG.to_string());
    }
    if event.title_from_stdin {
        parts.push(TITLE_FLAG.to_string());
    }
    if let Some(key) = &event.session_id_key {
        parts.push(SESSION_KEY_FLAG.to_string());
        parts.push(key.clone());
    }
    if event.owns_session {
        parts.push(OWNS_SESSION_FLAG.to_string());
    }
}
