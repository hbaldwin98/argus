//! The words an open feature's header says about its tickets and its reconcile, said once for every
//! window that draws them: the Tauri page keeps its own copy under node (`head_words.ts`), the GPUI
//! window reads these off `FeatureView`.
//!
//! They are built from what the board holds (the links, when the reconcile happened, who ran it, how many rows
//! changed since), never from the sentence `FeatureView::reconciled` says, which a reader would have to
//! pick apart.

use crate::connection::page::Link;

/// The Reconcile entry of the feature's menu: what it says, what sits under it, and whether it asks for the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileEntry {
    pub label: String,
    /// When it was last reconciled and how much changed since, or that it never was.
    pub hint: String,
    /// Only a proposal waiting on the person asks for them; the other stages say where the reconcile stands.
    pub attention: bool,
}

/// The one button the linked tickets are: a single ticket by name, several of one tracker by count under
/// its name, several trackers by count alone, so the header's line stays one length however many are
/// linked. None with no links.
pub fn tickets_label(links: &[Link]) -> Option<String> {
    let first = links.first()?;
    if links.len() == 1 {
        return Some(format!("{} {}", first.tracker, first.key));
    }
    let count = format!("{} tickets", links.len());
    Some(if links.iter().all(|link| link.tracker == first.tracker) { format!("{} \u{b7} {count}", first.tracker) } else { count })
}

/// What the reconcile has to say: `when` is how long ago it ran (`3d`), as the header has always said it.
pub fn reconcile_entry(stage: Option<&str>, when: Option<&str>, changes: Option<usize>) -> ReconcileEntry {
    let waiting = match stage {
        Some("asked") => Some("an agent is reading"),
        Some("proposed") => Some("a proposal waits"),
        Some("carrying") => Some("being carried out"),
        _ => None,
    };
    let changes = changes.unwrap_or(0);
    let said: Vec<String> = [
        when.map(|when| format!("Reconciled {when} ago")),
        (changes > 0).then(|| format!("{changes} change{} since", if changes == 1 { "" } else { "s" })),
    ]
    .into_iter()
    .flatten()
    .collect();
    ReconcileEntry {
        label: waiting.map_or_else(|| "Reconcile with its tickets\u{2026}".to_string(), |waiting| format!("Reconcile \u{2014} {waiting}")),
        hint: if said.is_empty() { "never reconciled".to_string() } else { said.join(" \u{b7} ") },
        attention: stage == Some("proposed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(tracker: &str, key: &str) -> Link {
        Link { tracker: tracker.into(), key: key.into(), url: format!("https://x/{key}"), role: String::new() }
    }

    // The page's own rule, `head.test.mjs`: the linked tickets are one button whatever their number.
    #[test]
    fn the_linked_tickets_are_one_button_whatever_their_number() {
        assert_eq!(tickets_label(&[]), None);
        assert_eq!(tickets_label(&[link("GitHub", "orion#212")]).as_deref(), Some("GitHub orion#212"));
        assert_eq!(tickets_label(&[link("GitHub", "orion#212"), link("GitHub", "orion#9")]).as_deref(), Some("GitHub \u{b7} 2 tickets"));
        assert_eq!(tickets_label(&[link("GitHub", "orion#212"), link("Jira", "ARG-1")]).as_deref(), Some("2 tickets"));
    }

    // The page's own rule, `head.test.mjs`: only a waiting proposal asks for attention at Reconcile.
    #[test]
    fn only_a_waiting_proposal_asks_for_attention_at_reconcile() {
        let waits = reconcile_entry(Some("proposed"), Some("3d"), Some(11));
        assert_eq!(waits.label, "Reconcile \u{2014} a proposal waits");
        assert!(waits.attention);
        assert_eq!(waits.hint, "Reconciled 3d ago \u{b7} 11 changes since");
        assert!(!reconcile_entry(Some("asked"), Some("3d"), Some(11)).attention);
        assert!(!reconcile_entry(Some("carrying"), Some("3d"), Some(11)).attention);
        let idle = reconcile_entry(None, None, None);
        assert_eq!(idle.label, "Reconcile with its tickets\u{2026}");
        assert_eq!(idle.hint, "never reconciled");
    }

    #[test]
    fn one_change_is_said_in_the_singular_and_none_is_not_said() {
        assert_eq!(reconcile_entry(None, Some("2h"), Some(1)).hint, "Reconciled 2h ago \u{b7} 1 change since");
        assert_eq!(reconcile_entry(None, Some("2h"), Some(0)).hint, "Reconciled 2h ago");
    }
}
