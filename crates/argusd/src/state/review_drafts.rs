//! A review being written against a checkout, and sending it: drafts held
//! in the store until the person sends them all to one agent at once.

use argus_protocol::{CheckoutId, DraftAction, ReviewComment, ReviewDraft};

use super::agents::checked_comment;
use super::*;

/// What a draft action came to: the drafts as they now are, and for a
/// send, how many comments went and whether the agent was told at once.
#[derive(Debug)]
pub struct Drafted {
    pub drafts: Vec<ReviewDraft>,
    pub sent: Option<(u32, bool)>,
}

impl Daemon {
    pub fn review_draft(&self, checkout: CheckoutId, action: DraftAction) -> anyhow::Result<Drafted> {
        let path = self.checkout_path(checkout)?;
        let gone = || anyhow::anyhow!("that comment is no longer in the review");
        let mut sent = None;
        match action {
            DraftAction::List => {}
            DraftAction::Add { anchor, body } => {
                self.store.add_review_draft(&path, &anchor, &checked_comment(&body)?)?;
            }
            DraftAction::Edit { id, body } => {
                if !self.store.edit_review_draft(&path, id, &checked_comment(&body)?)? {
                    return Err(gone());
                }
            }
            DraftAction::Delete { id } => {
                if !self.store.delete_review_draft(&path, id)? {
                    return Err(gone());
                }
            }
            DraftAction::Discard => self.store.discard_review_drafts(&path)?,
            DraftAction::Submit { recipient, summary } => {
                // Asked first, so a review for an agent that has gone stays
                // a draft rather than being sent to nobody.
                self.review_recipient(checkout, recipient)?;
                if self.store.review_drafts(&path)?.is_empty() {
                    anyhow::bail!("the review has no comments to send");
                }
                let comments = self.store.send_review_drafts(&path)?;
                let delivered = self.notify(recipient, &notification(&summary, &comments));
                sent = Some((comments.len() as u32, delivered));
            }
        }
        Ok(Drafted { drafts: self.store.review_drafts(&path)?, sent })
    }
}

/// A whole review in one line, as one comment's notification is: a newline
/// would submit the prompt half-written.
fn notification(summary: &str, comments: &[ReviewComment]) -> String {
    let summary = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut line = match comments.len() {
        1 => "Review, 1 comment".to_string(),
        n => format!("Review, {n} comments"),
    };
    if !summary.is_empty() {
        line.push_str(": ");
        line.push_str(&summary);
    }
    for (n, comment) in comments.iter().enumerate() {
        line.push_str(&format!(" | {}. {}", n + 1, comment.anchor.notification(&comment.body)));
    }
    line
}

#[cfg(test)]
mod tests {
    use argus_protocol::{ReviewAnchor, ReviewBase};

    use super::*;

    fn comment(path: &str, line: u32, body: &str) -> ReviewComment {
        ReviewComment {
            id: 1,
            anchor: ReviewAnchor {
                base: ReviewBase::Unstaged,
                commit: None,
                path: path.into(),
                old_path: None,
                old_start: None,
                old_end: None,
                new_start: Some(line),
                new_end: Some(line),
                text: Vec::new(),
            },
            body: body.into(),
        }
    }

    #[test]
    fn a_review_is_told_in_one_line_with_its_summary_first() {
        let told = notification(" Two things\nbefore merge ", &[comment("a.rs", 3, "why?"), comment("b.rs", 9, "rename\nthis")]);
        assert_eq!(told, "Review, 2 comments: Two things before merge | 1. a.rs:3: why? | 2. b.rs:9: rename this");
        assert!(!told.contains('\n'));
        assert_eq!(notification("", &[comment("a.rs", 3, "x")]), "Review, 1 comment | 1. a.rs:3: x");
    }
}
