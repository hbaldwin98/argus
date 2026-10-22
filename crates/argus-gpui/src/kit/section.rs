//! Rows listed under headings: a heading with a count, and the rows that belong to it.
//!
//! Reconcile's lines are grouped under `Into Argus` and `Out to <Tracker>` in the order a heading
//! was first proposed; a list of files or of sessions groups the same way. What is in which group
//! is decided once, here, and a surface draws the heading and its rows.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, ElementId, SharedString};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The rows with the same heading, in the order they came, under the heading in the order it was
/// first met.
pub fn group_by<T>(rows: impl IntoIterator<Item = T>, heading: impl Fn(&T) -> String) -> Vec<(String, Vec<T>)> {
    let mut groups: Vec<(String, Vec<T>)> = Vec::new();
    for row in rows {
        let under = heading(&row);
        match groups.iter_mut().find(|(known, _)| *known == under) {
            Some((_, rows)) => rows.push(row),
            None => groups.push((under, vec![row])),
        }
    }
    groups
}

/// A heading above its rows: the words in the quiet label type, and how many rows are under it
/// at the right.
pub fn heading(id: impl Into<ElementId>, words: impl Into<SharedString>, count: usize, tokens: &Tokens) -> impl IntoElement {
    let words = words.into();
    h_flex()
        .id(id)
        .test_support()
        .aria_label(words.clone())
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_6))
        .gap(rem(tokens.sp_8))
        .items_center()
        .justify_between()
        .typed(tokens.type_label)
        .text_color(tokens.fg_muted)
        .child(div().min_w_0().truncate().child(words))
        .child(div().flex_shrink_0().text_color(tokens.fg_faint).child(SharedString::from(count.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_group_under_their_heading_in_the_order_the_heading_was_first_met() {
        let lines = [("Out to Linear", 1), ("Into Argus", 2), ("Out to Linear", 3), ("Into Argus", 4), ("Into Argus", 5)];
        let groups = group_by(lines, |(heading, _)| (*heading).to_owned());
        let shown: Vec<(&str, Vec<i32>)> = groups.iter().map(|(heading, rows)| (heading.as_str(), rows.iter().map(|(_, n)| *n).collect())).collect();
        assert_eq!(shown, [("Out to Linear", vec![1, 3]), ("Into Argus", vec![2, 4, 5])]);
    }

    #[test]
    fn nothing_is_no_groups_and_one_heading_is_one_group() {
        assert!(group_by(Vec::<u8>::new(), |_| "x".to_owned()).is_empty());
        assert_eq!(group_by([1, 2, 3], |_| "all".to_owned()), [("all".to_owned(), vec![1, 2, 3])]);
    }
}
