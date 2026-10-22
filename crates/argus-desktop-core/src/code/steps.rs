//! The change marks of the file and how `]c` and Alt-] step between them: the lines this version
//! changed or removed lines before, and the next or previous run of them from a line
//! (`ts:code/editor.ts` `changeMarks` and `stepChange`, EDITOR-GUTTER-1, -4, -8, -9).

use super::stage::CodeRow;

/// What a marked line says of the version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The version added or changed this line: a bar the line's height.
    Added,
    /// Lines were removed just above this one: a short bar at its top.
    Removed,
}

/// A marked line, from one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub line: usize,
    pub kind: Kind,
}

/// The marks the page's lines say, a line by its place in the list. A line both added and with
/// lines removed before it draws only the added mark.
pub fn marks(lines: &[CodeRow]) -> Vec<Mark> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(at, row)| {
            let kind = if row.added {
                Kind::Added
            } else if row.removed_before > 0 {
                Kind::Removed
            } else {
                return None;
            };
            Some(Mark { line: at + 1, kind })
        })
        .collect()
}

/// The first line of the next run of marked lines after `here`, or of the last one before it. A
/// run of consecutive marked lines is one change, stepped to at its first line, so stepping back
/// from inside a run goes to that run's first line. `marked` is the marked lines, ascending.
pub fn step(marked: &[usize], here: usize, forward: bool) -> Option<usize> {
    let mut target = None;
    let mut previous = None;
    for &line in marked {
        let inside = previous.is_some_and(|p: usize| line == p + 1);
        previous = Some(line);
        if inside {
            continue;
        }
        if forward && line > here && target.is_none() {
            target = Some(line);
        }
        if !forward && line < here {
            target = Some(line);
        }
    }
    target
}

#[cfg(test)]
mod tests;
