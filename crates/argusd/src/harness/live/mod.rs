//! What a harness's own interface to its running session says, in Argus's
//! terms: the live channel, for an agent template that asks for one with
//! `live = true`.
//!
//! Two kinds. Codex's TUI can run on its app-server, which Argus joins as a
//! second client, reading the thread and speaking back through it
//! (`codex`). Claude Code's TUI can send its API traffic through a proxy,
//! which Argus is, reading the reply as it streams (`claude`). These
//! modules only translate; running the server, the connection and the
//! proxy is `state::live`'s and `state::tee`'s.

use serde::Deserialize;

pub mod claude;
// Only a Unix socket reaches Codex's app-server, so nothing else reads it.
#[cfg(unix)]
pub mod codex;

/// Which live channel a harness has. Off unless an agent template asks for
/// it with `live = true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LiveChannel {
    CodexAppServer,
    /// The harness's API traffic goes through Argus's tee.
    AnthropicStream,
}
