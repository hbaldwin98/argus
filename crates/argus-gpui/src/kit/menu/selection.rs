//! The menu a right-click on selected text opens: one item, `Copy`, under the heading `Selection`
//! (SHELL-MENU-5).
//!
//! It asks Rust to write the clipboard (`Call::CopyText`), as every other copy does, rather than
//! the view reaching for the platform: the page's menu writes it from the webview, against its own
//! rule that the page is never granted the clipboard.

use argus_desktop_core::connection::page::Call;

use super::item::MenuItem;

/// The menu for `text`, or none when it is only blanks, as a selection of nothing offers nothing.
pub fn items(text: &str) -> Option<Vec<MenuItem<Call>>> {
    if text.trim().is_empty() {
        return None;
    }
    Some(vec![MenuItem::heading("Selection"), MenuItem::choice("Copy", Call::CopyText { text: text.to_owned() })])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_text_offers_a_copy_of_all_of_it() {
        let items = items("  two words ").expect("a menu");
        assert!(matches!(&items[0], MenuItem::Heading(text) if text == "Selection"));
        let copy = items[1].as_choice().expect("a choice");
        assert_eq!(copy.label, "Copy");
        assert!(matches!(&copy.does, Call::CopyText { text } if text == "  two words "), "the selection as it was, not trimmed");
    }

    #[test]
    fn a_selection_of_blanks_offers_nothing() {
        assert!(items("").is_none());
        assert!(items(" \n\t").is_none());
    }
}
