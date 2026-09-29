//! What the daemon tells a harness's plugin over its inbox: the stream a
//! plugin that runs inside an agent keeps open to the pane API, so a reply,
//! an interrupt or an answer reaches the agent through the harness's own
//! interface rather than as keys typed into its terminal.
//!
//! One JSON value per server-sent event (`data: …`), from
//! `GET /pane/<id>/inbox`. A plugin that does not open it is simply typed
//! into, the way every other harness is.

use serde::{Deserialize, Serialize};

/// One thing for the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InboxItem {
    /// Something a person said. `steer` asks for it mid-turn; otherwise it
    /// waits for the agent's turn to end, however the harness queues.
    Message { text: String, steer: bool },
    /// Stop what the agent is doing.
    Interrupt,
    /// The answer to a question the plugin posed as a transcript entry:
    /// the entry's id, and the id of the choice.
    Answer { question: String, choice: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_reads_as_the_json_a_plugin_parses() {
        let item = InboxItem::Answer {
            question: "perm_1".into(),
            choice: "once".into(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert_eq!(json, r#"{"Answer":{"question":"perm_1","choice":"once"}}"#);
        assert_eq!(serde_json::to_string(&InboxItem::Interrupt).unwrap(), r#""Interrupt""#);
    }
}
