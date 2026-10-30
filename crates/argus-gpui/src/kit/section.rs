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

/// The section head of a list column (`project/Kit.dc.html`, "Section head"): 28px, the title in
/// 11px capitals muted, its count beside it, and `action` (a text action such as "Stage all", see
/// [`action`]) at the right. Eight pixels of air above it set it off from the rows before.
pub fn head(id: impl Into<ElementId>, title: impl Into<SharedString>, count: Option<usize>, action: Option<gpui_kit::AnyElement>, tokens: &Tokens) -> crate::kit::icon_button::Node {
    let title = title.into();
    h_flex()
        .id(id)
        .test_support()
        .aria_label(title.clone())
        .flex_shrink_0()
        .h(rem(tokens.sp_24 + tokens.sp_4))
        .mt(rem(tokens.sp_8))
        .px(rem(14.0))
        .gap(rem(tokens.sp_8))
        .items_center()
        .typed(tokens.type_label)
        .text_color(tokens.fg_muted)
        .child(div().min_w_0().truncate().child(SharedString::from(title.to_uppercase())))
        .children(count.map(|n| div().flex_none().weighted(tokens.weight_medium).child(SharedString::from(n.to_string()))))
        .child(div().flex_1())
        .children(action)
}

/// The text action at a section head's end: muted, the text colour under the pointer.
pub fn action(id: impl Into<ElementId>, words: impl Into<SharedString>, tokens: &Tokens) -> crate::kit::icon_button::Node {
    let (text, over) = (tokens.fg, tokens.fg);
    let words = words.into();
    h_flex()
        .id(id)
        .role(gpui_kit::Role::Button)
        .test_support()
        .aria_label(words.clone())
        .flex_none()
        .items_center()
        .text_size(rem(tokens.fs_sm))
        .weighted(tokens.weight_medium)
        .text_color(tokens.fg_muted)
        .cursor_pointer()
        .hover(move |style| style.text_color(over))
        .active(move |style| style.text_color(text))
        .child(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::host;
    use gpui_kit::TestAppContext;

    /// Kit.dc.html: the head is 28px and its action sits at the right edge, past the count.
    #[gpui_kit::test]
    fn a_section_head_is_28px_with_its_action_at_the_end(cx: &mut TestAppContext) {
        let handle = host::open(cx, 260.0, |tokens| {
            div().w(rem(260.0)).child(head("h", "Changed", Some(2), Some(action("all", "Stage all", tokens).into_any_element()), tokens)).into_any_element()
        });
        let (left, _, width, height) = host::bounds(cx, handle, "h");
        let (a_left, _, a_width, _) = host::bounds(cx, handle, "all");
        assert_eq!(height, 28.0);
        assert!((left + width - 14.0 - (a_left + a_width)).abs() < 1.0);
    }

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
