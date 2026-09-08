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
        (INSTRUCTIONS_VAR.into(), skill::fallback().to_string()),
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
