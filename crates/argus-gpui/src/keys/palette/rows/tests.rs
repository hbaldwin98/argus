use super::*;

/// SHELL-PALETTE-7: a run of marked characters is one stretch of the text.
#[test]
fn neighbouring_marks_are_one_range_and_the_rest_are_apart() {
    assert_eq!(ranges("Go to the Workspace", &[0, 1, 6]), [0..2, 6..7]);
    assert_eq!(ranges("abc", &[]), Vec::<Range<usize>>::new());
    assert_eq!(ranges("abc", &[0, 1, 2]), std::iter::once(0..3).collect::<Vec<_>>());
}

/// The search counts characters, a font draws bytes.
#[test]
fn marks_are_characters_and_ranges_are_bytes() {
    assert_eq!(ranges("\u{e9}\u{e9}e", &[2]), std::iter::once(4..5).collect::<Vec<_>>());
    assert_eq!(ranges("\u{e9}\u{e9}e", &[1, 2]), std::iter::once(2..5).collect::<Vec<_>>());
    assert_eq!(ranges("ab", &[7]), Vec::<Range<usize>>::new(), "a mark past the end is nothing");
}

#[test]
fn every_kind_of_row_has_a_glyph() {
    for mark in [
        Mark::Workspace,
        Mark::List,
        Mark::Branch,
        Mark::Code,
        Mark::Console,
        Mark::Search,
        Mark::Keyboard,
        Mark::Gear,
        Mark::Plus,
        Mark::Terminal,
        Mark::Puzzle,
        Mark::Chevron,
    ] {
        let _ = glyph(mark);
    }
    assert_eq!(glyph(Mark::Chevron), Glyph::Right, "a command with no mark of its own has the chevron");
    assert_eq!(glyph(Mark::Puzzle), Glyph::Puzzle);
}
