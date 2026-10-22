//! How wide the Code stage's columns are, from the rules the stylesheet gives them: the page's
//! `.code-tree` and `.code-outline` with their container queries, the narrow-window rule and the
//! widths the person dragged them to (`--code-tree-w`, `--code-outline-w`).
//!
//! The rules are applied in the order the stylesheet orders them, since a later one of the same
//! weight wins: the usual width, the stage's own ladder, and last the narrow window's cap. The
//! results column is wider while a search is up, and with no file open it takes what it may.

use crate::theme::scale::Scale;

/// What the stage is, for its columns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// The width of the stage, in the page's pixels (the container the queries ask).
    pub stage: f32,
    /// The window is narrow (`@media (max-width: 1200px)`).
    pub narrow: bool,
    /// What the person dragged the files column and the outline to, as Rust holds them.
    pub dragged_tree: Option<u32>,
    pub dragged_outline: Option<u32>,
    /// The search is up in the files column, and no file is open beside it.
    pub searching: bool,
    pub alone: bool,
}

/// How the files column takes its room.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tree {
    /// A width, at most this share of the stage.
    Fixed { width: f32, at_most: f32 },
    /// What a long line of results needs, up to this width: the empty stage keeps the rest.
    Alone { at_most: f32 },
}

pub fn tree(frame: Frame, t: &Scale) -> Tree {
    let dragged = frame.dragged_tree.map(|w| w as f32);
    if frame.searching {
        return if frame.alone {
            Tree::Alone { at_most: t.code_search_alone_max }
        } else {
            Tree::Fixed { width: dragged.unwrap_or(t.code_tree_search_w), at_most: t.code_tree_search_share }
        };
    }
    let mut width = dragged.unwrap_or(t.code_tree_w);
    if frame.stage <= t.code_mid_below {
        width = dragged.unwrap_or(t.code_tree_mid);
    }
    if frame.stage <= t.code_low_below {
        width = dragged.unwrap_or(t.code_tree_low);
    }
    if frame.narrow {
        width = dragged.unwrap_or(t.code_tree_w).min(t.code_tree_narrow);
    }
    Tree::Fixed { width, at_most: t.code_tree_share }
}

/// The outline's width and the most of the stage it may take, or `None` where the stage is too
/// narrow to hold it.
pub fn outline(frame: Frame, t: &Scale) -> Option<(f32, f32)> {
    if frame.stage <= t.code_outline_hides_below {
        return None;
    }
    let dragged = frame.dragged_outline.map(|w| w as f32);
    let mut width = dragged.unwrap_or(t.code_outline_w);
    if frame.stage <= t.code_mid_below {
        width = dragged.unwrap_or(t.code_outline_mid);
    }
    if frame.narrow {
        width = dragged.unwrap_or(t.code_outline_w).min(t.code_outline_narrow);
    }
    Some((width, t.code_outline_share))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::themes::ledger;

    fn frame(stage: f32) -> Frame {
        Frame { stage, narrow: false, dragged_tree: None, dragged_outline: None, searching: false, alone: false }
    }

    fn tokens() -> Scale {
        ledger::tokens().scale
    }

    /// CODE-11: the usual width and what the stage narrows it to.
    #[test]
    fn the_tree_narrows_with_the_stage() {
        let t = tokens();
        let at = |stage| match tree(frame(stage), &t) {
            Tree::Fixed { width, .. } => width,
            Tree::Alone { .. } => panic!("not searching"),
        };
        assert_eq!((at(1400.0), at(1100.0), at(1000.0), at(640.0), at(500.0)), (288.0, 240.0, 240.0, 180.0, 180.0));
    }

    /// The window's own narrowness wins over the stage's ladder, as it is the later rule.
    #[test]
    fn a_narrow_window_caps_the_tree_at_the_narrow_width() {
        let t = tokens();
        let narrow = Frame { narrow: true, ..frame(500.0) };
        assert_eq!(tree(narrow, &t), Tree::Fixed { width: 240.0, at_most: 0.45 });
    }

    /// CODE-11: a width the person set stands against every rule but the cap on the stage.
    #[test]
    fn a_dragged_width_stands_whatever_the_stage_is() {
        let t = tokens();
        let dragged = Frame { dragged_tree: Some(500), ..frame(600.0) };
        assert_eq!(tree(dragged, &t), Tree::Fixed { width: 500.0, at_most: 0.45 });
        let narrow = Frame { narrow: true, ..dragged };
        assert_eq!(tree(narrow, &t), Tree::Fixed { width: 240.0, at_most: 0.45 }, "even dragged, a narrow window caps it");
    }

    /// CODE-26 and the page's `.code-tree.searching`: results want room.
    #[test]
    fn a_search_is_wider_and_with_no_file_open_takes_what_it_may() {
        let t = tokens();
        let searching = Frame { searching: true, ..frame(1400.0) };
        assert_eq!(tree(searching, &t), Tree::Fixed { width: 400.0, at_most: 0.55 });
        assert_eq!(tree(Frame { dragged_tree: Some(520), ..searching }, &t), Tree::Fixed { width: 520.0, at_most: 0.55 });
        assert_eq!(tree(Frame { alone: true, ..searching }, &t), Tree::Alone { at_most: 720.0 });
    }

    /// CODE-11: the outline is gone from a stage 900 wide or less.
    #[test]
    fn the_outline_gives_way_to_the_file() {
        let t = tokens();
        assert_eq!(outline(frame(1400.0), &t), Some((232.0, 0.4)));
        assert_eq!(outline(frame(1100.0), &t), Some((200.0, 0.4)));
        assert_eq!(outline(frame(901.0), &t), Some((200.0, 0.4)));
        assert_eq!(outline(frame(900.0), &t), None);
        assert_eq!(outline(Frame { narrow: true, dragged_outline: Some(400), ..frame(1300.0) }, &t), Some((184.0, 0.4)));
        assert_eq!(outline(Frame { dragged_outline: Some(400), ..frame(1300.0) }, &t), Some((400.0, 0.4)));
    }
}
