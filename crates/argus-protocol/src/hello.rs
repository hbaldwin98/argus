//! What each side of a connection says about itself before anything else,
//! so that neither sends the other a message it cannot read.
//!
//! A peer from before the handshake hangs up on any message it does not
//! know, so the exchange is shaped around never sending it one. The client
//! greets first; a daemon that greets back can take what it listed. A
//! daemon that sends its tree instead predates the handshake, and a client
//! that sends nothing is taken to predate it too. See DESIGN.md, "Process
//! model".

use serde::{Deserialize, Serialize};

/// The protocol generation this build speaks.
///
/// Raised only for a change the capabilities cannot carry — one that no
/// peer on the previous number could be spoken to across. Additions are
/// capabilities, not a new number.
pub const PROTOCOL: u32 = 1;

/// The optional messages and encodings this build can take. A side sends
/// one only when the other listed it, so each addition is safe against a
/// peer of any age.
pub const CAPABILITIES: &[&str] = &[PANE_TELEMETRY, CELL_RUNS];

/// `ServerMsg::PaneTelemetry` in place of a whole tree when an agent
/// reports its model, context or spend.
pub const PANE_TELEMETRY: &str = "pane-telemetry";

/// `ServerMsg::PaneRows` and `ServerMsg::RowDamage` in place of their
/// per-cell forms: a pane's screen as runs of cells, and scrolls as moves.
pub const CELL_RUNS: &str = "cell-runs";

/// One side's greeting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    /// The build's own version, for saying which side is behind.
    pub version: String,
    /// What this side can take beyond the protocol's floor. Strings rather
    /// than an enum, so a name this build has never heard of is ignored
    /// rather than failing the whole greeting.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl Hello {
    /// This build's greeting.
    pub fn this_build() -> Hello {
        Hello {
            protocol: PROTOCOL,
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: CAPABILITIES.iter().map(|c| c.to_string()).collect(),
        }
    }

    /// Whether the side that sent this greeting can take `capability`.
    pub fn can(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_greeting_names_this_build() {
        let hello = Hello::this_build();
        assert_eq!(hello.protocol, PROTOCOL);
        assert_eq!(hello.version, env!("CARGO_PKG_VERSION"));
        for capability in CAPABILITIES {
            assert!(hello.can(capability));
        }
    }

    #[test]
    fn a_capability_this_build_has_never_heard_of_is_just_not_there() {
        let hello = Hello {
            protocol: PROTOCOL + 1,
            version: "99.0.0".into(),
            capabilities: vec!["teleport".into()],
        };
        let bytes = rmp_serde::to_vec_named(&hello).unwrap();
        let read: Hello = rmp_serde::from_slice(&bytes).unwrap();
        assert!(read.can("teleport"));
        assert!(!read.can("pane-telemetry"));
    }
}
