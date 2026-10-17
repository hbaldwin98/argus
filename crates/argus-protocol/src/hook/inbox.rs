//! What the daemon tells a harness's plugin over its inbox: the stream a
//! plugin that runs inside an agent keeps open to the pane API, so what a
//! person says reaches the agent through the harness's own interface rather
//! than as keys typed into its terminal.
//!
//! One JSON value per server-sent event (`data: …`), from
//! `GET /pane/<id>/inbox`. A plugin that does not open it is simply typed
//! into, the way every other harness is.

use serde::{Deserialize, Serialize};

/// One thing for the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InboxItem {
    /// Something for the agent to read once its turn ends, however the
    /// harness queues.
    Message { text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_reads_as_the_json_a_plugin_parses() {
        let item = InboxItem::Message { text: "look at line 4".into() };
        let json = serde_json::to_string(&item).unwrap();
        assert_eq!(json, r#"{"Message":{"text":"look at line 4"}}"#);
    }
}
