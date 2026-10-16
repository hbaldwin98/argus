//! The comment box under picked lines as the page draws it, for the review
//! and the Code stage alike: what the lines are, the agents it could go to
//! and the one it goes to. Whether a comment may go, and to whom, is
//! `argus_protocol::agent_comment`'s.

use argus_protocol::agent_comment;
use argus_protocol::{CheckoutInfo, PaneId};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CommentPage {
    /// What the box is on, as its header says it: `new lines 39–40 · unstaged`,
    /// `lines 3–5 · src/a.rs`.
    pub about: String,
    /// The agents in the checkout it could go to.
    pub recipients: Vec<Recipient>,
    /// The one it goes to, while that one still runs; none lit means one is
    /// to be chosen.
    pub to: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Recipient {
    pub pane: u64,
    pub title: String,
}

/// The box on `about`, offering the live agents of `checkout` and lit on
/// `to` only while it is one of them.
pub fn page(checkout: Option<&CheckoutInfo>, to: Option<PaneId>, about: String) -> CommentPage {
    let offered: Vec<Recipient> = checkout
        .into_iter()
        .flat_map(agent_comment::recipients)
        .map(|p| Recipient { pane: p.id.0, title: p.title.clone() })
        .collect();
    let to = to.filter(|p| offered.iter().any(|r| r.pane == p.0)).map(|p| p.0);
    CommentPage { about, recipients: offered, to }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane};
    use argus_protocol::{PaneKind, PaneStatus};

    #[test]
    fn the_page_lights_only_an_agent_that_still_runs() {
        let co = checkout(100, "main", true, vec![
            pane(6, PaneKind::Agent, "gone", PaneStatus::Exited { code: Some(0) }),
            pane(7, PaneKind::Agent, "claude", PaneStatus::Working),
            pane(8, PaneKind::Shell, "zsh", PaneStatus::Idle),
        ]);
        let lit = page(Some(&co), Some(PaneId(7)), "line 1 · a.rs".into());
        assert_eq!(lit.recipients, [Recipient { pane: 7, title: "claude".into() }]);
        assert_eq!(lit.to, Some(7));
        assert_eq!(page(Some(&co), Some(PaneId(6)), String::new()).to, None, "an exited agent is not lit");
        assert_eq!(page(None, Some(PaneId(7)), String::new()).to, None, "nor one in a checkout that has gone");
    }
}
