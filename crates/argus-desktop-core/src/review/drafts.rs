//! The review being written, as the window holds it: the drafts the daemon
//! keeps for the checkout as it last said them, the changes to them still
//! out, the send, and what the page draws of them — each draft placed
//! against the row of the diff on screen it was written on.
//!
//! The list is only ever the daemon's: a draft added, edited or deleted
//! shows once the daemon answers with the drafts as they then are.

use argus_protocol::{CheckoutId, CheckoutInfo, ClientMsg, DraftAction, PaneId, ReviewAnchor, ReviewDraft, MAX_REVIEW_COMMENT_BYTES};
use serde::{Deserialize, Serialize};

use argus_protocol::review::agent_comment;

use crate::connection::page::DiffRow;
use crate::review::comment_box::Recipient;

/// A draft or the review, as the window changes it.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum DraftGesture {
    Edit { id: u64, body: String },
    Delete { id: u64 },
    /// Every draft, unsent.
    Discard,
    /// Every draft, to `to` — or, when none is named, to the agent the
    /// comment box is lit on, else the checkout's first.
    Submit { summary: String, to: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DraftsPage {
    pub drafts: Vec<DraftView>,
    /// The agents it could be sent to, and the one it goes to unless
    /// another is picked.
    pub recipients: Vec<Recipient>,
    pub to: Option<u64>,
    pub sending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DraftView {
    pub id: u64,
    pub path: String,
    /// Where in the file, as `a.rs:12` or `a.rs:3-5`.
    pub at: String,
    pub body: String,
    /// The row of the diff on screen it hangs under — its last line's —
    /// when it was written on this file and side.
    pub row: Option<usize>,
}

#[derive(Debug, Default)]
pub struct Drafts {
    held: Vec<ReviewDraft>,
    /// Changes to the drafts out, by request id, the send apart.
    out: Vec<u64>,
    sending: Option<u64>,
}

impl Drafts {
    /// Asks for the checkout's drafts, as a review opens.
    pub fn list(&mut self, checkout: CheckoutId, request_id: u64) -> ClientMsg {
        self.out.push(request_id);
        ClientMsg::Draft { request_id, checkout, action: DraftAction::List }
    }

    /// The comment box's words, as a draft on `anchor`. The box keeps them
    /// until the daemon's answer names this request.
    pub fn add(anchor: Option<ReviewAnchor>, checkout: CheckoutId, body: &str, request_id: u64) -> Result<ClientMsg, &'static str> {
        let anchor = anchor.ok_or("pick a line to comment on")?;
        let body = checked(body)?;
        Ok(ClientMsg::Draft { request_id, checkout, action: DraftAction::Add { anchor: Box::new(anchor), body } })
    }

    /// `gesture` as the daemon is asked it. `lit` is the agent the comment
    /// box is on, which a send names when the gesture names none.
    pub fn gesture(
        &mut self,
        gesture: DraftGesture,
        checkout: Option<&CheckoutInfo>,
        lit: Option<PaneId>,
        request_id: u64,
    ) -> Result<ClientMsg, String> {
        let checkout_info = checkout.ok_or("the checkout is gone")?;
        let action = match gesture {
            DraftGesture::Edit { id, body } => DraftAction::Edit { id, body: checked(&body)? },
            DraftGesture::Delete { id } => DraftAction::Delete { id },
            DraftGesture::Discard => DraftAction::Discard,
            DraftGesture::Submit { summary, to } => {
                if self.sending.is_some() {
                    return Err("the review is still being sent".into());
                }
                if self.held.is_empty() {
                    return Err("the review has no comments to send".into());
                }
                let recipient = to.map(PaneId).or(lit).filter(|p| live(checkout_info).contains(p)).or_else(|| live(checkout_info).first().copied());
                let recipient = recipient.ok_or("no agent running in this checkout")?;
                self.sending = Some(request_id);
                DraftAction::Submit { recipient, summary }
            }
        };
        if self.sending != Some(request_id) {
            self.out.push(request_id);
        }
        Ok(ClientMsg::Draft { request_id, checkout: checkout_info.id, action })
    }

    /// The daemon's drafts for `checkout`, taken whole: the answer to any
    /// request names the list as it now stands.
    pub fn receive(&mut self, request_id: u64, drafts: Vec<ReviewDraft>) {
        self.out.retain(|&id| id != request_id);
        if self.sending == Some(request_id) {
            self.sending = None;
        }
        self.held = drafts;
    }

    /// Whether the refusal was of a request from here; the caller says why.
    pub fn refused(&mut self, request_id: u64) -> bool {
        if self.sending == Some(request_id) {
            self.sending = None;
            return true;
        }
        let before = self.out.len();
        self.out.retain(|&id| id != request_id);
        self.out.len() != before
    }

    pub fn forget_sent(&mut self) {
        self.out.clear();
        self.sending = None;
    }

    /// What the page draws, given the rows on screen and their file and
    /// side, so each draft written there is placed against its row.
    pub fn page(&self, checkout: Option<&CheckoutInfo>, lit: Option<PaneId>, on: Option<(&str, argus_protocol::ReviewBase)>, rows: &[DiffRow]) -> DraftsPage {
        let recipients: Vec<Recipient> = checkout
            .into_iter()
            .flat_map(agent_comment::recipients)
            .map(|p| Recipient { pane: p.id.0, title: p.title.clone() })
            .collect();
        let to = lit.map(|p| p.0).filter(|p| recipients.iter().any(|r| r.pane == *p)).or(recipients.first().map(|r| r.pane));
        let drafts = self
            .held
            .iter()
            .map(|draft| {
                let here = on.is_some_and(|(path, base)| draft.anchor.path == path && draft.anchor.base == base);
                DraftView {
                    id: draft.id,
                    path: draft.anchor.path.clone(),
                    at: at(&draft.anchor),
                    body: draft.body.clone(),
                    row: here.then(|| row_of(&draft.anchor, rows)).flatten(),
                }
            })
            .collect();
        DraftsPage { drafts, recipients, to, sending: self.sending.is_some() }
    }
}

fn live(checkout: &CheckoutInfo) -> Vec<PaneId> {
    agent_comment::recipients(checkout).map(|p| p.id).collect()
}

/// Trimmed and in bounds, as the daemon keeps a comment.
fn checked(body: &str) -> Result<String, &'static str> {
    let body = body.trim();
    if body.is_empty() {
        return Err("the comment is empty");
    }
    if body.len() > MAX_REVIEW_COMMENT_BYTES {
        return Err("the comment is longer than 4096 bytes");
    }
    Ok(body.to_owned())
}

/// `a.rs:12`, `a.rs:3-5`, by the new side's numbers when it has them.
fn at(anchor: &ReviewAnchor) -> String {
    let (start, end) = match (anchor.new_start, anchor.new_end) {
        (None, None) => (anchor.old_start, anchor.old_end),
        range => range,
    };
    match (start, end) {
        (Some(a), Some(b)) if a != b => format!("{}:{a}-{b}", anchor.path),
        (Some(a), _) => format!("{}:{a}", anchor.path),
        _ => anchor.path.clone(),
    }
}

/// The row whose line ends the anchor: its last new line, or for a removal
/// alone its last old one.
fn row_of(anchor: &ReviewAnchor, rows: &[DiffRow]) -> Option<usize> {
    let sides = |r: &DiffRow| r.left.clone().into_iter().chain(r.right.clone());
    let hit = |r: &DiffRow| match (anchor.new_end, anchor.old_end) {
        (Some(new), _) => sides(r).any(|s| s.new == Some(new)),
        (None, Some(old)) => sides(r).any(|s| s.old == Some(old) && s.new.is_none()),
        _ => false,
    };
    rows.iter().rev().find(|r| hit(r)).map(|r| r.row)
}

#[cfg(test)]
mod tests;
