//! The environment and command lines a managed hook is built from: the
//! URL that names a pane, the token that authorizes it, and the helper
//! invocation a harness ends up running.

use super::*;

use argus_protocol::{
    endpoint_url, pane_url, ArtifactScope, Endpoint, HELPER_VAR, INSTRUCTIONS_COMMAND,
    INSTRUCTIONS_VAR, NOTE_FLAG, OWNS_SESSION_FLAG, PANE_VAR, SESSION_KEY_FLAG, TITLE_FLAG,
    TOKEN_VAR, URL_VAR,
};

/// Environment handed to every agent pane, whatever its harness.
///
/// The universal floor: a harness with no settings file Argus understands
/// can still report, and an agent that has been told these exist can rename
/// its own pane from inside a turn.
pub fn env(pane: PaneId, port: u16, token: &str) -> Vec<(String, String)> {
    vec![
        (URL_VAR.into(), pane_url(port, pane)),
        (TOKEN_VAR.into(), token.to_string()),
        (PANE_VAR.into(), pane.0.to_string()),
        (HELPER_VAR.into(), helper_path()),
        (INSTRUCTIONS_VAR.into(), skill::fallback().to_string()),
    ]
}

/// What an event posts to: the pane's resume identity when it only claims
/// one, the status it reports otherwise.
fn event_endpoint(event: &Event) -> Endpoint {
    if event.claim_only {
        Endpoint::Session
    } else {
        Endpoint::Status(event.reports)
    }
}

pub(super) fn event_target_url(pane: PaneId, port: u16, event: &Event) -> String {
    endpoint_url(
        &pane_url(port, pane),
        event_endpoint(event),
        ArtifactScope::RepositoryBranch,
    )
}

pub(super) fn event_env_url(event: &Event) -> String {
    endpoint_url(
        &format!("${URL_VAR}"),
        event_endpoint(event),
        ArtifactScope::RepositoryBranch,
    )
}

/// Codex runs `commandWindows` through PowerShell, not cmd.exe. `%VAR%` and
/// a quoted executable path are treated like dot-sourcing; `& $env:VAR` runs
/// the helper.
pub(super) fn powershell_context_command() -> String {
    format!("& $env:ARGUS_HOOK {INSTRUCTIONS_COMMAND}")
}

/// The helper that actually posts to the daemon (`src/bin/argus-hook/`),
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
pub(super) fn baked_command_line(
    helper: &str,
    pane: PaneId,
    port: u16,
    token: &str,
    event: &Event,
) -> String {
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
    if windows {
        return powershell_env_command_line(event);
    }
    let mut parts = vec![
        "$ARGUS_HOOK".to_string(),
        event_env_url(event),
        "$ARGUS_HOOK_TOKEN".to_string(),
    ];
    push_event_flags(&mut parts, event);
    quote_command_parts(parts)
}

/// Target URL as a PowerShell expression, not a quoted `"$env:…/path"` string.
///
/// Codex may run `commandWindows` in Constrained Language mode. A token like
/// `$env:ARGUS_HOOK_URL/status/idle` is then parsed as division, which tries
/// to invoke methods on non-core types and fails with a language-mode error.
fn powershell_env_url_expr(event: &Event) -> String {
    format!("($env:{URL_VAR} + '/{}')", event_endpoint(event).suffix())
}

fn powershell_env_command_line(event: &Event) -> String {
    let mut segments = vec![
        "& $env:ARGUS_HOOK".to_string(),
        powershell_env_url_expr(event),
        "$env:ARGUS_HOOK_TOKEN".to_string(),
    ];
    if event.note_from_stdin {
        segments.push(format!("\"{NOTE_FLAG}\""));
    }
    if event.title_from_stdin {
        segments.push(format!("\"{TITLE_FLAG}\""));
    }
    if let Some(key) = &event.session_id_key {
        segments.push(format!("\"{SESSION_KEY_FLAG}\""));
        segments.push(format!("\"{}\"", key.replace('"', "`\"")));
    }
    if event.owns_session {
        segments.push(format!("\"{OWNS_SESSION_FLAG}\""));
    }
    segments.join(" ")
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
