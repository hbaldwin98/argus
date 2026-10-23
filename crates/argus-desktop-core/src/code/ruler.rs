//! Where the change ruler puts things, in the measures of whatever drew the rows: a mark at the
//! rows of its change, the band of the file on screen, and how far a press or a drag of the ruler
//! scrolls the file (`code/stage.ts`, `placeMarks`, `trackView`, `grab`).
//!
//! The ruler is the file's scrollbar. Its marks and band are measured off the rows as they are
//! drawn, so a removal marker or a comment box between lines moves them together rather than
//! sliding one past the other; this is only the arithmetic, so a window with its own units asks
//! the same questions. A diff's ruler (the Changes pane's) marks each run of changed rows instead of
//! the runs the session sends, as the prototype's `ruler` does.

use crate::connection::page::{DiffRow, DiffSide};

/// A stretch of the ruler.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub top: f32,
    pub height: f32,
}

/// How much of the file one unit of the ruler is: the ruler's height over the content's, nothing
/// for no content.
pub fn scale(ruler: f32, content: f32) -> f32 {
    if content > 0.0 { ruler / content } else { 0.0 }
}

/// The mark for rows from `first_top` to `last_bottom` of the content, at least `min` tall so a one-line
/// change in a long file can still be seen and pressed.
pub fn mark(first_top: f32, last_bottom: f32, scale: f32, min: f32) -> Span {
    Span { top: first_top * scale, height: ((last_bottom - first_top) * scale).max(min) }
}

/// The part of the file on screen: `scrolled` from the top, `viewport` tall, never taller than the
/// ruler and at least `min`.
pub fn band(scrolled: f32, viewport: f32, ruler: f32, content: f32, min: f32) -> Span {
    let scale = scale(ruler, content);
    Span { top: scrolled * scale, height: (viewport * scale).min(ruler).max(min) }
}

/// Where a press of the ruler holds the band: on it, at the point pressed; elsewhere, by its middle,
/// which the band is first centred on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hold {
    pub on_band: bool,
    /// How far down the band the pointer is held.
    pub offset: f32,
}

/// `press` is the pointer's place from the ruler's top.
pub fn grab(press: f32, band: Span) -> Hold {
    let on_band = press >= band.top && press <= band.top + band.height;
    Hold { on_band, offset: if on_band { press - band.top } else { band.height / 2.0 } }
}

/// How far the file is scrolled with the pointer at `pointer` from the ruler's top, holding the band
/// at `hold`; the file stops where its last line meets the foot of the viewport.
pub fn scrolled_to(pointer: f32, hold: Hold, ruler: f32, content: f32, viewport: f32) -> f32 {
    if ruler <= 0.0 {
        return 0.0;
    }
    ((pointer - hold.offset) * content / ruler).clamp(0.0, (content - viewport).max(0.0))
}

/// The title of a mark, and what it is called to a reader of the page.
pub fn mark_title(kind: &str, line: u32) -> (String, String) {
    if kind == "hit" {
        let said = format!("A match at line {line}");
        (said.clone(), said)
    } else {
        (format!("Changed at line {line}"), format!("A change at line {line}"))
    }
}

/// A run of a diff's changed rows, by the rows' numbers: `added`, `removed`, or `mixed` when it
/// holds both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub first: usize,
    pub last: usize,
    pub kind: &'static str,
}

/// Each run of changed rows in a diff, in order: a line added or removed, or a pair. A context
/// line, a hunk's header, a fold or a note ends a run.
pub fn runs(rows: &[DiffRow]) -> Vec<Run> {
    let changed = |side: &Option<DiffSide>| side.as_ref().map(|side| side.kind).filter(|kind| *kind != "context");
    let mut out: Vec<Run> = Vec::new();
    let mut open = false;
    for row in rows {
        let kinds = [changed(&row.left), changed(&row.right)];
        let (added, removed) = (kinds.contains(&Some("added")), kinds.contains(&Some("removed")));
        if !matches!(row.kind, "line" | "pair") || !(added || removed) {
            open = false;
            continue;
        }
        let kind = if added && removed { "mixed" } else if added { "added" } else { "removed" };
        match out.last_mut().filter(|_| open) {
            Some(run) => {
                run.last = row.row;
                if run.kind != kind {
                    run.kind = "mixed";
                }
            }
            None => out.push(Run { first: row.row, last: row.row, kind }),
        }
        open = true;
    }
    out
}

/// The words for a diff's run: its title, and what a screen reader says.
pub fn run_title(kind: &str) -> &'static str {
    match kind {
        "added" => "Lines added",
        "removed" => "Lines removed",
        _ => "Lines changed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(row: usize, kind: &'static str, left: Option<&'static str>, right: Option<&'static str>) -> DiffRow {
        let side = |kind: &'static str| DiffSide { old: None, new: None, marker: "", kind, parts: Vec::new() };
        DiffRow { row, kind, text: String::new(), hidden: None, step: None, left: left.map(side), right: right.map(side), cursor: false, picked: false }
    }

    /// The prototype's `ruler`: changed rows next to each other are one mark, of both kinds when
    /// they hold both, and anything else between them parts them.
    #[test]
    fn changed_rows_next_to_each_other_are_one_run_and_anything_else_parts_them() {
        let rows = [
            row(0, "hunk", None, None),
            row(1, "line", Some("context"), None),
            row(2, "line", Some("removed"), None),
            row(3, "line", Some("added"), None),
            row(4, "line", Some("context"), None),
            row(5, "line", Some("added"), None),
            row(6, "fold", None, None),
            row(7, "pair", Some("removed"), Some("added")),
            row(8, "line", Some("removed"), None),
        ];
        assert_eq!(
            runs(&rows),
            [Run { first: 2, last: 3, kind: "mixed" }, Run { first: 5, last: 5, kind: "added" }, Run { first: 7, last: 8, kind: "mixed" }]
        );
        assert_eq!(runs(&[row(0, "line", Some("removed"), None), row(1, "line", Some("removed"), None)]), [Run { first: 0, last: 1, kind: "removed" }]);
    }

    /// CODE-43: a mark is its rows' place scaled to the ruler, at least as tall as the minimum.
    #[test]
    fn a_mark_is_the_rows_place_scaled_and_never_thinner_than_the_minimum() {
        let s = scale(100.0, 1000.0);
        assert_eq!(mark(200.0, 300.0, s, 3.0), Span { top: 20.0, height: 10.0 });
        assert_eq!(mark(200.0, 220.0, s, 3.0), Span { top: 20.0, height: 3.0 });
        assert_eq!(scale(100.0, 0.0), 0.0, "an empty file has nothing to scale");
    }

    /// CODE-43: the band is what is on screen, in the ruler's scale.
    #[test]
    fn the_band_is_the_part_on_screen_and_fills_the_ruler_when_all_of_it_is() {
        assert_eq!(band(500.0, 250.0, 100.0, 1000.0, 12.0), Span { top: 50.0, height: 25.0 });
        assert_eq!(band(0.0, 400.0, 100.0, 300.0, 12.0), Span { top: 0.0, height: 100.0 }, "never taller than the ruler");
        assert_eq!(band(0.0, 10.0, 100.0, 1000.0, 12.0).height, 12.0, "a handle that can be held");
    }

    /// CODE-44: a press on the band holds it there; a press elsewhere centres it first.
    #[test]
    fn a_press_on_the_band_holds_it_there_and_a_press_off_it_centres_it() {
        let band = Span { top: 40.0, height: 20.0 };
        assert_eq!(grab(45.0, band), Hold { on_band: true, offset: 5.0 });
        assert_eq!(grab(40.0, band), Hold { on_band: true, offset: 0.0 }, "its edge is on it");
        assert_eq!(grab(10.0, band), Hold { on_band: false, offset: 10.0 });
    }

    #[test]
    fn dragging_scrolls_the_file_by_the_ratio_and_stops_at_its_ends() {
        let hold = Hold { on_band: true, offset: 5.0 };
        assert_eq!(scrolled_to(55.0, hold, 100.0, 1000.0, 250.0), 500.0);
        assert_eq!(scrolled_to(-30.0, hold, 100.0, 1000.0, 250.0), 0.0);
        assert_eq!(scrolled_to(500.0, hold, 100.0, 1000.0, 250.0), 750.0, "the last line at the foot");
        assert_eq!(scrolled_to(5.0, hold, 0.0, 1000.0, 250.0), 0.0, "no ruler, no scroll");
    }

    #[test]
    fn a_mark_is_titled_by_what_it_is() {
        assert_eq!(mark_title("hit", 9), ("A match at line 9".into(), "A match at line 9".into()));
        assert_eq!(mark_title("added", 3), ("Changed at line 3".into(), "A change at line 3".into()));
        assert_eq!(mark_title("removed", 3).1, "A change at line 3");
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // CODE-43: whatever is on screen, its band lies within the ruler it is drawn on.
            #[test]
            fn the_band_stays_inside_the_ruler(content in 1.0f32..1e6, viewport in 1.0f32..5000.0, share in 0.0f32..=1.0, ruler in 50.0f32..2000.0) {
                let scrolled = (content - viewport).max(0.0) * share;
                let band = band(scrolled, viewport, ruler, content, 12.0);
                prop_assert!(band.top >= 0.0);
                prop_assert!(band.top + band.height <= ruler + 12.0 + 0.01, "{band:?} on {ruler}");
            }

            // CODE-44: a drag never scrolls past the file's ends.
            #[test]
            fn a_drag_never_scrolls_past_the_file(pointer in -1e4f32..1e4, offset in 0.0f32..100.0, content in 1.0f32..1e6, viewport in 1.0f32..5000.0) {
                let at = scrolled_to(pointer, Hold { on_band: true, offset }, 400.0, content, viewport);
                prop_assert!(at >= 0.0 && at <= (content - viewport).max(0.0));
            }
        }
    }
}
