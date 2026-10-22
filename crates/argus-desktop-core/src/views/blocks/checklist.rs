//! A checklist's tally, the words over its items.

use argus_protocol::CheckItem;

/// `2 of 5 done`.
pub fn tally(items: &[CheckItem]) -> String {
    format!("{} of {} done", items.iter().filter(|item| item.done).count(), items.len())
}

/// What a reader that cannot see the box is told before an item's label.
pub fn state_words(done: bool) -> &'static str {
    if done {
        "Done: "
    } else {
        "Not done: "
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(done: bool) -> CheckItem {
        CheckItem { label: "x".into(), done, note: None, opens: None }
    }

    #[test]
    fn the_tally_counts_what_is_done_of_all() {
        assert_eq!(tally(&[item(true), item(false), item(true), item(false), item(false)]), "2 of 5 done");
        assert_eq!(tally(&[]), "0 of 0 done");
    }

    #[test]
    fn an_item_is_read_as_done_or_not_done() {
        assert_eq!(state_words(true), "Done: ");
        assert_eq!(state_words(false), "Not done: ");
    }
}
